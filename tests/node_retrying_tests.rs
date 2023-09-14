use async_channel::{bounded, Receiver, Sender};
use shinkai_message_primitives::schemas::agents::serialized_agent::{AgentAPIModel, OpenAI, SerializedAgent};
use shinkai_message_primitives::schemas::inbox_name::InboxName;
use shinkai_message_primitives::schemas::shinkai_name::ShinkaiName;
use shinkai_message_primitives::shinkai_message::shinkai_message_schemas::{JobMessage, MessageSchemaType};
use shinkai_message_primitives::shinkai_utils::encryption::{
    clone_static_secret_key, encryption_public_key_to_string, unsafe_deterministic_encryption_keypair, EncryptionMethod,
};
use shinkai_message_primitives::shinkai_utils::shinkai_message_builder::ShinkaiMessageBuilder;
use shinkai_message_primitives::shinkai_utils::signatures::{
    clone_signature_secret_key, unsafe_deterministic_signature_keypair,
};
use shinkai_message_primitives::shinkai_utils::utils::hash_string;
use shinkai_node::agent::agent;
use shinkai_node::network::node::NodeCommand;
use shinkai_node::network::node_api::APIError;
use shinkai_node::network::Node;
use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::{net::SocketAddr, time::Duration};
use tokio::runtime::Runtime;

mod utils;
use crate::utils::node_test_api::{
    api_agent_registration, api_create_job, api_message_job, api_registration_device_node_profile_main,
};
use crate::utils::node_test_local::local_registration_profile_node;

#[test]
fn setup() {
    let path = Path::new("db_tests/");
    let _ = fs::remove_dir_all(&path);
}

