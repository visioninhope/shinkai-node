use crate::agent::error::AgentError;
use crate::agent::execution::job_task_parser::ParsedJobTask;
use crate::agent::job::{Job, JobId, JobLike};
use crate::agent::job_manager::JobManager;
use crate::agent::parsing_helper::ParsingHelper;
use crate::db::ShinkaiDB;
use crate::vector_fs::vector_fs::VectorFS;
use async_recursion::async_recursion;
use keyphrases::KeyPhraseExtractor;
use serde_json::Value as JsonValue;
use shinkai_message_primitives::schemas::agents::serialized_agent::SerializedAgent;
use shinkai_message_primitives::schemas::shinkai_name::ShinkaiName;
use shinkai_message_primitives::shinkai_utils::job_scope::JobScope;
use shinkai_message_primitives::shinkai_utils::shinkai_logging::{shinkai_log, ShinkaiLogLevel, ShinkaiLogOption};
use shinkai_vector_resources::embedding_generator::{EmbeddingGenerator, RemoteEmbeddingGenerator};
use shinkai_vector_resources::embeddings::Embedding;
use shinkai_vector_resources::model_type::EmbeddingModelType;
use shinkai_vector_resources::resource_errors::VRError;
use shinkai_vector_resources::vector_resource::RetrievedNode;
use std::result::Result::Ok;
use std::{collections::HashMap, sync::Arc};
use tracing::instrument;

impl JobManager {
    /// An inference chain for summarizing every VR in the job's scope.
    #[async_recursion]
    #[instrument(skip(generator, vector_fs, db))]
    pub async fn start_summary_inference_chain(
        db: Arc<ShinkaiDB>,
        vector_fs: Arc<VectorFS>,
        full_job: Job,
        job_task: ParsedJobTask,
        agent: SerializedAgent,
        execution_context: HashMap<String, String>,
        generator: RemoteEmbeddingGenerator,
        user_profile: ShinkaiName,
        max_iterations: u64,
        max_tokens_in_prompt: usize,
    ) -> Result<String, AgentError> {
        Ok("Summary inference chain has been chosen".to_string())
    }

    /// Checks if the job's task contains any variation of the word summary,
    /// including common misspellings, or has an extremely high embedding similarity score to the word summary.
    pub async fn validate_job_task_requests_summary(
        job_task: ParsedJobTask,
        generator: RemoteEmbeddingGenerator,
        job_scope: &JobScope,
    ) -> bool {
        // Filter out code blocks
        let only_text_job_task = job_task.get_output_string_filtered(false, true);
        let job_task_embedding = if let Ok(e) = generator.generate_embedding(&only_text_job_task, "").await {
            e
        } else {
            return false;
        };

        let these_score = top_score_summarize_these_embeddings(generator.clone(), &job_task_embedding)
            .await
            .unwrap_or(0.0);
        let this_score = top_score_summarize_this_embeddings(generator.clone(), &job_task_embedding)
            .await
            .unwrap_or(0.0);
        let message_history_score =
            top_score_message_history_summary_embeddings(generator.clone(), &job_task_embedding)
                .await
                .unwrap_or(0.0);

        println!("Top These score: {:.2}", these_score);
        println!("Top This score: {:.2}", this_score);
        println!("Top Message history score: {:.2}", message_history_score);

        let mut passing = 1.0;
        if generator.model_type()
            == EmbeddingModelType::TextEmbeddingsInference(
                shinkai_vector_resources::model_type::TextEmbeddingsInference::AllMiniLML6v2,
            )
        {
            passing = 0.68;
        } else {
            eprintln!(
                "Embedding model type not accounted for in Summary Chain detection! Add: {:?}",
                generator.model_type()
            );
            passing = 0.75;
        }

        // Check if any of them are passing
        if these_score > passing || this_score > passing || message_history_score > passing {
            return true;
        } else {
            return false;
        }
    }
}

/// Scores job task embedding against a set of embeddings and returns the highest score.
async fn top_score_embeddings(embeddings: Vec<(String, Embedding)>, job_task_embedding: &Embedding) -> f32 {
    let mut top_score = 0.0;
    for (summary_string, summary_embedding) in embeddings {
        let score = summary_embedding.score_similarity(job_task_embedding);
        println!("{} Score: {:.2}", summary_string, score);
        if score > top_score {
            top_score = score;
        }
    }
    top_score
}

