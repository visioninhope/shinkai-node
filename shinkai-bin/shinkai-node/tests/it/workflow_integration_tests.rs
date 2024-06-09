use async_channel::{bounded, Receiver, Sender};
use shinkai_message_primitives::schemas::agents::serialized_agent::{
    AgentLLMInterface, Ollama, OpenAI, SerializedAgent, ShinkaiBackend,
};
use shinkai_message_primitives::schemas::inbox_name::InboxName;
use shinkai_message_primitives::schemas::shinkai_name::ShinkaiName;
use shinkai_message_primitives::shinkai_message::shinkai_message_schemas::JobMessage;
use shinkai_message_primitives::shinkai_utils::encryption::{
    clone_static_secret_key, unsafe_deterministic_encryption_keypair,
};
use shinkai_message_primitives::shinkai_utils::shinkai_logging::{
    init_default_tracing, shinkai_log, ShinkaiLogLevel, ShinkaiLogOption,
};
use shinkai_message_primitives::shinkai_utils::shinkai_message_builder::ShinkaiMessageBuilder;
use shinkai_message_primitives::shinkai_utils::signatures::{
    clone_signature_secret_key, unsafe_deterministic_signature_keypair,
};
use shinkai_node::network::node::NodeCommand;
use shinkai_node::network::Node;
use shinkai_vector_resources::utils::hash_string;
use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::{net::SocketAddr, time::Duration};
use tokio::runtime::Runtime;

use super::utils::node_test_api::{api_create_job, api_message_job, api_registration_device_node_profile_main};

use mockito::Server;

fn setup() {
    let path = Path::new("db_tests/");
    let _ = fs::remove_dir_all(path);
}

