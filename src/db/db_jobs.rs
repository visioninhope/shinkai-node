use std::collections::HashMap;
use std::time::Instant;

use super::{db::Topic, db_errors::ShinkaiDBError, ShinkaiDB};
use crate::agent::execution::job_prompts::{Prompt, SubPromptType};
use crate::agent::job::{Job, JobLike, JobStepResult};
use rocksdb::{IteratorMode, Options, WriteBatch};
use shinkai_message_primitives::schemas::{inbox_name::InboxName, shinkai_time::ShinkaiStringTime};
use shinkai_message_primitives::shinkai_message::shinkai_message::ShinkaiMessage;
use shinkai_message_primitives::shinkai_utils::job_scope::JobScope;
use shinkai_message_primitives::shinkai_utils::shinkai_logging::{shinkai_log, ShinkaiLogLevel, ShinkaiLogOption};

impl ShinkaiDB {
    pub fn create_new_job(
        &mut self,
        job_id: String,
        agent_id: String,
        scope: JobScope,
        is_hidden: bool,
    ) -> Result<(), ShinkaiDBError> {
        let start = std::time::Instant::now();

        // Use shared CFs
        let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();

        // Start a write batch
        let batch_write_start = Instant::now();
        let mut batch = WriteBatch::default();

        // Generate time currently, used as a key. It should be safe because it's generated here so it shouldn't be duplicated.
        let current_time = ShinkaiStringTime::generate_time_now();
        let scope_bytes = scope.to_bytes()?;

        // Construct keys with job_id as part of the key
        let job_scope_key = format!("jobinbox_{}_scope", job_id);
        let jobs_by_agent_id_key = format!("jobinbox_{}_{}_{}", agent_id, current_time, job_id); // Note: It won't work with prefix search. Unnecesary?
        let job_is_finished_key = format!("jobinbox_{}_is_finished", job_id);
        let job_datetime_created_key = format!("jobinbox_{}_datetime_created", job_id);
        let job_parent_agentid = format!("jobinbox_{}_agentid", job_id);
        let job_parent_agentid_key = format!("jobinbox_agent_{}_{}", Self::agent_id_to_hash(&agent_id), job_id); // needs to be 47 characters for prefix search to work
        let job_inbox_name = format!("jobinbox_{}_inboxname", job_id);
        let job_conversation_inbox_name_key = format!("jobinbox_{}_conversation_inbox_name", job_id);
        let all_jobs_time_keyed = format!("all_jobs_time_keyed_placeholder_to_fit_prefix__{}", current_time);
        let job_smart_inbox_name_key = format!("{}_smart_inbox_name", job_id);
        let job_is_hidden_key = format!("jobinbox_{}_is_hidden", job_id);
        let job_read_list_key = format!("jobinbox_{}_read_list", job_id);

        // Content
        let conversation_inbox_prefix = format!("inbox_{}", Self::job_id_to_hash(&job_id)); // 47 characters so prefix works
        let job_inbox_name_content = format!("job_inbox::{}::false", job_id);
        let initial_job_name = format!("New Job: {}", job_id);

        // Put Job Data into the DB
        batch.put_cf(cf_inbox, job_scope_key.as_bytes(), &scope_bytes);
        batch.put_cf(cf_inbox, jobs_by_agent_id_key.as_bytes(), "".as_bytes());
        batch.put_cf(cf_inbox, job_is_finished_key.as_bytes(), b"false");
        batch.put_cf(cf_inbox, job_datetime_created_key.as_bytes(), current_time.as_bytes());
        batch.put_cf(cf_inbox, job_parent_agentid.as_bytes(), agent_id.as_bytes());
        batch.put_cf(cf_inbox, job_parent_agentid_key.as_bytes(), job_id.as_bytes());
        batch.put_cf(
            cf_inbox,
            job_conversation_inbox_name_key.as_bytes(),
            conversation_inbox_prefix.as_bytes(),
        );
        batch.put_cf(
            cf_inbox,
            job_inbox_name.as_bytes(),
            job_inbox_name_content.as_bytes(),
        );
        batch.put_cf(cf_inbox, all_jobs_time_keyed.as_bytes(), job_id.as_bytes());
        batch.put_cf(
            cf_inbox,
            job_smart_inbox_name_key.as_bytes(),
            initial_job_name.as_bytes(),
        );
        batch.put_cf(cf_inbox, job_is_hidden_key.as_bytes(), &is_hidden.to_string());
        batch.put_cf(cf_inbox, job_read_list_key.as_bytes(), "".to_string());

        self.db.write(batch)?;

        let batch_write_duration = batch_write_start.elapsed();
        println!("create_new_job Batch write took: {:?}", batch_write_duration);

        let duration = start.elapsed();
        if std::env::var("DEBUG_TIMING").unwrap_or_default() == "true" {
            shinkai_log(
                ShinkaiLogOption::Database,
                ShinkaiLogLevel::Info,
                format!("create_new_job execution time: {:?}", duration).as_str(),
            );
        }

        Ok(())
    }

