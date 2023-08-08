use shinkai_message_wasm::{
    schemas::shinkai_name::{ShinkaiName, ShinkaiNameError},
    shinkai_message::shinkai_message::ShinkaiMessage,
};

#[derive(PartialEq, Debug)]
pub struct InboxNameManager {
    pub inbox_name: String,
    pub is_e2e: bool,
    pub identities: Vec<ShinkaiName>,
}

impl InboxNameManager {
    pub fn from_inbox_name(inbox_name: String) -> Result<Self, ShinkaiNameError> {
        let parts: Vec<&str> = inbox_name.split("::").collect();
        if parts.len() < 3 || parts.len() > 101 || parts[0] != "inbox" {
            return Err(ShinkaiNameError::InvalidGroupFormat(inbox_name.clone()));
        }

        let is_e2e = match parts.last().unwrap().parse::<bool>() {
            Ok(b) => b,
            Err(_) => return Err(ShinkaiNameError::InvalidGroupFormat(inbox_name.clone())),
        };

        let mut identities = Vec::new();
        for part in &parts[1..parts.len() - 1] {
            if !ShinkaiName::is_fully_valid(part.to_string()) {
                return Err(ShinkaiNameError::InvalidNameFormat(part.to_string()));
            }
            match ShinkaiName::new(part.to_string()) {
                Ok(name) => identities.push(name),
                Err(_) => return Err(ShinkaiNameError::InvalidNameFormat(part.to_string())),
            }
        }

        Ok(InboxNameManager {
            inbox_name,
            is_e2e,
            identities,
        })
    }

    pub fn from_message(message: &ShinkaiMessage) -> Result<InboxNameManager, ShinkaiNameError> {
        let body = message
            .body
            .as_ref()
            .ok_or(ShinkaiNameError::MissingBody(message.to_json_str().unwrap()))?;
        let internal_metadata = body
            .internal_metadata
            .as_ref()
            .ok_or(ShinkaiNameError::MissingInternalMetadata(
                message.to_json_str().unwrap(),
            ))?;

        let inbox_name = internal_metadata.inbox.clone();
        InboxNameManager::from_inbox_name(inbox_name)
    }

    pub fn has_creation_access(&self, identity_name: ShinkaiName) -> bool {
        for identity in &self.identities {
            if identity.contains(&identity_name) {
                return true;
            }
        }
        false
    }

    pub fn has_sender_creation_access(&self, message: ShinkaiMessage) -> bool {
        match ShinkaiName::from_shinkai_message_using_sender(&message) {
            Ok(shinkai_name) => self.has_creation_access(shinkai_name),
            Err(_) => false,
        }
    }

    fn get_inbox_name_from_params(
        sender: String,
        sender_subidentity: String,
        recipient: String,
        recipient_subidentity: String,
        is_e2e: bool,
    ) -> Result<InboxNameManager, ShinkaiNameError> {
        let inbox_name_separator = "::";

        let sender_full = format!("{}/{}", sender, sender_subidentity);
        let recipient_full = format!("{}/{}", recipient, recipient_subidentity);

        let sender_name =
            ShinkaiName::new(sender_full.clone()).map_err(|_| ShinkaiNameError::InvalidNameFormat(sender_full.to_string()))?;
        let recipient_name = ShinkaiName::new(recipient_full.clone())
            .map_err(|_| ShinkaiNameError::InvalidNameFormat(recipient_full.to_string()))?;

        let mut inbox_name_parts = vec![sender_name.to_string(), recipient_name.to_string()];
        inbox_name_parts.sort();

        let inbox_name = format!(
            "inbox{}{}{}{}{}{}",
            inbox_name_separator,
            inbox_name_parts[0],
            inbox_name_separator,
            inbox_name_parts[1],
            inbox_name_separator,
            is_e2e
        );
        InboxNameManager::from_inbox_name(inbox_name)
    }
}

#[cfg(test)]
mod tests {
    use shinkai_message_wasm::{
        shinkai_message::{
            shinkai_message::{Body, ExternalMetadata, InternalMetadata, ShinkaiMessage},
            shinkai_message_schemas::MessageSchemaType,
        },
        shinkai_utils::encryption::EncryptionMethod,
    };

    use super::*;

