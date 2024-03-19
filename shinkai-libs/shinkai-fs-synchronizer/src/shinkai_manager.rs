use crate::{
    communication::{self, generate_encryption_keys, generate_signature_keys},
    persistent::Storage,
};
use ed25519_dalek::SigningKey;
use serde::Deserialize;
use shinkai_message_primitives::{
    shinkai_message::shinkai_message::ShinkaiMessage,
    shinkai_utils::shinkai_message_builder::{ProfileName, ShinkaiMessageBuilder},
};
use std::env;
use std::{convert::TryInto, fs};
use x25519_dalek::StaticSecret;
use x25519_dalek::{PublicKey as EncryptionPublicKey, StaticSecret as EncryptionStaticKey};

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct NodeHealthStatus {
    pub is_pristine: bool,
    pub node_name: String,
    pub status: String,
    pub version: String,
}

#[derive(Clone)]
pub struct ShinkaiManager {
    pub message_builder: ShinkaiMessageBuilder,
    pub my_encryption_secret_key: EncryptionStaticKey,
    pub my_signature_secret_key: SigningKey,
    pub receiver_public_key: EncryptionPublicKey,
    pub sender: ProfileName,
    pub sender_subidentity: String,
    pub node_receiver: ProfileName,
    pub node_receiver_subidentity: ProfileName,
    pub profile_name: ProfileName,
}

