use super::error::AgentError;
use super::providers::LLMProvider;
use reqwest::Client;
use serde_json::{Map, Value as JsonValue};
use shinkai_message_primitives::{
    schemas::{
        agents::serialized_agent::{AgentAPIModel, SerializedAgent},
        shinkai_name::ShinkaiName,
    },
    shinkai_message::shinkai_message_schemas::{JobPreMessage, JobRecipient},
};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};

#[derive(Debug, Clone)]
pub struct Agent {
    pub id: String,
    pub full_identity_name: ShinkaiName,
    pub job_manager_sender: mpsc::Sender<(Vec<JobPreMessage>, String)>,
    pub agent_receiver: Arc<Mutex<mpsc::Receiver<String>>>,
    pub client: Client,
    pub perform_locally: bool,        // flag to perform computation locally or not
    pub external_url: Option<String>, // external API URL
    pub api_key: Option<String>,
    pub model: AgentAPIModel,
    pub toolkit_permissions: Vec<String>, // list of toolkits the agent has access to
    pub storage_bucket_permissions: Vec<String>, // list of storage buckets the agent has access to
    pub allowed_message_senders: Vec<String>, // list of sub-identities allowed to message the agent
}

impl Agent {
    pub fn new(
        id: String,
        full_identity_name: ShinkaiName,
        job_manager_sender: mpsc::Sender<(Vec<JobPreMessage>, String)>,
        perform_locally: bool,
        external_url: Option<String>,
        api_key: Option<String>,
        model: AgentAPIModel,
        toolkit_permissions: Vec<String>,
        storage_bucket_permissions: Vec<String>,
        allowed_message_senders: Vec<String>,
    ) -> Self {
        let client = Client::new();
        let (_, agent_receiver) = mpsc::channel(1); // TODO: I think we can remove this altogether
        let agent_receiver = Arc::new(Mutex::new(agent_receiver)); // wrap the receiver
        Self {
            id,
            full_identity_name,
            job_manager_sender,
            agent_receiver,
            client,
            perform_locally,
            external_url,
            api_key,
            model,
            toolkit_permissions,
            storage_bucket_permissions,
            allowed_message_senders,
        }
    }

    pub async fn call_external_api(&self, content: &str) -> Result<JsonValue, AgentError> {
        match &self.model {
            AgentAPIModel::OpenAI(openai) => {
                openai
                    .call_api(&self.client, self.external_url.as_ref(), self.api_key.as_ref(), content)
                    .await
            }
            AgentAPIModel::Sleep(sleep_api) => {
                sleep_api
                    .call_api(&self.client, self.external_url.as_ref(), self.api_key.as_ref(), content)
                    .await
            }
        }
    }

    /// TODO: Probably just throw this away, and move this logic into a LocalLLM struct that implements the Provider trait
    pub async fn inference_locally(&self, content: String) -> Result<JsonValue, AgentError> {
        // Here we run our GPU-intensive task on a separate thread
        let handle = tokio::task::spawn_blocking(move || {
            let mut map = Map::new();
            map.insert(
                "answer".to_string(),
                JsonValue::String("\n\nHello there, how may I assist you today?".to_string()),
            );
            JsonValue::Object(map)
        });

        match handle.await {
            Ok(response) => Ok(response),
            Err(e) => Err(AgentError::FailedInferencingLocalLLM),
        }
    }

    /// Inferences the LLM model tied to the agent to get a response back.
    /// Note, all `content` is expected to use prompts from the PromptGenerator,
    /// meaning that they tell/force the LLM to always respond in JSON. We automatically
    /// parse the JSON object out of the response into a JsonValue, or error if no object is found.
    pub async fn inference(&self, content: String) -> Result<JsonValue, AgentError> {
        if self.perform_locally {
            // No need to spawn a new task here
            return self.inference_locally(content.clone()).await;
        } else {
            // Call external API
            return self.call_external_api(&content.clone()).await;
        }
    }
}

impl Agent {
    pub fn from_serialized_agent(
        serialized_agent: SerializedAgent,
        sender: mpsc::Sender<(Vec<JobPreMessage>, String)>,
    ) -> Self {
        Self::new(
            serialized_agent.id,
            serialized_agent.full_identity_name,
            sender,
            serialized_agent.perform_locally,
            serialized_agent.external_url,
            serialized_agent.api_key,
            serialized_agent.model,
            serialized_agent.toolkit_permissions,
            serialized_agent.storage_bucket_permissions,
            serialized_agent.allowed_message_senders,
        )
    }
}
