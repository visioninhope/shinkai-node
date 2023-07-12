use async_channel::{bounded, Receiver, Sender};
use shinkai_node::network::node::NodeCommand;
use shinkai_node::network::{Node, SubIdentityManager, Subidentity};
use shinkai_node::shinkai_message::encryption::{
    decrypt_body_message, decrypt_content_message, encryption_public_key_to_string, encryption_secret_key_to_string,
    hash_encryption_public_key, unsafe_deterministic_encryption_keypair, EncryptionMethod,
};
use shinkai_node::shinkai_message::shinkai_message_builder::ShinkaiMessageBuilder;
use shinkai_node::shinkai_message::shinkai_message_handler::ShinkaiMessageHandler;
use shinkai_node::shinkai_message::signatures::{
    clone_signature_secret_key, sign_message, signature_public_key_to_string, signature_secret_key_to_string,
    unsafe_deterministic_signature_keypair,
};
use shinkai_node::shinkai_message::utils::hash_string;
use std::fs;
use std::net::{IpAddr, Ipv4Addr};
use std::path::Path;
use std::{net::SocketAddr, time::Duration};
use tokio::runtime::Runtime;

#[test]
fn setup() {
    let path = Path::new("db_tests/");
    let _ = fs::remove_dir_all(&path);
}

#[test]
fn db_inbox() {
    let (node1_subidentity_sk, node1_subidentity_pk) = unsafe_deterministic_signature_keypair(100);
    let (node1_subencryption_sk, node1_subencryption_pk) = unsafe_deterministic_encryption_keypair(100);
    let node1_identity_name = "@@node1.shinkai";

    let node1_db_path = format!("db_tests/{}", hash_string(node1_identity_name.clone()));
}

// #[cfg(test)]
// mod tests {
//     use super::*;
//     use shinkai_node::{shinkai_message_proto::ShinkaiMessage, shinkai_message::{encryption::unsafe_deterministic_private_key, shinkai_message_builder::ShinkaiMessageBuilder}};
//     use prost::Message;
//     use rocksdb::{ColumnFamilyDescriptor, Error, Options, DB};
//     use std::{convert::TryInto, collections::HashMap};
//     // use tempfile::Builder;

//     fn get_test_db_path() -> String {
//         let temp_dir = Builder::new()
//             .prefix("test_db")
//             .rand_bytes(5)
//             .tempdir()
//             .unwrap();
//         temp_dir.into_path().to_str().unwrap().to_string()
//     }

//     fn get_test_message() -> ShinkaiMessage {
//         let (secret_key, public_key) = unsafe_deterministic_private_key(0);

//         // Replace this with actual field data
//         let fields = HashMap::new();

//         // Build the ShinkaiMessage
//         ShinkaiMessageBuilder::new(&secret_key, &public_key)
//             .body("body content".to_string())
//             .encryption("no_encryption".to_string())
//             .message_schema_type("schema type".to_string(), &fields)
//             .topic("topic_id".to_string(), "channel_id".to_string())
//             .internal_metadata_content("internal metadata content".to_string())
//             .external_metadata(&public_key)
//             .build()
//             .unwrap()
//     }

//     #[test]
//     fn test_insert_get() {
//         let db_path = get_test_db_path();
//         let db = ShinkaiMessageDB::new(&db_path).unwrap();
//         let message = get_test_message();

//         // Insert the message in AllMessages topic
//         let key = ShinkaiMessageHandler::calculate_hash(&message);
//         db.insert(key.clone(), &message, Topic::AllMessages).unwrap();

//         // Retrieve the message and validate it
//         let retrieved_message = db.get(key, Topic::AllMessages).unwrap().unwrap();
//         assert_eq!(message, retrieved_message);
//     }

//     #[test]
//     fn test_insert_message() {
//         let db_path = get_test_db_path();
//         let db = ShinkaiMessageDB::new(&db_path).unwrap();
//         let message = get_test_message();

//         // Insert the message
//         db.insert_message(&message).unwrap();

//         // Retrieve the message from AllMessages and validate it
//         let all_messages_key = ShinkaiMessageHandler::calculate_hash(&message);
//         let retrieved_message = db.get(all_messages_key, Topic::AllMessages).unwrap().unwrap();
//         assert_eq!(message, retrieved_message);

//         // Retrieve the pointer from AllMessagesTimeKeyed and validate it
//         let time_keyed_key = if message.scheduled_time.is_empty() {
//             ShinkaiMessageHandler::generate_time_now()
//         } else {
//             message.scheduled_time.clone()
//         };
//         let retrieved_key = db.get(time_keyed_key, Topic::AllMessagesTimeKeyed).unwrap().unwrap();
//         assert_eq!(all_messages_key, retrieved_key);
//     }

//     #[test]
//     fn test_schedule_message() {
//         let db_path = get_test_db_path();
//         let db = ShinkaiMessageDB::new(&db_path).unwrap();
//         let message = get_test_message();

//         // Schedule the message
//         db.schedule_message(&message).unwrap();

//         // Retrieve the scheduled message and validate it
//         let scheduled_key = if message.scheduled_time.is_empty() {
//             ShinkaiMessageHandler::generate_time_now()
//         } else {
//             message.scheduled_time.clone()
//         };
//         let retrieved_message = db.get(scheduled_key, Topic::ScheduledMessage).unwrap().unwrap();
//         assert_eq!(message, retrieved_message);
//     }
// }