/// Scores job task embedding against "summarize these" embeddings and returns the highest score.
async fn top_score_summarize_these_embeddings(
    generator: RemoteEmbeddingGenerator,
    job_task_embedding: &Embedding,
) -> Result<f32, VRError> {
    let embeddings = summarize_these_embeddings(generator).await?;
    Ok(top_score_embeddings(embeddings, job_task_embedding).await)
}

/// Scores job task embedding against "summarize this" embeddings and returns the highest score.
async fn top_score_summarize_this_embeddings(
    generator: RemoteEmbeddingGenerator,
    job_task_embedding: &Embedding,
) -> Result<f32, VRError> {
    let embeddings = summarize_this_embeddings(generator).await?;
    Ok(top_score_embeddings(embeddings, job_task_embedding).await)
}

/// Scores job task embedding against message history summary embeddings and returns the highest score.
async fn top_score_message_history_summary_embeddings(
    generator: RemoteEmbeddingGenerator,
    job_task_embedding: &Embedding,
) -> Result<f32, VRError> {
    let embeddings = message_history_summary_embeddings(generator).await?;
    Ok(top_score_embeddings(embeddings, job_task_embedding).await)
}

/// Returns summary embeddings related to requests for summarizing multiple documents or files
async fn summarize_these_embeddings(generator: RemoteEmbeddingGenerator) -> Result<Vec<(String, Embedding)>, VRError> {
    let strings = vec![
        "Summarize these files".to_string(),
        "I want a summary of these".to_string(),
        "These files, I need a summary".to_string(),
        "Summarize all of these together".to_string(),
        "Provide a summary for these documents".to_string(),
        "Can you summarize these?".to_string(),
        "Need a quick summary of these files".to_string(),
        "Sum up these documents for me".to_string(),
        "Give an overview of these files".to_string(),
        "Condense these documents into a summary".to_string(),
        "Wrap up these files in a summary".to_string(),
        "Break down these documents for me".to_string(),
        "Summarize the contents of these files".to_string(),
        "Quick summary of these, please".to_string(),
        "Overview these documents".to_string(),
        "Condense these into a summary".to_string(),
        "Summarize these readings".to_string(),
        "Give a concise summary of these documents".to_string(),
        "Sumarize these".to_string(),
        "Summarise these".to_string(),
        "Sumarise these".to_string(),
        "Summrize these".to_string(),
        "Sumrize these".to_string(),
        "Sumariz these".to_string(),
        "Sumarze these".to_string(),
        "Summrize these".to_string(),
        "Smmarize these".to_string(),
        "Sumrize these".to_string(),
        "Sumrise these".to_string(),
        "Smarize these".to_string(),
        "Sunnarize these".to_string(),
        "Summarize these".to_string(),
        "Summarize these documents/files".to_string(),
        "Sumarize the documents/files".to_string(),
        "Summarise the documents/files".to_string(),
        "Sumarise the documents/files".to_string(),
        "Summrize the documents/files".to_string(),
        "Sumrize the documents/files".to_string(),
        "Sumariz the documents/files".to_string(),
        "Sumarze the documents/files".to_string(),
        "Summrize the documents/files".to_string(),
        "Smmarize the documents/files".to_string(),
        "Sumrize the documents/files".to_string(),
        "Sumrise the documents/files".to_string(),
        "Smarize the documents/files".to_string(),
        "Sunnarize the documents/files".to_string(),
        "Summarize the documents/files".to_string(),
        "Give me a summary of these docs/files".to_string(),
        "Give me a sumary of these docs/files".to_string(),
        "Give me a sumry of these docs/files".to_string(),
        "Give me a summry of these docs/files".to_string(),
        "Give me a summay of these docs/files".to_string(),
        "Give me a summy of these docs/files".to_string(),
        "Give me a smmary of these docs/files".to_string(),
    ];
    let ids = vec!["".to_string(); strings.len()];
    let embeddings = generator.generate_embeddings(&strings, &ids).await?;
    Ok(strings.into_iter().zip(embeddings.into_iter()).collect())
}

