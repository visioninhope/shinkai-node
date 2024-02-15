use super::job_prompts::{JobPromptGenerator, Prompt};
use crate::agent::error::AgentError;
use crate::agent::file_parsing::ParsingHelper;
use crate::agent::job::Job;
use crate::agent::{agent::Agent, job_manager::JobManager};
use crate::db::db_errors::ShinkaiDBError;
use crate::db::ShinkaiDB;
use async_std::println;
use serde_json::Value as JsonValue;
use shinkai_message_primitives::schemas::agents::serialized_agent::SerializedAgent;
use shinkai_message_primitives::schemas::shinkai_name::ShinkaiName;
use shinkai_message_primitives::shinkai_utils::shinkai_logging::{shinkai_log, ShinkaiLogLevel, ShinkaiLogOption};
use shinkai_vector_resources::source::{SourceFileType, VRSource};
use std::result::Result::Ok;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::instrument;

impl JobManager {
    /// Attempts to extract a single key from the inference response (first matched of potential_keys), including retry inferencing if necessary.
    /// Also tries variants of each potential key using capitalization/casing.
    /// Returns a tuple of the value found at the first matching key + the (potentially new) response JSON (new if retry was done).
    pub async fn advanced_extract_key_from_inference_response(
        agent: SerializedAgent,
        response_json: JsonValue,
        filled_prompt: Prompt,
        potential_keys: Vec<String>,
        retry_attempts: u64,
    ) -> Result<(String, JsonValue), AgentError> {
        if potential_keys.is_empty() {
            return Err(AgentError::InferenceJSONResponseMissingField(
                "No keys supplied to attempt to extract".to_string(),
            ));
        }

        for key in &potential_keys {
            if let Ok(value) = JobManager::direct_extract_key_inference_json_response(response_json.clone(), key) {
                return Ok((value, response_json));
            }
        }

        let mut current_response_json = response_json;
        for _ in 0..retry_attempts {
            for key in &potential_keys {
                let new_response_json = JobManager::json_not_found_retry(
                    agent.clone(),
                    current_response_json.to_string(),
                    filled_prompt.clone(),
                    Some(key.to_string()),
                )
                .await?;
                if let Ok(value) =
                    JobManager::direct_extract_key_inference_json_response(new_response_json.clone(), key)
                {
                    return Ok((value, new_response_json.clone()));
                }
                current_response_json = new_response_json;
            }
        }

        Err(AgentError::InferenceJSONResponseMissingField(potential_keys.join(", ")))
    }

    /// Attempts to extract a String using the provided key in the JSON response.
    /// Also tries variants of the provided key using capitalization/casing.
    pub fn direct_extract_key_inference_json_response(
        response_json: JsonValue,
        key: &str,
    ) -> Result<String, AgentError> {
        let keys_to_try = [
            key.to_string(),
            key[..1].to_uppercase() + &key[1..],
            key.to_uppercase(),
            key.to_lowercase(),
            to_snake_case(key),
            to_camel_case(key),
            to_dash_case(key),
        ];

        for key_variant in keys_to_try.iter() {
            if let Some(value) = response_json.get(key_variant) {
                let value_str = match value {
                    JsonValue::String(s) => s.clone(),
                    _ => value.to_string(),
                };
                return Ok(value_str);
            }
        }

        Err(AgentError::InferenceJSONResponseMissingField(key.to_string()))
    }

    /// Inferences the Agent's LLM with the given prompt. Automatically validates the response is
    /// a valid JSON object, and if it isn't re-inferences to ensure that it is returned as one.
    pub async fn inference_agent(agent: SerializedAgent, filled_prompt: Prompt) -> Result<JsonValue, AgentError> {
        let agent_cloned = agent.clone();
        let prompt_cloned = filled_prompt.clone();
        let task_response = tokio::spawn(async move {
            let agent = Agent::from_serialized_agent(agent_cloned);
            agent.inference(prompt_cloned).await
        })
        .await;

        let response = match task_response {
            Ok(res) => res,
            Err(e) => {
                eprintln!("Task panicked with error: {:?}", e);
                return Err(AgentError::InferenceFailed);
            }
        };

        shinkai_log(
            ShinkaiLogOption::JobExecution,
            ShinkaiLogLevel::Debug,
            format!("inference_agent> response: {:?}", response).as_str(),
        );

        // Validates that the response is a proper JSON object, else inferences again to get the
        // LLM to parse the previous response into proper JSON
        JobManager::_extract_json_value_from_inference_result(response, agent.clone(), filled_prompt).await
    }