    // Test new inbox name
    #[test]
    fn valid_inbox_names() {
        let valid_names = vec![
            "inbox::@@node.shinkai::true",
            "inbox::@@node1.shinkai/subidentity::false",
            "inbox::@@alice.shinkai/profileName/agent/myChatGPTAgent::true",
            "inbox::@@alice.shinkai/profileName/device/myPhone::true",
            "inbox::@@node1.shinkai/subidentity::@@node2.shinkai/subidentity2::false",
            "inbox::@@node1.shinkai/subidentity::@@node2.shinkai/subidentity::@@node3.shinkai/subidentity2::false",
        ];

        for name in valid_names {
            let result = InboxNameManager::from_inbox_name(name.to_string());
            assert!(result.is_ok(), "Expected valid inbox name {}", name);
        }
    }

    #[test]
    fn invalid_inbox_names() {
        let invalid_names = vec![
            "@@node1.shinkai::false",
            "inbox::@@node1.shinkai::falsee",
            "@@node1.shinkai",
            "inbox::@@node1.shinkai",
            "inbox::node1::false",
            "inbox::node1.shinkai::false",
            "inbox::@@node1::false",
            "inbox::@@node1.shinkai//subidentity::@@node2.shinkai::false",
            "inbox::@@node1/subidentity::false",
        ];

        for name in &invalid_names {
            let result = InboxNameManager::from_inbox_name(name.to_string());
            assert!(
                result.is_err(),
                "Expected invalid inbox name, but got a valid one for: {}",
                name
            );
        }
    }

    // Test creation of InboxNameManager instance from an inbox name
    #[test]
    fn test_from_inbox_name() {
        let inbox_name = "inbox::@@node1.shinkai/subidentity::@@node2.shinkai/subidentity2::true".to_string();
        let manager = InboxNameManager::from_inbox_name(inbox_name.clone()).unwrap();

        assert_eq!(manager.inbox_name, inbox_name);
    }

    #[test]
    fn test_from_message() {
        let mock_message = ShinkaiMessage {
            body: Some(Body {
                content: "ACK".into(),
                internal_metadata: Some(InternalMetadata {
                    sender_subidentity: "".into(),
                    recipient_subidentity: "".into(),
                    message_schema_type: MessageSchemaType::TextContent,
                    inbox: "inbox::@@node1.shinkai/subidentity::@@node2.shinkai/subidentity2::true".into(),
                    encryption: EncryptionMethod::None,
                }),
            }),
            external_metadata: Some(ExternalMetadata {
                sender: "@@node2.shinkai".into(),
                recipient: "@@node1.shinkai".into(),
                scheduled_time: "20230714T19363326163".into(),
                signature: "3PLx2vZV8kccEEbwPepPQYv2D5zaiSFJXy3JtK57fLuKyh7TBJmcwqMkuCnzLgzAxoatAyKnUSf41smqijpiPBFJ"
                    .into(),
                other: "".into(),
            }),
            encryption: EncryptionMethod::None,
        };

        let manager = InboxNameManager::from_message(&mock_message).unwrap();
        assert_eq!(
            manager.inbox_name,
            "inbox::@@node1.shinkai/subidentity::@@node2.shinkai/subidentity2::true"
        );
    }

    #[test]
    fn test_from_message_invalid() {
        let mock_message = ShinkaiMessage {
            body: Some(Body {
                content: "ACK".into(),
                internal_metadata: Some(InternalMetadata {
                    sender_subidentity: "".into(),
                    recipient_subidentity: "".into(),
                    message_schema_type: MessageSchemaType::TextContent,
                    inbox: "1nb0x::@@node1.shinkai/subidentity::@@node2.shinkai/subidentity2::truee".into(),
                    encryption: EncryptionMethod::None,
                }),
            }),
            external_metadata: Some(ExternalMetadata {
                sender: "@@node2.shinkai".into(),
                recipient: "@@node1.shinkai".into(),
                scheduled_time: "20230714T19363326163".into(),
                signature: "3PLx2vZV8kccEEbwPepPQYv2D5zaiSFJXy3JtK57fLuKyh7TBJmcwqMkuCnzLgzAxoatAyKnUSf41smqijpiPBFJ"
                    .into(),
                other: "".into(),
            }),
            encryption: EncryptionMethod::None,
        };

        let result = InboxNameManager::from_message(&mock_message);
        assert!(result.is_err(), "Expected invalid conversion");
    }

    #[test]
    fn test_get_inbox_name_from_params_valid() {
        let sender = "@@sender.shinkai".to_string();
        let sender_subidentity = "subidentity".to_string();
        let recipient = "@@recipient.shinkai".to_string();
        let recipient_subidentity = "subidentity2".to_string();
        let is_e2e = true;

        let result = InboxNameManager::get_inbox_name_from_params(
            sender,
            sender_subidentity,
            recipient,
            recipient_subidentity,
            is_e2e,
        );

        assert!(result.is_ok(), "Expected valid conversion");
    }

