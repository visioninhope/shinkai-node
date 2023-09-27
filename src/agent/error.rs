use crate::db::db_errors::ShinkaiDBError;
use shinkai_message_primitives::schemas::shinkai_name::ShinkaiNameError;
use std::fmt;

#[derive(Debug)]
pub enum AgentError {
    UrlNotSet,
    ApiKeyNotSet,
    ReqwestError(reqwest::Error),
    MissingInitialStepInExecutionPlan,
    FailedExtractingJSONObjectFromResponse(String),
    FailedInferencingLocalLLM,
    UserPromptMissingEBNFDefinition,
    NotAJobMessage,
    JobNotFound,
    JobCreationDeserializationFailed,
    JobMessageDeserializationFailed,
    JobPreMessageDeserializationFailed,
    MessageTypeParseFailed,
    IO(String),
    ShinkaiDB(ShinkaiDBError),
    ShinkaiNameError(ShinkaiNameError),
    AgentNotFound,
    ContentParseFailed,
}

impl fmt::Display for AgentError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            AgentError::UrlNotSet => write!(f, "URL is not set"),
            AgentError::ApiKeyNotSet => write!(f, "API Key not set"),
            AgentError::ReqwestError(err) => write!(f, "Reqwest error: {}", err),
            AgentError::MissingInitialStepInExecutionPlan => write!(
                f,
                "The provided execution plan does not have an InitialExecutionStep as its first element."
            ),
            AgentError::FailedExtractingJSONObjectFromResponse(s) => {
                write!(f, "Could not find JSON Object in the LLM's response: {}", s)
            }
            AgentError::FailedInferencingLocalLLM => {
                write!(f, "Failed inferencing and getting a valid response from the local LLM")
            }
            AgentError::UserPromptMissingEBNFDefinition => {
                write!(f, "At least 1 EBNF subprompt must be defined for the user message.")
            }
            AgentError::NotAJobMessage => write!(f, "Message is not a job message"),
            AgentError::JobNotFound => write!(f, "Job not found"),
            AgentError::JobCreationDeserializationFailed => {
                write!(f, "Failed to deserialize JobCreationInfo message")
            }
            AgentError::JobMessageDeserializationFailed => write!(f, "Failed to deserialize JobMessage"),
            AgentError::JobPreMessageDeserializationFailed => write!(f, "Failed to deserialize JobPreMessage"),
            AgentError::MessageTypeParseFailed => write!(f, "Could not parse message type"),
            AgentError::IO(err) => write!(f, "IO error: {}", err),
            AgentError::ShinkaiDB(err) => write!(f, "Shinkai DB error: {}", err),
            AgentError::AgentNotFound => write!(f, "Agent not found"),
            AgentError::ContentParseFailed => write!(f, "Failed to parse content"),
            AgentError::ShinkaiNameError(err) => write!(f, "ShinkaiName error: {}", err),
        }
    }
}

impl std::error::Error for AgentError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            AgentError::ReqwestError(err) => Some(err),
            AgentError::ShinkaiDB(err) => Some(err),
            AgentError::ShinkaiNameError(err) => Some(err),
            _ => None,
        }
    }
}

impl From<reqwest::Error> for AgentError {
    fn from(err: reqwest::Error) -> AgentError {
        AgentError::ReqwestError(err)
    }
}

impl From<ShinkaiDBError> for AgentError {
    fn from(err: ShinkaiDBError) -> AgentError {
        AgentError::ShinkaiDB(err)
    }
}

impl From<ShinkaiNameError> for AgentError {
    fn from(err: ShinkaiNameError) -> AgentError {
        AgentError::ShinkaiNameError(err)
    }
}

impl From<Box<dyn std::error::Error>> for AgentError {
    fn from(err: Box<dyn std::error::Error>) -> AgentError {
        AgentError::IO(err.to_string())
    }
}
