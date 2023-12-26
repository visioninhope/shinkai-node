use ed25519_dalek::SigningKey;
use futures::SinkExt;
use futures::StreamExt;
use shinkai_message_primitives::schemas::inbox_name::InboxName;
use shinkai_message_primitives::shinkai_message::shinkai_message::ShinkaiMessage;
use shinkai_message_primitives::shinkai_message::shinkai_message_schemas::MessageSchemaType;
use shinkai_message_primitives::shinkai_utils::encryption::unsafe_deterministic_encryption_keypair;
use shinkai_message_primitives::shinkai_utils::encryption::EncryptionMethod;
use shinkai_message_primitives::shinkai_utils::job_scope::JobScope;
use shinkai_message_primitives::shinkai_utils::shinkai_message_builder::ShinkaiMessageBuilder;
use shinkai_message_primitives::shinkai_utils::signatures::unsafe_deterministic_signature_keypair;
use shinkai_message_primitives::shinkai_utils::utils::hash_string;
use shinkai_node::db::ShinkaiDB;
use shinkai_node::network::ws_routes::WSMessage;
use shinkai_node::network::{ws_manager::WebSocketManager, ws_routes::run_ws_api};
use std::fs;
use std::path::Path;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_tungstenite::tungstenite;
use x25519_dalek::{PublicKey as EncryptionPublicKey, StaticSecret as EncryptionStaticKey};

fn generate_message_with_text(
    content: String,
    my_encryption_secret_key: EncryptionStaticKey,
    my_signature_secret_key: SigningKey,
    receiver_public_key: EncryptionPublicKey,
    recipient_subidentity_name: String,
    origin_destination_identity_name: String,
    timestamp: String,
) -> ShinkaiMessage {
    let inbox_name = InboxName::get_job_inbox_name_from_params("test_job".to_string()).unwrap();

    let inbox_name_value = match inbox_name {
        InboxName::RegularInbox { value, .. } | InboxName::JobInbox { value, .. } => value,
    };

    let message = ShinkaiMessageBuilder::new(my_encryption_secret_key, my_signature_secret_key, receiver_public_key)
        .message_raw_content(content.to_string())
        .body_encryption(EncryptionMethod::None)
        .message_schema_type(MessageSchemaType::TextContent)
        .internal_metadata_with_inbox(
            "".to_string(),
            recipient_subidentity_name.clone().to_string(),
            inbox_name_value,
            EncryptionMethod::None,
        )
        .external_metadata_with_schedule(
            origin_destination_identity_name.clone().to_string(),
            origin_destination_identity_name.clone().to_string(),
            timestamp,
        )
        .build()
        .unwrap();
    message
}

fn setup() {
    let path = Path::new("db_tests/");
    let _ = fs::remove_dir_all(&path);
}

#[tokio::test]
async fn test_websocket() {
    // Setup
    setup();
    let job_id = "test_job".to_string();
    let agent_id = "agent4".to_string();
    let db_path = format!("db_tests/{}", hash_string(&agent_id.clone()));
    let mut shinkai_db = ShinkaiDB::new(&db_path).unwrap();

    let node1_identity_name = "@@node1.shinkai";
    let node1_subidentity_name = "main_profile_node1";
    let (node1_identity_sk, _) = unsafe_deterministic_signature_keypair(0);
    let (node1_encryption_sk, node1_encryption_pk) = unsafe_deterministic_encryption_keypair(0);

    let agent_id = "agent_test".to_string();
    let scope = JobScope::new_default();

    // Start the WebSocket server
    let manager = Arc::new(Mutex::new(WebSocketManager::new()));
    let ws_address = "127.0.0.1:8080".parse().expect("Failed to parse WebSocket address");
    tokio::spawn(run_ws_api(ws_address, Arc::clone(&manager)));

    // Give the server a little time to start
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Connect to the server
    let (mut ws_stream, _) = tokio_tungstenite::connect_async("ws://127.0.0.1:8080/ws")
        .await
        .expect("Failed to connect");

    // Generate a ShinkaiMessage
    let shinkai_message = generate_message_with_text(
        "Hello, world!".to_string(),
        node1_encryption_sk,
        node1_identity_sk,
        node1_encryption_pk,
        node1_subidentity_name.to_string(),
        node1_identity_name.to_string(),
        "2023-07-02T20:53:34.810Z".to_string(),
    );

    // Send a message to the server to establish the connection and subscribe to a topic
    let ws_message = WSMessage {
        action: "subscribe".to_string(),
        message: shinkai_message,
    };
    let ws_message_json = serde_json::to_string(&ws_message).unwrap();
    ws_stream
        .send(tungstenite::Message::Text(ws_message_json))
        .await
        .expect("Failed to send message");

    // Wait for the server to process the subscription message
    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

    // Send a message to all connections that are subscribed to the topic
    manager
        .lock()
        .await
        .handle_update(
            "topic1".to_string(),
            "some_subtopic".to_string(),
            "Hello, world!".to_string(),
        )
        .await;

    // Check the response
    let msg = ws_stream
        .next()
        .await
        .expect("Failed to read message")
        .expect("Failed to read message");
    assert_eq!(msg.to_text().unwrap(), "Hello, world!");

    // Send a close message
    ws_stream
        .send(tungstenite::Message::Close(None))
        .await
        .expect("Failed to send close message");
}