    /// Returns the the first half of the blake3 hash of the agent id value
    pub fn agent_id_to_hash(agent_id: &str) -> String {
        let full_hash = blake3::hash(agent_id.as_bytes()).to_hex().to_string();
        full_hash[..full_hash.len() / 2].to_string()
    }

    /// Returns the first half of the blake3 hash of the job id value
    pub fn job_id_to_hash(job_id: &str) -> String {
        let input = &format!("job_inbox::{}::false", job_id);
        let full_hash = blake3::hash(input.as_bytes()).to_hex().to_string();
        full_hash[..full_hash.len() / 2].to_string()
    }

    /// Fetches a job from the DB
    pub fn get_job(&self, job_id: &str) -> Result<Job, ShinkaiDBError> {
        let start = std::time::Instant::now();
        let (
            scope,
            is_finished,
            is_hidden,
            datetime_created,
            parent_agent_id,
            conversation_inbox,
            step_history,
            unprocessed_messages,
            execution_context,
        ) = self.get_job_data(job_id, true)?;

        // Construct the job
        let job = Job {
            job_id: job_id.to_string(),
            is_hidden,
            datetime_created,
            is_finished,
            parent_agent_id,
            scope,
            conversation_inbox_name: conversation_inbox,
            step_history: step_history.unwrap_or_else(Vec::new),
            unprocessed_messages,
            execution_context,
        };

        let duration = start.elapsed();
        if std::env::var("DEBUG_TIMING").unwrap_or_default() == "true" {
            shinkai_log(
                ShinkaiLogOption::Database,
                ShinkaiLogLevel::Info,
                format!("get_job execution time: {:?}", duration).as_str(),
            );
        }

        Ok(job)
    }

    /// Fetches a job from the DB as a Box<dyn JobLik>
    pub fn get_job_like(&self, job_id: &str) -> Result<Box<dyn JobLike>, ShinkaiDBError> {
        let start = std::time::Instant::now();
        let (
            scope,
            is_finished,
            is_hidden,
            datetime_created,
            parent_agent_id,
            conversation_inbox,
            _,
            unprocessed_messages,
            execution_context,
        ) = self.get_job_data(job_id, false)?;

        // Construct the job
        let job = Job {
            job_id: job_id.to_string(),
            is_hidden,
            datetime_created,
            is_finished,
            parent_agent_id,
            scope,
            conversation_inbox_name: conversation_inbox,
            step_history: Vec::new(), // Empty step history for JobLike
            unprocessed_messages,
            execution_context,
        };

        let duration = start.elapsed();
        if std::env::var("DEBUG_TIMING").unwrap_or_default() == "true" {
            shinkai_log(
                ShinkaiLogOption::Database,
                ShinkaiLogLevel::Info,
                format!("get_job_like execution time: {:?}", duration).as_str(),
            );
        }

        Ok(Box::new(job))
    }