    /// Internal method that attempts to extract the JsonValue out of the LLM's response. If it is not proper JSON
    /// then inferences the LLM again asking it to take its previous answer and make sure it responds with a proper JSON object.
    #[instrument]
    async fn _extract_json_value_from_inference_result(
        response: Result<JsonValue, AgentError>,
        agent: SerializedAgent,
        filled_prompt: Prompt,
    ) -> Result<JsonValue, AgentError> {
        match response {
            Ok(json) => Ok(json),
            Err(AgentError::FailedExtractingJSONObjectFromResponse(text)) => {
                shinkai_log(
                    ShinkaiLogOption::JobExecution,
                    ShinkaiLogLevel::Error,
                    "FailedExtractingJSONObjectFromResponse",
                );
                // First try to remove line breaks and re-parse
                let cleaned_text = ParsingHelper::clean_json_response_line_breaks(&text);
                if let Ok(json) = serde_json::from_str::<JsonValue>(&cleaned_text) {
                    return Ok(json);
                }

                //
                match JobManager::json_not_found_retry(agent.clone(), text.clone(), filled_prompt, None).await {
                    Ok(json) => Ok(json),
                    Err(e) => Err(e),
                }
            }
            Err(e) => Err(e),
        }
    }

    /// Inferences the LLM again asking it to take its previous answer and make sure it responds with a proper JSON object
    /// that we can parse. json_key_to_correct allows providing a specific key that the LLM should make sure to correct.
    async fn json_not_found_retry(
        agent: SerializedAgent,
        invalid_json_answer: String,
        original_prompt: Prompt,
        json_key_to_correct: Option<String>,
    ) -> Result<JsonValue, AgentError> {
        let response = tokio::spawn(async move {
            let agent = Agent::from_serialized_agent(agent);
            let prompt = JobPromptGenerator::basic_json_retry_response_prompt(
                invalid_json_answer,
                original_prompt,
                json_key_to_correct,
            );
            agent.inference(prompt).await
        })
        .await;
        let response = match response {
            Ok(res) => res?,
            Err(e) => {
                eprintln!("Task panicked with error: {:?}", e);
                return Err(AgentError::InferenceFailed);
            }
        };

        Ok(response)
    }

    /// Fetches boilerplate/relevant data required for a job to process a step
    pub async fn fetch_relevant_job_data(
        job_id: &str,
        db: Arc<Mutex<ShinkaiDB>>,
    ) -> Result<(Job, Option<SerializedAgent>, String, Option<ShinkaiName>), AgentError> {
        // Fetch the job
        let full_job = { db.lock().await.get_job(job_id)? };

        // Acquire Agent
        let agent_id = full_job.parent_agent_id.clone();
        let mut agent_found = None;
        let mut profile_name = String::new();
        let mut user_profile: Option<ShinkaiName> = None;
        let agents = JobManager::get_all_agents(db).await.unwrap_or(vec![]);
        for agent in agents {
            if agent.id == agent_id {
                agent_found = Some(agent.clone());
                profile_name = agent.full_identity_name.full_name.clone();
                user_profile = Some(agent.full_identity_name.extract_profile().unwrap());
                break;
            }
        }

        Ok((full_job, agent_found, profile_name, user_profile))
    }

    pub async fn get_all_agents(db: Arc<Mutex<ShinkaiDB>>) -> Result<Vec<SerializedAgent>, ShinkaiDBError> {
        let db = db.lock().await;
        db.get_all_agents()
    }
}

// Helper function to convert a string to snake_case
fn to_snake_case(s: &str) -> String {
    s.chars()
        .enumerate()
        .map(|(i, c)| {
            if c.is_uppercase() {
                if i == 0 {
                    c.to_lowercase().to_string()
                } else {
                    format!("_{}", c.to_lowercase())
                }
            } else {
                c.to_string()
            }
        })
        .collect()
}

// Helper function to convert a string to camelCase
fn to_camel_case(s: &str) -> String {
    let mut result = String::new();
    let mut uppercase_next = false;
    for c in s.chars() {
        if c == '_' {
            uppercase_next = true;
        } else if uppercase_next {
            result.push(c.to_ascii_uppercase());
            uppercase_next = false;
        } else {
            result.push(c);
        }
    }
    result
}

// Helper function to convert a string to dash-case (kebab-case)
fn to_dash_case(s: &str) -> String {
    s.chars()
        .enumerate()
        .map(|(i, c)| {
            if c.is_uppercase() {
                if i == 0 {
                    c.to_lowercase().to_string()
                } else {
                    format!("-{}", c.to_lowercase())
                }
            } else {
                c.to_string()
            }
        })
        .collect()
}