#[test]
fn workflow_integration_test() {
    std::env::set_var("WELCOME_MESSAGE", "false");
    init_default_tracing();

    // WIP: need to find a way to test the agent registration
    setup();
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let node1_identity_name = "@@node1_test.arb-sep-shinkai";
        let node1_subidentity_name = "main";
        let node1_device_name = "node1_device";
        let node1_agent = "node1_gpt_agent";

        let (node1_identity_sk, _node1_identity_pk) = unsafe_deterministic_signature_keypair(0);
        let (node1_encryption_sk, node1_encryption_pk) = unsafe_deterministic_encryption_keypair(0);

        let (node1_commands_sender, node1_commands_receiver): (Sender<NodeCommand>, Receiver<NodeCommand>) =
            bounded(100);

        let (node1_profile_identity_sk, _node1_profile_identity_pk) = unsafe_deterministic_signature_keypair(100);
        let (node1_profile_encryption_sk, _node1_profile_encryption_pk) = unsafe_deterministic_encryption_keypair(100);

        let (node1_device_identity_sk, _node1_device_identity_pk) = unsafe_deterministic_signature_keypair(200);
        let (node1_device_encryption_sk, _node1_device_encryption_pk) = unsafe_deterministic_encryption_keypair(200);

        let node1_db_path = format!("db_tests/{}", hash_string(node1_identity_name));
        let node1_fs_db_path = format!("db_tests/vector_fs{}", hash_string(node1_identity_name));

        // Agent pre-creation

        let mut server = Server::new();
        let _m = server
            .mock("POST", "/v1/chat/completions")
            .match_header("authorization", "Bearer mockapikey")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(
                r#"{
            "id": "chatcmpl-123",
            "object": "chat.completion",
            "created": 1677652288,
            "choices": [{
                "index": 0,
                "message": {
                    "role": "assistant",
                    "content": "\n# Answer \n The Roman Empire is very interesting"
                },
                "finish_reason": "stop"
            }],
            "usage": {
                "prompt_tokens": 9,
                "completion_tokens": 12,
                "total_tokens": 21
            }
        }"#,
            )
            .create();

        let agent_name = ShinkaiName::new(
            format!(
                "{}/{}/agent/{}",
                node1_identity_name, node1_subidentity_name, node1_agent
            )
            .to_string(),
        )
        .unwrap();

        let open_ai = OpenAI {
            model_type: "gpt-3.5-turbo-1106".to_string(),
        };

        let agent = SerializedAgent {
            id: node1_agent.to_string(),
            full_identity_name: agent_name,
            perform_locally: false,
            external_url: Some(server.url()),
            api_key: Some("mockapikey".to_string()),
            model: AgentLLMInterface::OpenAI(open_ai),
            toolkit_permissions: vec![],
            storage_bucket_permissions: vec![],
            allowed_message_senders: vec![],
        };

        // Create node1 and node2
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let node1 = Node::new(
            node1_identity_name.to_string(),
            addr1,
            clone_signature_secret_key(&node1_identity_sk),
            node1_encryption_sk.clone(),
            0,
            node1_commands_receiver,
            node1_db_path,
            "".to_string(),
            None,
            true,
            vec![agent],
            None,
            node1_fs_db_path,
            None,
            None,
        );

        let node1_handler = tokio::spawn(async move {
            shinkai_log(ShinkaiLogOption::Tests, ShinkaiLogLevel::Debug, "Starting Node 1");
            let _ = node1.await.lock().await.start().await;
        });

        let abort_handler = node1_handler.abort_handle();

        let interactions_handler = tokio::spawn(async move {
            shinkai_log(
                ShinkaiLogOption::Tests,
                ShinkaiLogLevel::Debug,
                "\n\nRegistration of an Admin Profile",
            );

            {
                // Register a Profile in Node1 and verifies it
                eprintln!("\n\nRegister a Device with main Profile in Node1 and verify it");
                api_registration_device_node_profile_main(
                    node1_commands_sender.clone(),
                    node1_subidentity_name,
                    node1_identity_name,
                    node1_encryption_pk,
                    node1_device_encryption_sk.clone(),
                    clone_signature_secret_key(&node1_device_identity_sk),
                    node1_profile_encryption_sk.clone(),
                    clone_signature_secret_key(&node1_profile_identity_sk),
                    node1_device_name,
                )
                .await;
            }

            #[allow(unused_assignments)]
            let mut job_id = "".to_string();
            let agent_subidentity = format!("{}/agent/{}", node1_subidentity_name, node1_agent).to_string();
            {
                // Create a Job
                shinkai_log(
                    ShinkaiLogOption::Tests,
                    ShinkaiLogLevel::Debug,
                    &format!("Creating a Job for Agent {}", agent_subidentity.clone()),
                );
                job_id = api_create_job(
                    node1_commands_sender.clone(),
                    clone_static_secret_key(&node1_profile_encryption_sk),
                    node1_encryption_pk,
                    clone_signature_secret_key(&node1_profile_identity_sk),
                    node1_identity_name,
                    node1_subidentity_name,
                    &agent_subidentity.clone(),
                )
                .await;
            }
            {
                // Send a Message to the Job for processing
                shinkai_log(
                    ShinkaiLogOption::Api,
                    ShinkaiLogLevel::Debug,
                    &format!("Sending a message to Job {}", job_id.clone()),
                );
                let message = "Run this workflow (this message is not used)".to_string();
                let workflow = r#"
                workflow MyProcess v0.1 {
                    step Initialize {
                        $R1 = ""
                        $R2 = "Tell me about the Economy of the Roman Empire"
                    }
                    step Inference {
                        $R1 = call inference($R2)
                    }
                }
                "#;

                api_message_job(
                    node1_commands_sender.clone(),
                    clone_static_secret_key(&node1_profile_encryption_sk),
                    node1_encryption_pk,
                    clone_signature_secret_key(&node1_profile_identity_sk),
                    node1_identity_name,
                    node1_subidentity_name,
                    &agent_subidentity.clone(),
                    &job_id.clone().to_string(),
                    &message,
                    "",
                    "",
                    Some(workflow.to_string())
                )
                .await;
            }
            {
                let inbox_name = InboxName::get_job_inbox_name_from_params(job_id.clone()).unwrap();
                let sender = format!("{}/{}", node1_identity_name, node1_subidentity_name);

                let mut node2_last_messages = vec![];
                for _ in 0..30 {
                    let msg = ShinkaiMessageBuilder::get_last_messages_from_inbox(
                        clone_static_secret_key(&node1_profile_encryption_sk),
                        clone_signature_secret_key(&node1_profile_identity_sk),
                        node1_encryption_pk,
                        inbox_name.to_string(),
                        10,
                        None,
                        "".to_string(),
                        sender.clone(),
                        node1_identity_name.to_string(),
                    )
                    .unwrap();
                    let (res2_sender, res2_receiver) = async_channel::bounded(1);
                    node1_commands_sender
                        .send(NodeCommand::APIGetLastMessagesFromInbox { msg, res: res2_sender })
                        .await
                        .unwrap();
                    node2_last_messages = res2_receiver.recv().await.unwrap().expect("Failed to receive messages");

                    if node2_last_messages.len() >= 2 {
                        eprintln!("breaking>> node2_last_messages: {:?}", node2_last_messages);
                        break;
                    }

                    tokio::time::sleep(Duration::from_millis(500)).await;
                }

                shinkai_log(
                    ShinkaiLogOption::Tests,
                    ShinkaiLogLevel::Debug,
                    &format!("node2_last_messages: {:?}", node2_last_messages),
                );

                eprintln!("node2_last_messages: {:?}", node2_last_messages);
                let shinkai_message_content_agent = node2_last_messages[1].get_message_content().unwrap();
                let message_content_agent: JobMessage = serde_json::from_str(&shinkai_message_content_agent).unwrap();

                assert_eq!(
                    message_content_agent.content,
                    "The Roman Empire is very interesting".to_string()
                );
                assert!(node2_last_messages.len() == 2);
            }
            {
                // Check Profile inboxes (to confirm job's there)
                let full_profile = format!("{}/{}", node1_identity_name, node1_subidentity_name);

                let msg = ShinkaiMessageBuilder::get_all_inboxes_for_profile(
                    clone_static_secret_key(&node1_profile_encryption_sk),
                    clone_signature_secret_key(&node1_profile_identity_sk),
                    node1_encryption_pk,
                    full_profile.clone().to_string(),
                    node1_subidentity_name.to_string(),
                    node1_identity_name.to_string(),
                    node1_identity_name.to_string(),
                )
                .unwrap();

                let (res2_sender, res2_receiver) = async_channel::bounded(1);
                node1_commands_sender
                    .send(NodeCommand::APIGetAllInboxesForProfile { msg, res: res2_sender })
                    .await
                    .unwrap();
                let node2_last_messages = res2_receiver.recv().await.unwrap().expect("Failed to receive messages");
                // println!("node1_all_profiles: {:?}", node2_last_messages);
                assert!(node2_last_messages.len() == 1);
            }
            {
                // Send a scheduled message
                // let message = "scheduled message".to_string();
                // let inbox_name = InboxName::get_job_inbox_name_from_params(job_id.clone()).unwrap();
                // let sender = format!("{}/{}", node1_identity_name, node1_subidentity_name);
                // let future_time_2_secs = ShinkaiStringTime::generate_time_in_future_with_secs(2);

                // let msg = ShinkaiMessageBuilder::new(
                //     clone_static_secret_key(&node1_profile_encryption_sk),
                //     clone_signature_secret_key(&node1_profile_identity_sk),
                //     node1_encryption_pk,
                // )
                // .body_encryption(EncryptionMethod::DiffieHellmanChaChaPoly1305)
                // .external_metadata_with_schedule(node1_identity_name.to_string(), sender, future_time_2_secs)
                // .message_raw_content(message.clone())
                // .internal_metadata_with_inbox(
                //     "".to_string(),
                //     "".to_string(),
                //     inbox_name.to_string(),
                //     EncryptionMethod::None,
                //     None,
                // )
                // .build();

                abort_handler.abort();
            }
        });

        // Wait for all tasks to complete
        let result = tokio::try_join!(node1_handler, interactions_handler);

        match result {
            Ok(_) => {}
            Err(e) => {
                // Check if the error is because one of the tasks was aborted
                if e.is_cancelled() {
                    println!("One of the tasks was aborted, but this is expected.");
                } else {
                    // If the error is not due to an abort, then it's unexpected
                    panic!("An unexpected error occurred: {:?}", e);
                }
            }
        }
    });
    rt.shutdown_background();
}