    /// Fetches data for a specific Job from the DB
    fn get_job_data(
        &self,
        job_id: &str,
        fetch_step_history: bool,
    ) -> Result<
        (
            JobScope,
            bool,
            bool,
            String,
            String,
            InboxName,
            Option<Vec<JobStepResult>>,
            Vec<String>,
            HashMap<String, String>,
        ),
        ShinkaiDBError,
    > {
        // Use shared CFs
        let cf_jobs = self.get_cf_handle(Topic::Inbox).unwrap();

        // Begin fetching the data from the DB
        let scope_value = self
            .db
            .get_cf(cf_jobs, format!("jobinbox_{}_scope", job_id).as_bytes())?
            .ok_or(ShinkaiDBError::DataNotFound)?;
        let scope = JobScope::from_bytes(&scope_value)?;

        let is_finished_value = self
            .db
            .get_cf(cf_jobs, format!("jobinbox_{}_is_finished", job_id).as_bytes())?
            .ok_or(ShinkaiDBError::DataNotFound)?;
        let is_finished = std::str::from_utf8(&is_finished_value)?.to_string() == "true";

        let datetime_created_value = self
            .db
            .get_cf(cf_jobs, format!("jobinbox_{}_datetime_created", job_id).as_bytes())?
            .ok_or(ShinkaiDBError::DataNotFound)?;
        let datetime_created = std::str::from_utf8(&datetime_created_value)?.to_string();

        let parent_agent_id_value = self
            .db
            .get_cf(cf_jobs, format!("jobinbox_{}_agentid", job_id).as_bytes())?
            .ok_or(ShinkaiDBError::DataNotFound)?;
        let parent_agent_id = std::str::from_utf8(&parent_agent_id_value)?.to_string();

        let job_inbox_name = self
            .db
            .get_cf(
                cf_jobs,
                format!("jobinbox_{}_inboxname", job_id).as_bytes(),
            )?
            .ok_or(ShinkaiDBError::DataNotFound)?;
        let inbox_name = std::str::from_utf8(&job_inbox_name)?.to_string();
        let conversation_inbox = InboxName::new(inbox_name)?;

        let is_hidden_value = self
            .db
            .get_cf(cf_jobs, format!("jobinbox_{}_is_hidden", job_id).as_bytes())?
            .unwrap_or_else(|| b"false".to_vec());
        let is_hidden = std::str::from_utf8(&is_hidden_value)?.to_string() == "true";

        // Reads all of the step history by iterating
        let step_history = self.get_step_history(job_id, fetch_step_history)?;

        // Reads all of the unprocessed messages by iterating
        let unprocessed_messages = self.get_unprocessed_messages(job_id)?;

        Ok((
            scope,
            is_finished,
            is_hidden,
            datetime_created,
            parent_agent_id,
            conversation_inbox,
            step_history,
            unprocessed_messages,
            self.get_job_execution_context(job_id)?,
        ))
    }

    /// Fetches all jobs
    pub fn get_all_jobs(&self) -> Result<Vec<Box<dyn JobLike>>, ShinkaiDBError> {
        // Use shared CFs
        let cf_jobs = self.get_cf_handle(Topic::Inbox).unwrap();

        let mut jobs = Vec::new();
        // Create a prefix iterator for keys starting with "all_jobs_time_keyed_"
        let prefix = b"all_jobs_time_keyed_placeholder_to_fit_prefix__";
        let iter = self.db.prefix_iterator_cf(cf_jobs, prefix);
        for item in iter {
            let (_key, value) = item.map_err(|e| ShinkaiDBError::RocksDBError(e))?;
            // The value is the job ID
            let job_id = std::str::from_utf8(&value)?.to_string();
            eprintln!("Job ID: {}", job_id);
            // Fetch the job using the job ID
            let job = self.get_job_like(&job_id)?;
            jobs.push(job);
        }
        Ok(jobs)
    }

