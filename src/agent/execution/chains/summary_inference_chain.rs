use crate::agent::error::AgentError;
use crate::agent::execution::job_task_parser::ParsedJobTask;
use crate::agent::job::{Job, JobId, JobLike, JobStepResult};
use crate::agent::job_manager::JobManager;
use crate::db::ShinkaiDB;
use crate::vector_fs::vector_fs::VectorFS;
use async_recursion::async_recursion;
use futures::stream::StreamExt;
use keyphrases::KeyPhraseExtractor;
use serde_json::Value as JsonValue;
use shinkai_message_primitives::schemas::agents::serialized_agent::SerializedAgent;
use shinkai_message_primitives::schemas::shinkai_name::ShinkaiName;
use shinkai_message_primitives::shinkai_utils::job_scope::JobScope;
use shinkai_message_primitives::shinkai_utils::shinkai_logging::{shinkai_log, ShinkaiLogLevel, ShinkaiLogOption};
use shinkai_vector_resources::embedding_generator::{EmbeddingGenerator, RemoteEmbeddingGenerator};
use shinkai_vector_resources::model_type::EmbeddingModelType;
use std::result::Result::Ok;
use std::{collections::HashMap, sync::Arc};
use tracing::instrument;

use super::chain_detection_embeddings::{
    top_score_message_history_summary_embeddings, top_score_summarize_these_embeddings,
    top_score_summarize_this_embeddings,
};

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
        // Perform the checks
        let this_check = this_check(&generator, &job_task, full_job.scope(), &full_job.step_history).await?;
        let these_check = these_check(&generator, &job_task, full_job.scope()).await?;
        let message_history_check = message_history_check(&generator, &job_task).await?;

        let checks = vec![this_check, these_check, message_history_check];
        let highest_score_check = checks
            .into_iter()
            .filter(|check| check.0)
            .fold((false, 0.0f32), |acc, check| if check.1 > acc.1 { check } else { acc });

        // Later implement this alternative summary flow
        // if message_history_check.1 == highest_score_check.1 {
        if these_check.1 == highest_score_check.1 || this_check.1 == highest_score_check.1 {
            Self::start_summarize_job_context_sub_chain(
                db,
                vector_fs,
                full_job,
                job_task,
                agent,
                execution_context,
                generator,
                user_profile,
                max_iterations,
                max_tokens_in_prompt,
            )
            .await
        } else {
            Self::start_summarize_job_context_sub_chain(
                db,
                vector_fs,
                full_job,
                job_task,
                agent,
                execution_context,
                generator,
                user_profile,
                max_iterations,
                max_tokens_in_prompt,
            )
            .await
        }
    }

    /// Core logic which summarizes VRs in the job context.
    async fn start_summarize_job_context_sub_chain(
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
        let scope = full_job.scope();

        let resource_count =
            JobManager::count_number_of_resources_in_job_scope(vector_fs.clone(), &user_profile, scope).await?;

        // Get a stream that retrieves all resources in the job scope automatically, and chunk it in groups of 5 (same as stream buffer size)
        let resource_stream =
            JobManager::retrieve_all_resources_in_job_scope_stream(vector_fs.clone(), &scope, &user_profile).await;
        let mut chunks = resource_stream.chunks(5);

        // For each chunk parallelize creating a detailed summary for each
        let mut num_resources_processed = 0;
        while let Some(resources) = chunks.next().await {
            println!("Received chunk of resources: {}", resources.len());

            num_resources_processed += resources.len();
        }

        // Optimization TODO:
        // If a significant amount of VRs, simply search first to find the the top 5 most relevant and summarize them fully.
        // The rest summarize as 1-2 line sentences and list them up to 25.

        Ok("Summary inference chain has been chosen".to_string())
    }

    // TODO: Optimization. Directly check if the text holds any substring of summary/summarize/recap botched or not. If yes, only then do the embedding checks.
    /// Checks if the job's task asks to summarize in one of many ways using vector search.
    pub async fn validate_job_task_requests_summary(
        job_task: ParsedJobTask,
        generator: RemoteEmbeddingGenerator,
        job_scope: &JobScope,
        step_history: &Vec<JobStepResult>,
    ) -> bool {
        // Perform the checks
        let these_check = these_check(&generator, &job_task, job_scope)
            .await
            .unwrap_or((false, 0.0));
        let this_check = this_check(&generator, &job_task, job_scope, step_history)
            .await
            .unwrap_or((false, 0.0));
        let message_history_check = message_history_check(&generator, &job_task)
            .await
            .unwrap_or((false, 0.0));

        // Check if any of the conditions passed
        these_check.0 || this_check.0 || message_history_check.0
    }
}