#[test]
fn node_retrying_test() {
    setup();
    let rt = Runtime::new().unwrap();

    rt.block_on(async {
        let node1_identity_name = "@@node1_test.shinkai";
        let node2_identity_name = "@@node2_test.shinkai";
        let node1_profile_name = "main";
        let node1_device_name = "node1_device";

        let node2_profile_name = "main_profile_node2";

        let (node1_identity_sk, node1_identity_pk) = unsafe_deterministic_signature_keypair(0);
        let (node1_encryption_sk, node1_encryption_pk) = unsafe_deterministic_encryption_keypair(0);
        let node1_encryption_sk_clone = node1_encryption_sk.clone();
        let node1_encryption_sk_clone2 = node1_encryption_sk.clone();

        let (node2_identity_sk, node2_identity_pk) = unsafe_deterministic_signature_keypair(1);
        let (node2_encryption_sk, node2_encryption_pk) = unsafe_deterministic_encryption_keypair(1);
        let node2_encryption_sk_clone = node2_encryption_sk.clone();

        let node1_identity_sk_clone = clone_signature_secret_key(&node1_identity_sk);
        let node2_identity_sk_clone = clone_signature_secret_key(&node2_identity_sk);

        let (node1_profile_identity_sk, node1_profile_identity_pk) = unsafe_deterministic_signature_keypair(100);
        let (node1_profile_encryption_sk, node1_profile_encryption_pk) = unsafe_deterministic_encryption_keypair(100);

        let (node2_subidentity_sk, node2_subidentity_pk) = unsafe_deterministic_signature_keypair(101);
        let (node2_subencryption_sk, node2_subencryption_pk) = unsafe_deterministic_encryption_keypair(101);

        let node1_subencryption_sk_clone = node1_profile_encryption_sk.clone();
        let node2_subencryption_sk_clone = node2_subencryption_sk.clone();

        let node1_subidentity_sk_clone = clone_signature_secret_key(&node1_profile_identity_sk);
        let node2_subidentity_sk_clone = clone_signature_secret_key(&node2_subidentity_sk);

        let (node1_device_identity_sk, node1_device_identity_pk) = unsafe_deterministic_signature_keypair(200);
        let (node1_device_encryption_sk, node1_device_encryption_pk) = unsafe_deterministic_encryption_keypair(200);

        let (node1_commands_sender, node1_commands_receiver): (Sender<NodeCommand>, Receiver<NodeCommand>) =
            bounded(100);
        let (node2_commands_sender, node2_commands_receiver): (Sender<NodeCommand>, Receiver<NodeCommand>) =
            bounded(100);

        let node1_db_path = format!("db_tests/{}", hash_string(node1_identity_name.clone()));
        let node2_db_path = format!("db_tests/{}", hash_string(node2_identity_name.clone()));

        // Create node1 and node2
        let addr1 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8080);
        let mut node1 = Node::new(
            node1_identity_name.to_string(),
            addr1,
            node1_identity_sk,
            node1_encryption_sk,
            0,
            node1_commands_receiver,
            node1_db_path,
        );

        let addr2 = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 8081);
        let mut node2 = Node::new(
            node2_identity_name.to_string(),
            addr2,
            node2_identity_sk,
            node2_encryption_sk,
            0,
            node2_commands_receiver,
            node2_db_path,
        );

        eprintln!("Starting nodes");
        // Start node1 and node2
        let node1_handler = tokio::spawn(async move {
            eprintln!("\n\n");
            eprintln!("Starting node 1");
            let _ = node1.await.start().await;
        });

        let node2_handler = tokio::spawn(async move {
            eprintln!("\n\n");
            eprintln!("Starting node 2");
            let _ = node2.await.start().await;
        });

        let interactions_handler = tokio::spawn(async move {
            eprintln!("Starting interactions");
            eprintln!("Registration of Subidentities");

            // Register a Profile in Node1 and verifies it
            {
                eprintln!("Register a Device with main profile in Node1 and verify it");
                api_registration_device_node_profile_main(
                    node1_commands_sender.clone(),
                    node1_profile_name,
                    node1_identity_name,
                    node1_encryption_pk.clone(),
                    node1_device_encryption_sk.clone(),
                    clone_signature_secret_key(&node1_device_identity_sk),
                    node1_profile_encryption_sk.clone(),
                    clone_signature_secret_key(&node1_profile_identity_sk),
                    node1_device_name,
                )
                .await;
            }

            // Register a Profile in Node2 and verifies it
            {
                eprintln!("Register a Profile in Node2 and verify it");
                local_registration_profile_node(
                    node2_commands_sender.clone(),
                    node2_profile_name,
                    node2_identity_name,
                    node2_subencryption_sk_clone.clone(),
                    node2_encryption_pk,
                    clone_signature_secret_key(&node2_subidentity_sk),
                    1,
                )
                .await;
            }

            // Send message from Node 2 subidentity to Node 1
            {
                eprintln!("\n\n### Sending message from a node 2 profile to node 1 profile\n\n");

                let message_content = "test body content".to_string();
                let unchanged_message = ShinkaiMessageBuilder::new(
                    node2_subencryption_sk.clone(),
                    clone_signature_secret_key(&node2_subidentity_sk),
                    node1_encryption_pk,
                )
                .message_raw_content(message_content.clone())
                .no_body_encryption()
                .message_schema_type(MessageSchemaType::TextContent)
                .internal_metadata(
                    node2_profile_name.to_string().clone(),
                    node1_profile_name.to_string(),
                    EncryptionMethod::DiffieHellmanChaChaPoly1305,
                )
                .external_metadata_with_other(
                    node1_identity_name.to_string(),
                    node2_identity_name.to_string().clone(),
                    encryption_public_key_to_string(node2_subencryption_pk.clone()),
                )
                .build()
                .unwrap();

                eprintln!("\n\n unchanged message: {:?}", unchanged_message);

                // Shutdown Node 1
                node1_commands_sender.send(NodeCommand::Shutdown).await.unwrap();

                let (res_send_msg_sender, res_send_msg_receiver): (
                    async_channel::Sender<Result<(), APIError>>,
                    async_channel::Receiver<Result<(), APIError>>,
                ) = async_channel::bounded(1);

                node2_commands_sender
                    .send(NodeCommand::SendOnionizedMessage {
                        msg: unchanged_message,
                        res: res_send_msg_sender,
                    })
                    .await
                    .unwrap();

                let send_result = res_send_msg_receiver.recv().await.unwrap();
                eprintln!("send_result: {:?}", send_result);
                assert!(send_result.is_ok(), "Failed to send onionized message");
                tokio::time::sleep(Duration::from_secs(1)).await;

                // Get Node2 messages
                let (res2_sender, res2_receiver) = async_channel::bounded(1);
                node2_commands_sender
                    .send(NodeCommand::FetchLastMessages {
                        limit: 2,
                        res: res2_sender,
                    })
                    .await
                    .unwrap();
                let node2_last_messages = res2_receiver.recv().await.unwrap();
            }
        });

        let _ = tokio::try_join!(node1_handler, node2_handler, interactions_handler).unwrap();
    });
}
