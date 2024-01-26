use super::super::{error::AgentError, execution::job_prompts::Prompt};
use super::shared::openai::{openai_prepare_messages, MessageContent, OpenAIResponse};
use super::LLMProvider;
use crate::managers::model_capabilities_manager::{ModelCapabilitiesManager, PromptResultEnum};
use async_trait::async_trait;
use reqwest::Client;
use serde_json::json;
use serde_json::Value as JsonValue;
use serde_json::{self, Map};
use shinkai_message_primitives::schemas::agents::serialized_agent::{AgentLLMInterface, OpenAI};
use shinkai_message_primitives::shinkai_utils::shinkai_logging::{shinkai_log, ShinkaiLogLevel, ShinkaiLogOption};
use tiktoken_rs::model::get_context_size;

fn truncate_image_url_in_payload(payload: &mut JsonValue) {
    if let Some(messages) = payload.get_mut("messages") {
        if let Some(array) = messages.as_array_mut() {
            for message in array {
                if let Some(content) = message.get_mut("content") {
                    if let Some(array) = content.as_array_mut() {
                        for item in array {
                            if let Some(image_url) = item.get_mut("image_url") {
                                if let Some(url) = image_url.get_mut("url") {
                                    if let Some(str_url) = url.as_str() {
                                        let truncated_url = format!("{}...", &str_url[0..20.min(str_url.len())]);
                                        *url = JsonValue::String(truncated_url);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[async_trait]
impl LLMProvider for OpenAI {
    async fn call_api(
        &self,
        client: &Client,
        url: Option<&String>,
        api_key: Option<&String>,
        prompt: Prompt,
    ) -> Result<JsonValue, AgentError> {
        if let Some(base_url) = url {
            if let Some(key) = api_key {
                let url = format!("{}{}", base_url, "/v1/chat/completions");
                let open_ai = OpenAI {
                    model_type: self.model_type.clone(),
                };
                let model = AgentLLMInterface::OpenAI(open_ai);
                let max_tokens = ModelCapabilitiesManager::get_max_tokens(&model);
                // Note(Nico): we can use prepare_messages directly or we could had called AgentsCapabilitiesManager
                let result = openai_prepare_messages(&model, self.model_type.clone(), prompt, max_tokens)?;
                let messages_json = match result.value {
                    PromptResultEnum::Value(v) => v,
                    _ => {
                        return Err(AgentError::UnexpectedPromptResultVariant(
                            "Expected Value variant in PromptResultEnum".to_string(),
                        ))
                    }
                };

                let mut payload = json!({
                    "model": self.model_type,
                    "messages": messages_json,
                    "temperature": 0.7,
                    "max_tokens": result.remaining_tokens,
                });

                // Openai doesn't support json_object response format for vision models. wut?
                if !self.model_type.contains("vision") {
                    payload["response_format"] = json!({ "type": "json_object" });
                }

                let mut payload_log = payload.clone();
                truncate_image_url_in_payload(&mut payload_log);
                shinkai_log(
                    ShinkaiLogOption::JobExecution,
                    ShinkaiLogLevel::Debug,
                    format!("Call API Body: {:?}", payload_log).as_str(),
                );

                eprintln!("Curl command:");
                eprintln!("curl -X POST \\");
                eprintln!("  -H 'Content-Type: application/json' \\");
                eprintln!("  -H 'Authorization: Bearer {}' \\", key);
                eprintln!("  -d '{}' \\", payload);
                eprintln!("  '{}'", url);

                let res = client
                    .post(url)
                    .bearer_auth(key)
                    .header("Content-Type", "application/json")
                    .json(&payload)
                    .send()
                    .await?;
                shinkai_log(
                    ShinkaiLogOption::JobExecution,
                    ShinkaiLogLevel::Debug,
                    format!("Call API Status: {:?}", res.status()).as_str(),
                );

                let response_text = res.text().await?;
                let data_resp: Result<JsonValue, _> = serde_json::from_str(&response_text);
                shinkai_log(
                    ShinkaiLogOption::JobExecution,
                    ShinkaiLogLevel::Debug,
                    format!("Call API Response Text: {:?}", response_text).as_str(),
                );

                match data_resp {
                    Ok(value) => {
                        if self.model_type.contains("vision") {
                            let data: OpenAIResponse = serde_json::from_value(value).map_err(AgentError::SerdeError)?;
                            let response_string: String = data
                                .choices
                                .iter()
                                .filter_map(|choice| match &choice.message.content {
                                    MessageContent::Text(text) => {
                                        // Unescape the JSON string
                                        let cleaned_json_str = text.replace("\\\"", "\"").replace("\\n", "\n");
                                        Some(cleaned_json_str)
                                    }
                                    MessageContent::ImageUrl { .. } => None,
                                })
                                .collect::<Vec<String>>()
                                .join(" ");
                            Self::extract_first_json_object(&response_string)
                        } else {
                            let data: OpenAIResponse = serde_json::from_value(value).map_err(AgentError::SerdeError)?;
                            let response_string: String = data
                                .choices
                                .iter()
                                .filter_map(|choice| match &choice.message.content {
                                    MessageContent::Text(text) => Some(text.clone()),
                                    MessageContent::ImageUrl { .. } => None,
                                })
                                .collect::<Vec<String>>()
                                .join(" ");
                            Self::extract_first_json_object(&response_string)
                        }
                    }
                    Err(e) => {
                        shinkai_log(
                            ShinkaiLogOption::JobExecution,
                            ShinkaiLogLevel::Error,
                            format!("Failed to parse response: {:?}", e).as_str(),
                        );
                        Err(AgentError::SerdeError(e))
                    }
                }
            } else {
                Err(AgentError::ApiKeyNotSet)
            }
        } else {
            Err(AgentError::UrlNotSet)
        }
    }
}