/// Returns the passing score for the summary chain checks
fn passing_score(generator: &RemoteEmbeddingGenerator) -> f32 {
    if generator.model_type()
        == EmbeddingModelType::TextEmbeddingsInference(
            shinkai_vector_resources::model_type::TextEmbeddingsInference::AllMiniLML6v2,
        )
    {
        0.68
    } else {
        eprintln!(
            "Embedding model type not accounted for in Summary Chain detection! Add: {:?}",
            generator.model_type()
        );
        0.75
    }
}

/// Checks if the job task's similarity score passes for any of the "these" summary strings
async fn these_check(
    generator: &RemoteEmbeddingGenerator,
    job_task: &ParsedJobTask,
    job_scope: &JobScope,
) -> Result<(bool, f32), AgentError> {
    // Get job task embedding, without code blocks for clarity in task
    let job_task_embedding = job_task
        .generate_embedding_filtered(generator.clone(), false, true)
        .await?;
    let passing = passing_score(&generator.clone());
    let these_score = top_score_summarize_these_embeddings(generator.clone(), &job_task_embedding).await?;
    println!("Top These score: {:.2}", these_score);
    Ok((these_score > passing && !job_scope.is_empty(), these_score))
}

/// Checks if the job task's similarity score passes for any of the "this" summary strings
async fn this_check(
    generator: &RemoteEmbeddingGenerator,
    job_task: &ParsedJobTask,
    job_scope: &JobScope,
    step_history: &Vec<JobStepResult>,
) -> Result<(bool, f32), AgentError> {
    // Get job task embedding, without code blocks for clarity in task
    let job_task_embedding = job_task
        .generate_embedding_filtered(generator.clone(), false, true)
        .await?;

    let passing = passing_score(&generator.clone());
    let this_score = top_score_summarize_this_embeddings(generator.clone(), &job_task_embedding).await?;
    println!("Top This score: {:.2}", this_score);

    // Get current job task code block count, and the previous job task's code block count if it exists in step history
    let current_code_block_count = job_task.get_elements_filtered(true, false).len();

    for step in step_history {
        println!("Step: {:?}", step);
    }

    // let previous_job_task = step_history.last().map(|step| step.).unwrap_or_default();
    // let previous_code_block_count = previous_job_task.get_elements_filtered(true, false).len();

    // Old check. Potentially reuse this if we decide to go with a custom code block summarizer.
    // let check = (this_score > passing && !job_scope.is_empty()) && code_block_count < 1);

    // Only pass if there are VRs in scope, and no code blocks in job task. This is to allow QA chain to deal with codeblock summary for now.
    let check = (this_score > passing && !job_scope.is_empty()) && current_code_block_count < 1;

    Ok((check, this_score))
}

/// Checks if the job task's similarity score passes for the "message history" summary string
async fn message_history_check(
    generator: &RemoteEmbeddingGenerator,
    job_task: &ParsedJobTask,
) -> Result<(bool, f32), AgentError> {
    // Get job task embedding, without code blocks for clarity in task
    let job_task_embedding = job_task
        .generate_embedding_filtered(generator.clone(), false, true)
        .await?;

    let passing = passing_score(&generator.clone());
    let message_history_score =
        top_score_message_history_summary_embeddings(generator.clone(), &job_task_embedding).await?;
    println!("Top Message history score: {:.2}", message_history_score);
    Ok((message_history_score > passing, message_history_score))
}