    #[test]
    fn test_get_inbox_name_from_reparable_params() {
        let sender = "sender.shinkai".to_string();
        let sender_subidentity = "subidentity".to_string();
        let recipient = "@@recipient".to_string();
        let recipient_subidentity = "subidentity2".to_string();
        let is_e2e = true;

        let result = InboxNameManager::get_inbox_name_from_params(
            sender,
            sender_subidentity,
            recipient,
            recipient_subidentity,
            is_e2e,
        );

        assert!(result.is_ok(), "Expected valid conversion");
    }

    #[test]
    fn test_get_inbox_name_from_params_invalid() {
        let sender = "invald.sender".to_string(); // Invalid sender
        let sender_subidentity = "subidentity//1".to_string();
        let recipient = "@@@recipient.shinkai".to_string();
        let recipient_subidentity = "subidentity2".to_string();
        let is_e2e = true;

        let result = InboxNameManager::get_inbox_name_from_params(
            sender,
            sender_subidentity,
            recipient,
            recipient_subidentity,
            is_e2e,
        );

        assert!(result.is_err(), "Expected invalid conversion");
    }

    #[test]
    fn test_has_creation_access() {
        let manager = InboxNameManager::from_inbox_name(
            "inbox::@@node1.shinkai/subidentity::@@node2.shinkai::@@node3.shinkai/subidentity3::true".to_string(),
        )
        .unwrap();
        let identity_name = ShinkaiName::new("@@node1.shinkai/subidentity".to_string()).unwrap();
        let identity_name_2 = ShinkaiName::new("@@node2.shinkai/subidentity".to_string()).unwrap();

        assert!(
            manager.has_creation_access(identity_name),
            "Expected identity to have creation access"
        );

        assert!(
            manager.has_creation_access(identity_name_2),
            "Expected identity to have creation access"
        );
    }

    #[test]
    fn test_has_sender_creation_access() {
        let mock_message = ShinkaiMessage {
            body: Some(Body {
                content: "ACK".into(),
                internal_metadata: Some(InternalMetadata {
                    sender_subidentity: "".into(),
                    recipient_subidentity: "".into(),
                    message_schema_type: MessageSchemaType::TextContent,
                    inbox: "inbox::@@node1.shinkai::@@node2.shinkai::true".into(),
                    encryption: EncryptionMethod::None,
                }),
            }),
            external_metadata: Some(ExternalMetadata {
                sender: "@@node3.shinkai".into(),
                recipient: "@@node1.shinkai".into(),
                scheduled_time: "20230714T19363326163".into(),
                signature: "3PLx2vZV8kccEEbwPepPQYv2D5zaiSFJXy3JtK57fLuKyh7TBJmcwqMkuCnzLgzAxoatAyKnUSf41smqijpiPBFJ"
                    .into(),
                other: "".into(),
            }),
            encryption: EncryptionMethod::None,
        };

        let manager = InboxNameManager::from_message(&mock_message).unwrap();

        assert!(
            manager.has_sender_creation_access(mock_message),
            "Expected sender to have creation access"
        );
    }

    #[test]
    fn test_sender_does_not_have_creation_access() {
        let mock_message = ShinkaiMessage {
            body: Some(Body {
                content: "ACK".into(),
                internal_metadata: Some(InternalMetadata {
                    sender_subidentity: "subidentity3".into(),
                    recipient_subidentity: "".into(),
                    message_schema_type: MessageSchemaType::TextContent,
                    inbox: "inbox::@@node1.shinkai::@@node2.shinkai::true".into(),
                    encryption: EncryptionMethod::None,
                }),
            }),
            external_metadata: Some(ExternalMetadata {
                sender: "@@node3.shinkai".into(),
                recipient: "@@node1.shinkai".into(),
                scheduled_time: "20230714T19363326163".into(),
                signature: "3PLx2vZV8kccEEbwPepPQYv2D5zaiSFJXy3JtK57fLuKyh7TBJmcwqMkuCnzLgzAxoatAyKnUSf41smqijpiPBFJ"
                    .into(),
                other: "".into(),
            }),
            encryption: EncryptionMethod::None,
        };

        let manager = InboxNameManager::from_message(&mock_message).unwrap();

        assert!(
            manager.has_sender_creation_access(mock_message),
            "Expected sender to have creation access"
        );
    }
}