impl ShinkaiManager {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        my_encryption_secret_key: EncryptionStaticKey,
        my_signature_secret_key: SigningKey,
        receiver_public_key: EncryptionPublicKey,
        sender: ProfileName,
        sender_subidentity: String,
        node_receiver: ProfileName,
        node_receiver_subidentity: ProfileName,
        profile_name: ProfileName,
    ) -> Self {
        let shinkai_message_builder = ShinkaiMessageBuilder::new(
            my_encryption_secret_key.clone(),
            my_signature_secret_key.clone(),
            receiver_public_key,
        );

        Self {
            message_builder: shinkai_message_builder,
            my_encryption_secret_key,
            my_signature_secret_key,
            receiver_public_key,
            sender,
            sender_subidentity,
            node_receiver,
            node_receiver_subidentity,
            profile_name,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn initialize_node_connection(health_status: NodeHealthStatus) -> anyhow::Result<Self, &'static str> {
        let (profile_signature_sk, profile_signing_key) = generate_signature_keys().await;
        let storage_path = env::var("SHINKAI_STORAGE_PATH").expect("SHINKAI_STORAGE_PATH must be set");
        let local_storage_path = format!("{}/{}", env!("CARGO_MANIFEST_DIR"), storage_path);

        // Ensure the storage directory exists
        fs::create_dir_all(&local_storage_path).expect("Failed to create storage directory");

        let storage = Storage::new(local_storage_path, "node_keys.json".to_string());

        let sender = env::var("PROFILE_NAME").expect("PROFILE_NAME must be set");
        let sender_subidentity = env::var("DEVICE_NAME").expect("DEVICE_NAME must be set");
        let receiver = env::var("PROFILE_NAME").expect("PROFILE_NAME must be set");

        if health_status.is_pristine {
            let (my_device_encryption_sk, my_device_encryption_pk) = generate_encryption_keys().await;
            let (my_device_signature_sk, my_device_signing_key) = generate_signature_keys().await;

            let (profile_encryption_sk, profile_encryption_pk) = generate_encryption_keys().await;
            let (profile_signature_sk, profile_signing_key) = generate_signature_keys().await;

            let my_encryption_secret_key = my_device_encryption_sk.clone();
            let my_signature_secret_key = my_device_signing_key.clone();

            // this one is irrelevant here, because it will be overwritten by the encryption_publick_key from the node
            let receiver_public_key = profile_encryption_pk.clone();

            let shinkai_message_result = ShinkaiMessageBuilder::initial_registration_with_no_code_for_device(
                my_device_encryption_sk.clone(),
                my_device_signing_key.clone(),
                profile_encryption_sk.clone(),
                profile_signing_key.clone(),
                "registration_name".to_string(),
                sender_subidentity.clone(),
                sender.clone(),
                receiver.clone(),
            );

            if shinkai_message_result.is_err() {
                return Err(shinkai_message_result.err().unwrap());
            }

            let shinkai_message = shinkai_message_result.unwrap();
            let shinkai_message_json =
                serde_json::to_string(&shinkai_message).expect("Failed to serialize ShinkaiMessage");
            match communication::request_post(shinkai_message_json, "/v1/use_registration_code").await {
                Ok(response) => {
                    println!("Successfully posted ShinkaiMessage. Response: {:?}", response);

                    let response_data = response.data;
                    let encryption_public_key = response_data["encryption_public_key"]
                        .as_str()
                        .expect("Failed to extract encryption_public_key");

                    let my_encryption_secret_key = my_device_encryption_sk.clone();
                    let my_signature_secret_key = my_device_signing_key.clone();

                    let encryption_public_key_bytes = hex::decode(encryption_public_key).expect("Decoding failed");
                    let receiver_public_key_bytes: [u8; 32] = encryption_public_key_bytes
                        .try_into()
                        .expect("encryption_public_key_bytes with incorrect length");
                    let receiver_public_key = x25519_dalek::PublicKey::from(receiver_public_key_bytes);

                    let _ = storage.write_encryption_secret_key(&my_encryption_secret_key);
                    let _ = storage.write_signature_secret_key(&my_signature_secret_key);
                    let _ = storage.write_receiver_public_key(&receiver_public_key);

                    let shinkai_manager = ShinkaiManager::new(
                        my_encryption_secret_key,
                        my_signature_secret_key,
                        receiver_public_key,
                        ProfileName::default(),
                        String::default(),
                        sender,
                        sender_subidentity,
                        receiver,
                    );

                    // TODO: store keys received from the respone in persistent storage so we can reuse them
                    // TODO: verify if there is better way to do that
                    Ok(shinkai_manager)
                }
                Err(e) => {
                    eprintln!("Failed to post ShinkaiMessage. Error: {}", e);
                    Err("Failed to communicate with the endpoint")
                }
            }
        } else {
            let my_encryption_secret_key = storage.read_encryption_secret_key();
            let my_signature_secret_key = storage.read_signature_secret_key();
            let receiver_public_key = storage.read_receiver_public_key();

            let shinkai_manager = ShinkaiManager::new(
                my_encryption_secret_key,
                my_signature_secret_key,
                receiver_public_key,
                ProfileName::default(),
                String::default(),
                sender,
                sender_subidentity,
                receiver,
            );

            Ok(shinkai_manager)
        }

        // if is pristine, run initial_registration

        // if not pristine, use keys stored in the storage
    }

    pub async fn check_node_health() -> Result<NodeHealthStatus, &'static str> {
        let shinkai_health_url = format!(
            "{}/v1/shinkai_health",
            env::var("SHINKAI_NODE_URL").expect("SHINKAI_NODE_URL must be set")
        );

        match reqwest::get(&shinkai_health_url).await {
            Ok(response) => {
                if response.status().is_success() {
                    let health_data: serde_json::Value =
                        response.json().await.expect("Failed to parse health check response");

                    let health_status: NodeHealthStatus = serde_json::from_value(health_data.clone())
                        .expect("Failed to parse health data into NodeHealthStatusPayload");

                    if health_status.status == "ok" {
                        println!("Shinkai node is healthy.");
                        Ok(health_status)
                    } else {
                        eprintln!("Shinkai node health check failed.");
                        Err("Shinkai node health check failed")
                    }
                } else {
                    eprintln!("Failed to reach Shinkai node for health check.");
                    Err("Failed to reach Shinkai node for health check")
                }
            }
            Err(e) => {
                eprintln!("Error verifying node health. Please check Node configuration and if all is fine, then Shinkai Node itself. \n{}", e);
                Err("Error verifying node health")
            }
        }
    }

    pub async fn get_node_folder(&mut self, path: &str) -> Result<String, &'static str> {
        println!("vecfs_retrieve_path_simplified");
        let shinkai_message = self.message_builder.vecfs_retrieve_path_simplified(
            path,
            self.my_encryption_secret_key.clone(),
            self.my_signature_secret_key.clone(),
            self.receiver_public_key,
            self.sender.clone(),
            self.sender_subidentity.clone(),
            self.node_receiver.clone(),
            self.node_receiver_subidentity.clone(),
        );

        match shinkai_message {
            Ok(shinkai_message) => {
                let decoded_message = self.decode_message(shinkai_message).await;
                // Assuming decodeMessage returns a Result<String, &'static str>, you can directly return its result here
                // If decodeMessage's return type is not a Result, you need to adjust its implementation accordingly
                Ok(decoded_message) // Example conversion, adjust based on actual logic
            }
            Err(e) => Err(e),
        }
    }

    pub fn create_folder(&mut self, folder_name: &str, path: &str) -> Result<(), &'static str> {
        self.message_builder.vecfs_create_folder(
            folder_name,
            path,
            self.my_encryption_secret_key.clone(),
            self.my_signature_secret_key.clone(),
            self.receiver_public_key.clone(),
            self.sender.clone(),
            self.sender_subidentity.clone(),
            self.node_receiver.clone(),
            self.node_receiver_subidentity.clone(),
        )?;

        Ok(())
    }

    // TODO: how to delete folder with files on the node
    // fn delete_folder(&self, folder_name: &str, path: &str) -> Result<(), &'static str> {
    //     self.message_builder.vecfs_delete_folder(
    //         folder_name,
    //         path,
    //         self.my_encryption_secret_key.clone(),
    //         self.my_signature_secret_key.clone(),
    //         self.receiver_public_key.clone(),
    //         self.sender.clone(),
    //         self.sender_subidentity.clone(),
    //         self.node_receiver.clone(),
    //         self.node_receiver_subidentity.clone(),
    //     )?;

    //     Ok(())
    // }

    pub async fn upload_file(&self, file_bytes: &[u8], destination_path: &str) -> Result<(), &'static str> {
        // TODO: add missing pieces here

        // Prepare the file data
        // let file_data = encrypted_file_data; // In Rust, Vec<u8> can be used directly

        // let form_data = multipart::Form::new()
        //     .file("file", file_data, destination_path)
        //     .map_err(|_| "Failed to create form data")?;

        // let url = format!(
        //     "{}/v1/add_file_to_inbox_with_symmetric_key/{}/{}",
        //     self.base_url, hash, nonce_str
        // );

        // TODO: add http service that communicates with the node api
        // self.http_service
        //     .fetch(&url, form_data)
        //     .await
        //     .map_err(|_| "HTTP request failed")?;

        Ok(())
    }

    fn add_items_to_db(&mut self, destination_path: &str, file_inbox: &str) -> Result<(), &'static str> {
        self.message_builder.vecfs_create_items(
            destination_path,
            file_inbox,
            self.my_encryption_secret_key.clone(),
            self.my_signature_secret_key.clone(),
            self.receiver_public_key,
            self.sender.clone(),
            self.sender_subidentity.clone(),
            self.node_receiver.clone(),
            self.node_receiver_subidentity.clone(),
        )?;

        Ok(())
    }

    async fn decode_message(&self, message: ShinkaiMessage) -> String {
        let decrypted_message = message
            .decrypt_outer_layer(&self.my_encryption_secret_key, &self.receiver_public_key)
            .expect("Failed to decrypt body content");

        let content = decrypted_message.get_message_content().unwrap();

        // Deserialize the content into a JSON object
        let content: serde_json::Value = serde_json::from_str(&content).unwrap();
        content.to_string()
    }
}