/// Returns summary embeddings related to specific requests for summarization
async fn summarize_this_embeddings(generator: RemoteEmbeddingGenerator) -> Result<Vec<(String, Embedding)>, VRError> {
    let strings = vec![
        "Summarize this for me".to_string(),
        "Summarize this fro me".to_string(),
        "Recap the below for me:".to_string(),
        "Summarize this".to_string(),
        "Sumarize this".to_string(),
        "Summarise this".to_string(),
        "Sumarise this".to_string(),
        "Sumarise this".to_string(),
        "Summrize this".to_string(),
        "Sumrize this".to_string(),
        "Sumariz this".to_string(),
        "Sumarze this".to_string(),
        "Summrize this".to_string(),
        "Smmarize this".to_string(),
        "Sumrize this".to_string(),
        "Sumrise this".to_string(),
        "Smarize this".to_string(),
        "Sunnarize this".to_string(),
        "Summarize this".to_string(),
        "Give me a summary:".to_string(),
        "Give me a sumary:".to_string(),
        "Give me a sumry:".to_string(),
        "Give me a summry:".to_string(),
        "Give me a summay:".to_string(),
        "Give me a summy:".to_string(),
        "Give me a smmary:".to_string(),
        "Provide a summary of this".to_string(),
        "Can you summarize this?".to_string(),
        "I need a summary".to_string(),
        "Summarize the following".to_string(),
        "Quick summary, please".to_string(),
        "Summarization needed".to_string(),
        "Sum it up for me".to_string(),
        "Overview this content".to_string(),
        "Condense this into a summary".to_string(),
        "Wrap this up in a summary".to_string(),
        "Break this down for me".to_string(),
        "Condense this into a summary".to_string(),
        "Summarize the content".to_string(),
        "Gimmie a summary".to_string(),
        "Gimme summary now".to_string(),
        "I said I want a detailed summary".to_string(),
    ];
    let ids = vec!["".to_string(); strings.len()];
    let embeddings = generator.generate_embeddings(&strings, &ids).await;
    if let Err(e) = embeddings {
        println!("Failed generating this embeddings: {:?}", e);
        return Err(e);
    }

    Ok(strings.into_iter().zip(embeddings.unwrap().into_iter()).collect())
}

/// Returns summary embeddings related to chat message history
async fn message_history_summary_embeddings(
    generator: RemoteEmbeddingGenerator,
) -> Result<Vec<(String, Embedding)>, VRError> {
    let strings = vec![
        "Summarize our conversation.".to_string(),
        "Summarize this chat.".to_string(),
        "Summarize this conversation.".to_string(),
        "Summarize this chat in 300 words or less.".to_string(),
        "Summarize the message history".to_string(),
        "Recap the message history".to_string(),
        "Recap the conversation".to_string(),
        "Recap our chat".to_string(),
        "Give a rundown of our discussion.".to_string(),
        "Outline the key points from our chat.".to_string(),
        "Condense our conversation into a summary.".to_string(),
        "Briefly recap our chat history.".to_string(),
        "Sum up this conversation for me.".to_string(),
        "Provide a concise summary of our discussion.".to_string(),
        "Highlight the main points from this chat.".to_string(),
        "Give a brief overview of our conversation.".to_string(),
        "Wrap up this chat with a summary.".to_string(),
        "Boil down our conversation to the essentials.".to_string(),
        "Summarize the key takeaways from this chat.".to_string(),
        "Condense the chat into a few key points.".to_string(),
        "Give a quick summary of our conversation.".to_string(),
        "Recap the highlights of our discussion.".to_string(),
        "Summarize the main points of this chat.".to_string(),
        "Provide a summary of our chat highlights.".to_string(),
        "Outline the essentials of our conversation.".to_string(),
        "Give a snapshot of our chat.".to_string(),
        "Summarize the gist of our conversation.".to_string(),
        "Recap the core points of our chat.".to_string(),
        "Sumarize this chat/conversation".to_string(),
        "Summarise this chat/conversation".to_string(),
        "Sumarise this chat/conversation".to_string(),
        "Summrize this chat/conversation".to_string(),
        "Sumrize this chat/conversation".to_string(),
        "Sumariz this chat/conversation".to_string(),
        "Sumarze this chat/conversation".to_string(),
        "Summrize this chat/conversation".to_string(),
        "Smmarize this chat/conversation".to_string(),
        "Sumrize this chat/conversation".to_string(),
        "Sumrise this chat/conversation".to_string(),
        "Smarize this chat/conversation".to_string(),
        "Sunnarize this chat/conversation".to_string(),
        "Summarize this chat/conversation".to_string(),
        "Give me a summary of this chat/conversation".to_string(),
        "Give me a sumary of this chat/conversation".to_string(),
        "Give me a sumry of this chat/conversation".to_string(),
        "Give me a summry of this chat/conversation".to_string(),
        "Give me a summay of this chat/conversation".to_string(),
        "Give me a summy of this chat/conversation".to_string(),
        "Give me a smmary of this chat/conversation".to_string(),
    ];
    let ids = vec!["".to_string(); strings.len()];
    let embeddings = generator.generate_embeddings(&strings, &ids).await?;
    Ok(strings.into_iter().zip(embeddings.into_iter()).collect())
}