    /// Updates the JobScope of a job given it's id
    pub fn update_job_scope(&mut self, job_id: String, scope: JobScope) -> Result<(), ShinkaiDBError> {
        let cf_jobs = self.get_cf_handle(Topic::Inbox).unwrap();
        let scope_bytes = scope.to_bytes()?;
        let job_scope_key = format!("jobinbox_{}_scope", &job_id);
        self.db.put_cf(cf_jobs, job_scope_key.as_bytes(), &scope_bytes)?;

        Ok(())
    }

    /// Fetches all jobs under a specific Agent
    pub fn get_agent_jobs(&self, agent_id: String) -> Result<Vec<Box<dyn JobLike>>, ShinkaiDBError> {
        let cf_jobs = self.get_cf_handle(Topic::Inbox).unwrap();
        let prefix_string = format!("jobinbox_agent_{}", Self::agent_id_to_hash(&agent_id));
        let prefix = prefix_string.as_bytes();
        let mut jobs = Vec::new();
        let iter = self.db.prefix_iterator_cf(cf_jobs, prefix);
        for item in iter {
            let (_key, value) = item.map_err(|e| ShinkaiDBError::RocksDBError(e))?;
            let job_id = std::str::from_utf8(&value)?.to_string();
            eprintln!("Job ID: {}", job_id);
            let job = self.get_job_like(&job_id)?;
            jobs.push(job);
        }
        Ok(jobs)
    }

    /// Sets/updates the execution context for a Job in the DB
    pub fn set_job_execution_context(
        &mut self,
        job_id: String,
        context: HashMap<String, String>,
        message_key: Option<String>,
    ) -> Result<(), ShinkaiDBError> {
        let message_key = match message_key {
            Some(key) => key,
            None => {
                // Fetch the most recent message from the job's inbox
                let inbox_name = InboxName::get_job_inbox_name_from_params(job_id.clone())?;
                let last_messages = self.get_last_messages_from_inbox(inbox_name.to_string(), 1, None)?;
                if let Some(message) = last_messages.first() {
                    if let Some(message) = message.first() {
                        message.calculate_message_hash_for_pagination()
                    } else {
                        return Err(ShinkaiDBError::SomeError("No messages found in the inbox".to_string()));
                    }
                } else {
                    return Err(ShinkaiDBError::SomeError("No messages found in the inbox".to_string()));
                }
            }
        };

        let cf_jobs = self.get_cf_handle(Topic::Inbox).unwrap();
        let current_time = ShinkaiStringTime::generate_time_now();
        let execution_context_key = format!(
            "jobinbox_{}_{}_execution_context_{}",
            &job_id, &message_key, current_time
        );

        // Convert the context to bytes
        let context_bytes = bincode::serialize(&context).map_err(|_| {
            ShinkaiDBError::SomeError("Failed converting execution context hashmap to bytes".to_string())
        })?;

        self.db
            .put_cf(cf_jobs, execution_context_key.as_bytes(), &context_bytes)?;

        Ok(())
    }

    /// Gets the execution context for a job
    pub fn get_job_execution_context(&self, job_id: &str) -> Result<HashMap<String, String>, ShinkaiDBError> {
        let start = std::time::Instant::now();
        let inbox_name = InboxName::get_job_inbox_name_from_params(job_id.to_string())?;
        let mut execution_context: HashMap<String, String> = HashMap::new();

        // Fetch the last message from the job's inbox
        let last_messages = self.get_last_messages_from_inbox(inbox_name.to_string(), 1, None)?;
        if let Some(message_path) = last_messages.first() {
            if let Some(message) = message_path.first() {
                let message_key = message.calculate_message_hash_for_pagination();
                // TODO: fix this
                let cf_name = format!("{}_{}_execution_context", job_id, message_key);
                if let Some(cf_handle) = self.db.cf_handle(&cf_name) {
                    // Get the last context (should be only one)
                    let mut iter = self.db.iterator_cf(cf_handle, IteratorMode::End);
                    if let Some(Ok((_, value))) = iter.next() {
                        let context: Result<HashMap<String, String>, ShinkaiDBError> = bincode::deserialize(&value)
                            .map_err(|_| {
                                ShinkaiDBError::SomeError(
                                    "Failed converting execution context bytes to hashmap".to_string(),
                                )
                            });
                        if let Ok(context) = context {
                            execution_context = context;
                        }
                    }
                }
            }
        }
        let duration = start.elapsed();
        if std::env::var("DEBUG_TIMING").unwrap_or_default() == "true" {
            shinkai_log(
                ShinkaiLogOption::Database,
                ShinkaiLogLevel::Info,
                format!("get_job_execution_context execution time: {:?}", duration).as_str(),
            );
        }

        Ok(execution_context)
    }

