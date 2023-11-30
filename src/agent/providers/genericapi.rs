use super::super::{error::AgentError, execution::job_prompts::Prompt};
use super::LLMProvider;
use super::shared::togetherai::TogetherAPIResponse;
use async_trait::async_trait;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json;
use serde_json::json;
use serde_json::Value as JsonValue;
use shinkai_message_primitives::schemas::agents::serialized_agent::GenericAPI;
use shinkai_message_primitives::shinkai_utils::shinkai_logging::{shinkai_log, ShinkaiLogLevel, ShinkaiLogOption};

#[async_trait]
impl LLMProvider for GenericAPI {
    async fn call_api(
        &self,
        client: &Client,
        url: Option<&String>,
        api_key: Option<&String>,
        prompt: Prompt,
    ) -> Result<JsonValue, AgentError> {
        if let Some(base_url) = url {
            if let Some(key) = api_key {
                let url = format!("{}{}", base_url, "/inference");
                let mut messages_string = prompt.generate_genericapi_messages(None)?;
                if !messages_string.ends_with(" ```") {
                    messages_string.push_str(" ```json");
                }

                shinkai_log(
                    ShinkaiLogOption::JobExecution,
                    ShinkaiLogLevel::Info,
                    format!("Messages JSON: {:?}", messages_string).as_str(),
                );

                // panic!();
                // let max_tokens = std::cmp::max(5, 4097 - used_characters);

                // TODO: implement diff tokenizers depending on the model
                let mut max_tokens = Self::get_max_tokens(self.model_type.as_str());
                max_tokens = std::cmp::max(5, max_tokens - (messages_string.len() / 2));

                let payload = json!({
                    "model": self.model_type,
                    "max_tokens": max_tokens,
                    "prompt": messages_string,
                    "request_type": "language-model-inference",
                    "temperature": 0.7,
                    "top_p": 0.7,
                    "top_k": 50,
                    "repetition_penalty": 1,
                    "stream_tokens": false,
                    "stop": [
                        "[/INST]",
                        "</s>"
                    ],
                    "negative_prompt": "",
                    "safety_model": "",
                    "repetitive_penalty": 1,
                });

                shinkai_log(
                    ShinkaiLogOption::JobExecution,
                    ShinkaiLogLevel::Debug,
                    format!("Call API Body: {:?}", payload).as_str(),
                );

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
                shinkai_log(
                    ShinkaiLogOption::JobExecution,
                    ShinkaiLogLevel::Info,
                    format!("Call API Response Text: {:?}", response_text).as_str(),
                );

                let data_resp: Result<TogetherAPIResponse, _> = serde_json::from_str(&response_text);

                match data_resp {
                    Ok(data) => {
                        // Comment(Nico): maybe we could go over all the choices and check for the ones that can convert to json with our format
                        // and from those the longest one. I haven't see multiple choices so far though.
                        let mut response_string: String = data
                            .output
                            .choices
                            .first()
                            .map(|choice| choice.text.clone())
                            .unwrap_or_else(|| String::new());

                        let json_part = Self::json_string_cleanup(&response_string);
                        Self::extract_first_json_object(&json_part)
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

    fn normalize_model(s: &str) -> String {
        s.to_string()
    }

    fn get_max_tokens(s: &str) -> usize {
        if s.to_string().starts_with("Open-Orca/Mistral-7B-OpenOrca") {
            8000
        } else {
            4096
        }
    }

    fn get_max_output_tokens(s: &str) -> usize {
        Self::get_max_tokens(s)
    }
}