    /// Fetches all unprocessed messages for a specific Job from the DB
    fn get_unprocessed_messages(&self, job_id: &str) -> Result<Vec<String>, ShinkaiDBError> {
        let prefix = format!("{}_unprocess_", job_id);
        let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();
        let mut unprocessed_messages: Vec<String> = Vec::new();

        let iter = self.db.prefix_iterator_cf(cf_inbox, prefix.as_bytes());
        for item in iter {
            let (_key, value) = item.map_err(|e| ShinkaiDBError::RocksDBError(e))?;
            let message = std::str::from_utf8(&value)?.to_string();
            unprocessed_messages.push(message);
        }

        Ok(unprocessed_messages)
    }

    /// Removes the oldest unprocessed message for a specific Job from the DB
    pub fn remove_oldest_unprocessed_message(&self, job_id: &str) -> Result<(), ShinkaiDBError> {
        let prefix = format!("{}_unprocess_", job_id);
        let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();

        let mut iter = self.db.prefix_iterator_cf(cf_inbox, prefix.as_bytes());
        if let Some(Ok((key, _))) = iter.next() {
            self.db.delete_cf(cf_inbox, &key)?;
        }

        Ok(())
    }

    /// Updates the Job to being finished
    pub fn update_job_to_finished(&self, job_id: &str) -> Result<(), ShinkaiDBError> {
        // Use shared CFs
        let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();

        // Construct key for checking if the job is finished
        let job_is_finished_key = format!("jobinbox_{}_is_finished", job_id);

        // Check if the job is already finished
        let is_finished_value = self
            .db
            .get_cf(cf_inbox, job_is_finished_key.as_bytes())?
            .ok_or(ShinkaiDBError::DataNotFound)?;
        let is_finished = std::str::from_utf8(&is_finished_value)?.to_string() == "true";

        if is_finished {
            return Err(ShinkaiDBError::SomeError(format!("Job {} is already finished", job_id)));
        }

        // Update the job to be marked as finished
        self.db.put_cf(cf_inbox, job_is_finished_key.as_bytes(), b"true")?;

        Ok(())
    }

    pub fn add_step_history(
        &mut self,
        job_id: String,
        user_message: String,
        agent_response: String,
        message_key: Option<String>,
    ) -> Result<(), ShinkaiDBError> {
        let message_key = match message_key {
            Some(key) => key,
            None => {
                // Fetch the most recent message from the job's inbox
                let inbox_name = InboxName::get_job_inbox_name_from_params(job_id.clone())?;
                let last_messages = self.get_last_messages_from_inbox(inbox_name.to_string(), 1, None)?;
                if let Some(message) = last_messages.first() {
                    if let Some(message) = message.first() {
                        message.calculate_message_hash_for_pagination()
                    } else {
                        return Err(ShinkaiDBError::SomeError("No messages found in the inbox".to_string()));
                    }
                } else {
                    return Err(ShinkaiDBError::SomeError("No messages found in the inbox".to_string()));
                }
            }
        };

        let key = format!("{}_{}_step_history", &job_id, &message_key);
        let current_time = ShinkaiStringTime::generate_time_now();

        // Create prompt & JobStepResult
        let mut prompt = Prompt::new();
        prompt.add_content(user_message, SubPromptType::User, 100);
        prompt.add_content(agent_response, SubPromptType::Assistant, 100);
        let mut job_step_result = JobStepResult::new();
        job_step_result.add_new_step_revision(prompt);

        // Convert to json and save to DB
        let json = job_step_result
            .to_json()
            .map_err(|e| ShinkaiDBError::DataConversionError(e.to_string()))?;

        // Use shared CFs
        let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();

        // Construct the key with the current time to ensure uniqueness
        let unique_key = format!("{}_{}", key, current_time);

        self.db.put_cf(cf_inbox, unique_key.as_bytes(), json.as_bytes())?;

        Ok(())
    }

    pub fn get_step_history(
        &self,
        job_id: &str,
        fetch_step_history: bool,
    ) -> Result<Option<Vec<JobStepResult>>, ShinkaiDBError> {
        if !fetch_step_history {
            return Ok(None);
        }

        let inbox_name = InboxName::get_job_inbox_name_from_params(job_id.to_string())?;
        let mut step_history: Vec<JobStepResult> = Vec::new();
        let mut until_offset_key: Option<String> = None;

        // Use shared CFs
        let cf_inbox = self.get_cf_handle(Topic::Inbox).unwrap();

        loop {
            // Note(Nico): changing n to 2 helps a lot to debug potential pagination problems
            let mut messages =
                self.get_last_messages_from_inbox(inbox_name.to_string(), 20, until_offset_key.clone())?;

            if messages.is_empty() {
                break;
            }

            messages.reverse();

            for message_path in &messages {
                if let Some(message) = message_path.first() {
                    let message_key = message.calculate_message_hash_for_pagination();
                    let key = format!("{}_{}_step_history", job_id, message_key);
                    if let Ok(Some(value)) = self.db.get_cf(cf_inbox, key.as_bytes()) {
                        let step_json_string = std::str::from_utf8(&value)?.to_string();
                        let step_res = JobStepResult::from_json(&step_json_string)?;
                        step_history.push(step_res);
                    }
                }
            }

            if let Some(last_message_path) = messages.last() {
                if let Some(last_message) = last_message_path.first() {
                    until_offset_key = Some(last_message.calculate_message_hash_for_pagination());
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        // Reverse the step history before returning
        step_history.reverse();
        Ok(Some(step_history))
    }

    pub fn is_job_inbox_empty(&self, job_id: &str) -> Result<bool, ShinkaiDBError> {
        eprintln!("jod_id is: {}", job_id);
        let hashed_job_id = Self::job_id_to_hash(job_id);
        let conversation_inbox_prefix = format!("inbox_{}_message_", hashed_job_id); // 47 characters so prefix works
        eprintln!("Conversation inbox prefix: {}", conversation_inbox_prefix);
        let cf_handle = self.get_cf_handle(Topic::Inbox).unwrap();

        let mut iter = self
            .db
            .prefix_iterator_cf(cf_handle, conversation_inbox_prefix.as_bytes());

        // Correctly handle the Result type yielded by the iterator
        for item in iter {
            match item {
                Ok((key, _value)) => {
                    println!("Key: {:?}", std::str::from_utf8(&key).unwrap_or("[Invalid UTF-8]"));
                }
                Err(e) => {
                    println!("Error reading from iterator: {:?}", e);
                    return Err(ShinkaiDBError::from(e)); // Assuming ShinkaiDBError can be constructed from rocksdb::Error
                }
            }
        }

        // Restart the iterator for the actual check
        let mut iter = self
            .db
            .prefix_iterator_cf(cf_handle, conversation_inbox_prefix.as_bytes());

        Ok(iter.next().is_none())
    }

    pub async fn add_message_to_job_inbox(
        &mut self,
        _: &str,
        message: &ShinkaiMessage,
        parent_message_key: Option<String>,
    ) -> Result<(), ShinkaiDBError> {
        self.unsafe_insert_inbox_message(&message, parent_message_key).await?;
        Ok(())
    }
}
