use crate::agent::queue::job_queue_manager::JobQueueManager;
use crate::db::{ShinkaiDB, Topic};
use crate::managers::IdentityManager;
use crate::network::subscription_manager::subscriber_manager_error::SubscriberManagerError;
use crate::network::Node;
use crate::vector_fs::vector_fs::VectorFS;
use crate::vector_fs::vector_fs_error::VectorFSError;
use crate::vector_fs::vector_fs_permissions::ReadPermission;
use crate::vector_fs::vector_fs_types::{FSEntry, FSFolder, FSItem};
use chrono::NaiveDateTime;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use shinkai_message_primitives::schemas::shinkai_name::ShinkaiName;
use shinkai_message_primitives::schemas::shinkai_subscription::{
    ShinkaiSubscription, ShinkaiSubscriptionAction, ShinkaiSubscriptionRequest,
};
use shinkai_message_primitives::schemas::shinkai_subscription_req::ShinkaiFolderSubscription;
use shinkai_vector_resources::vector_resource::VRPath;
use std::collections::HashMap;
use std::env;
use std::result::Result::Ok;
use std::sync::Arc;
use std::sync::Weak;
use tokio::sync::{Mutex, MutexGuard};

use super::fs_item_tree::FSItemTree;

/*
How to subscribe
- Node can scan multiple nodes and process what they offer. Endpoint: Node (External) -> Shareable stuff? (different than local node endpoint)
- User sees something that they like
- They subscribe to it
- Node validates and adds node to their subscriptors (maybe it should sync from the chain (?)) how do we know which subscription is which one?
  - can you be switching so you dont pay multiple subscriptions? -> maybe minimal time is good enough to avoid this
- Node processes the subscription and adds it to the queue
  - node ask the subscriber what they state
  - node calculates diff
  - node sends the diff to the subscriber
- Node checks for changes every X time and sends the diff to the subscriber in order to update the state
*/

// Message

// action: subscribe
// subscription_id: id
// vector_db: path
// state: [] or tree

// -> Validation ->

// it comes from the actual node
// vector_db path is valid and "shareable"
// state is valid
// delegation is enough

// -> Processing ->
// add node to the subscription_db
// schedule the job to be processed

// -> NetworkJobForProcessing -> (On Demand rather than calculated in advance)
// shinkai node name
// subscription_id
// vector_db
// state

/// Temp

// Who decides to split it? a cron? the SubscriberManager that checks the state? -> The subscriber manager

const NUM_THREADS: usize = 2;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct SharedFolderInfo {
    pub path: String,
    pub permission: String,
    pub tree: FSItemTree,
    pub subscription_requirement: Option<ShinkaiFolderSubscription>,
}

pub struct ExternalSubscriberManager {
    pub db: Weak<Mutex<ShinkaiDB>>,
    pub vector_fs: Weak<Mutex<VectorFS>>,
    pub identity_manager: Weak<Mutex<IdentityManager>>,
    pub subscriptions_queue_manager: Arc<Mutex<JobQueueManager<ShinkaiSubscription>>>,
    pub subscription_processing_task: Option<tokio::task::JoinHandle<()>>,
    pub shared_folders_trees: HashMap<String, Arc<SharedFolderInfo>>,
    pub node_name: ShinkaiName,
}

impl ExternalSubscriberManager {
    pub async fn new(
        db: Weak<Mutex<ShinkaiDB>>,
        vector_fs: Weak<Mutex<VectorFS>>,
        identity_manager: Weak<Mutex<IdentityManager>>,
        node_name: ShinkaiName,
    ) -> Self {
        let db_prefix = "subscriptions_abcprefix_";
        let subscriptions_queue = JobQueueManager::<ShinkaiSubscription>::new(
            db.clone(),
            Topic::AnyQueuesPrefixed.as_str(),
            Some(db_prefix.to_string()),
        )
        .await
        .unwrap();
        let subscriptions_queue_manager = Arc::new(Mutex::new(subscriptions_queue));

        let thread_number = env::var("SUBSCRIBER_MANAGER_NETWORK_CONCURRENCY")
            .unwrap_or(NUM_THREADS.to_string())
            .parse::<usize>()
            .unwrap_or(NUM_THREADS); // Start processing the job queue

        let subscription_queue_handler = ExternalSubscriberManager::process_subscription_queue(
            subscriptions_queue_manager.clone(),
            db.clone(),
            vector_fs.clone(),
            thread_number,
            |job, db, vector_fs| ExternalSubscriberManager::process_job_message_queued(job, db, vector_fs),
        )
        .await;

        ExternalSubscriberManager {
            db,
            vector_fs,
            identity_manager,
            subscriptions_queue_manager,
            subscription_processing_task: Some(subscription_queue_handler),
            shared_folders_trees: HashMap::new(),
            node_name,
        }
    }

    pub async fn get_cached_shared_folder_tree(&self, path: &str) -> Vec<Arc<SharedFolderInfo>> {
        if path == "/" {
            self.shared_folders_trees.values().cloned().collect()
        } else {
            self.shared_folders_trees
                .get(path)
                .cloned()
                .map_or_else(Vec::new, |v| vec![v])
        }
    }

    // Placeholder for process_job_message_queued
    // Correct the return type of the function to match the expected type
    fn process_job_message_queued(
        job: ShinkaiSubscription,
        db: Weak<Mutex<ShinkaiDB>>,
        vector_fs: Weak<Mutex<VectorFS>>,
    ) -> Box<dyn std::future::Future<Output = ()> + Send + 'static> {
        Box::new(async move {
            // Placeholder logic for processing a queued job message
            println!("Processing job: {:?}", job.subscription_id);

            // Simulate some processing work
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;

            // Log completion of job processing
            println!("Completed processing job: {:?}", job.subscription_id);
        })
    }

    pub async fn process_subscription_queue(
        job_queue_manager: Arc<Mutex<JobQueueManager<ShinkaiSubscription>>>,
        db: Weak<Mutex<ShinkaiDB>>,
        vector_fs: Weak<Mutex<VectorFS>>,
        thread_number: usize,
        process_job: impl Fn(
            ShinkaiSubscription,
            Weak<Mutex<ShinkaiDB>>,
            Weak<Mutex<VectorFS>>,
        ) -> Box<dyn std::future::Future<Output = ()> + Send + 'static>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut handles = Vec::new();
            for _ in 0..thread_number {
                let job_queue_manager = job_queue_manager.clone();
                let db = db.clone();
                let vector_fs = vector_fs.clone();
                let handle = tokio::spawn(async move {
                    loop {
                        match job_queue_manager.lock().await.dequeue("some_key").await {
                            Ok(Some(job)) => {
                                "hey_replace_me".to_string();
                            }
                            Ok(None) => break,
                            Err(err) => {
                                eprintln!("Error dequeuing job: {:?}", err);
                                break;
                            }
                        }
                    }
                });
                handles.push(handle);
            }
            for handle in handles {
                handle.await.unwrap();
            }
        })
    }

    /// The return type is (shareable_path, permission, tree, subscription_requirement)
    pub async fn available_shared_folders(
        &mut self,
        requester_shinkai_identity: ShinkaiName,
        path: String,
    ) -> Result<Vec<SharedFolderInfo>, SubscriberManagerError> {
        let mut converted_results = Vec::new();
        {
            let vector_fs = self
                .vector_fs
                .upgrade()
                .ok_or(SubscriberManagerError::VectorFSNotAvailable(
                    "VectorFS instance is not available".to_string(),
                ))?;
            let mut vector_fs = vector_fs.lock().await;

            eprintln!("path: {:?}", path);
            let vr_path =
                VRPath::from_string(&path).map_err(|e| SubscriberManagerError::InvalidRequest(e.to_string()))?;
            eprintln!("VR Path: {:?}", vr_path);

            let reader = vector_fs
                .new_reader(
                    requester_shinkai_identity.clone(),
                    vr_path,
                    requester_shinkai_identity.clone(),
                )
                .map_err(|e| SubscriberManagerError::InvalidRequest(e.to_string()))?;

            // Note: double-check that the Whitelist is correct here under these assumptions
            let results = vector_fs.find_paths_with_read_permissions(&reader, vec![ReadPermission::Public])?; // everything is whitelisted. I think it should be Private by default ReadPermission::Whitelist
            eprintln!("\n\n\n >>> Results: {:?}", results);

            // Use the new function to filter results to only include top-level folders
            let filtered_results = self.filter_to_top_level_folders(results);
            eprintln!("\n\n\n >>> Filtered Results: {:?}", filtered_results);

            // Drop the lock on vector_fs before proceeding
            drop(vector_fs);

            let db = self.db.upgrade().ok_or(SubscriberManagerError::DatabaseNotAvailable(
                "Database instance is not available".to_string(),
            ))?;
            let db = db.lock().await;

            for (path, permission) in filtered_results {
                let path_str = path.to_string();
                let permission_str = format!("{:?}", permission);
                let subscription_requirement = match db.get_folder_requirements(&path_str) {
                    Ok(req) => Some(req),
                    Err(_) => None, // Instead of erroring out, we return None for folders without requirements
                };
                let tree = self
                    .shared_folders_to_tree(requester_shinkai_identity.clone(), path_str.clone())
                    .await?;

                let result = SharedFolderInfo {
                    path: path_str.clone(),
                    permission: permission_str,
                    tree,
                    subscription_requirement,
                };
                converted_results.push(result.clone());
                eprintln!("Converted result: {:?}", result);
                self.shared_folders_trees.insert(path_str, Arc::new(result));
            }
        }

        // TODO: convert eprintlns to shinkai_logs
        eprintln!("Node name: {} Shared folders to tree...", self.node_name.clone());
        eprintln!("Converted results: {:?}", converted_results);

        Ok(converted_results)
    }

    pub async fn update_shareable_folder_requirements(
        &self,
        path: String,
        requester_shinkai_identity: ShinkaiName,
        subscription_requirement: ShinkaiFolderSubscription,
    ) -> Result<bool, SubscriberManagerError> {
        // TODO: check that you are actually an admin of the folder
        let vector_fs = self
            .vector_fs
            .upgrade()
            .ok_or(SubscriberManagerError::VectorFSNotAvailable(
                "VectorFS instance is not available".to_string(),
            ))?;
        let vector_fs = vector_fs.lock().await;

        let vr_path = VRPath::from_string(&path).map_err(|e| SubscriberManagerError::InvalidRequest(e.to_string()))?;
        let result = vector_fs.get_path_permission_for_paths(requester_shinkai_identity.clone(), vec![vr_path])?;

        // Checks that the permission is valid (Whitelist or Public)
        for (_, path_permission) in &result {
            if path_permission.read_permission != ReadPermission::Public
                && path_permission.read_permission != ReadPermission::Whitelist
            {
                return Err(SubscriberManagerError::InvalidRequest(
                    "Permission is not valid".to_string(),
                ));
            }
        }

        // Assuming we have validated the admin and permissions, we proceed to update the DB
        let db = self.db.upgrade().ok_or(SubscriberManagerError::DatabaseNotAvailable(
            "Database instance is not available".to_string(),
        ))?;
        let mut db = db.lock().await;

        db.set_folder_requirements(&path, subscription_requirement)
            .map_err(|e| SubscriberManagerError::DatabaseError(e.to_string()))?;

        Ok(true)
    }

    pub async fn create_shareable_folder(
        &mut self,
        path: String,
        requester_shinkai_identity: ShinkaiName,
        subscription_requirement: ShinkaiFolderSubscription,
    ) -> Result<bool, SubscriberManagerError> {
        // TODO: check that you are actually an admin of the folder
        // Note: I think is done automatically
        {
            let vector_fs = self
                .vector_fs
                .upgrade()
                .ok_or(SubscriberManagerError::VectorFSNotAvailable(
                    "VectorFS instance is not available".to_string(),
                ))?;
            let mut vector_fs = vector_fs.lock().await;

            let vr_path =
                VRPath::from_string(&path).map_err(|e| SubscriberManagerError::InvalidRequest(e.to_string()))?;
            let writer = vector_fs.new_writer(
                requester_shinkai_identity.clone(),
                vr_path.clone(),
                requester_shinkai_identity.clone(),
            )?;

            // Retrieve the current write permissions for the path
            let permissions_vector =
                vector_fs.get_path_permission_for_paths(requester_shinkai_identity.clone(), vec![vr_path.clone()])?;

            if permissions_vector.is_empty() {
                return Err(SubscriberManagerError::InvalidRequest(
                    "Path does not exist".to_string(),
                ));
            }

            let (_, current_permissions) = permissions_vector.into_iter().next().unwrap();

            // Set the read permissions to Public while reusing the write permissions
            vector_fs.update_permissions_recursively(
                &writer,
                ReadPermission::Public,
                current_permissions.write_permission,
            )?;
        }
        {
            // Assuming we have validated the admin and permissions, we proceed to update the DB
            let db = self.db.upgrade().ok_or(SubscriberManagerError::DatabaseNotAvailable(
                "Database instance is not available".to_string(),
            ))?;
            let mut db = db.lock().await;

            db.set_folder_requirements(&path, subscription_requirement)
                .map_err(|e| SubscriberManagerError::DatabaseError(e.to_string()))?;
        }

        // TODO: maybe call get shareable folders to trigger a refresh?

        Ok(true)
    }

    pub async fn unshare_folder(
        &mut self,
        path: String,
        requester_shinkai_identity: ShinkaiName,
    ) -> Result<bool, SubscriberManagerError> {
        {
            let vector_fs = self
                .vector_fs
                .upgrade()
                .ok_or(SubscriberManagerError::VectorFSNotAvailable(
                    "VectorFS instance is not available".to_string(),
                ))?;
            let mut vector_fs = vector_fs.lock().await;

            // Retrieve the current permissions for the path
            let permissions_vector = vector_fs
                .get_path_permission_for_paths(requester_shinkai_identity.clone(), vec![VRPath::from_string(&path)?])?;

            if permissions_vector.is_empty() {
                return Err(SubscriberManagerError::InvalidRequest(
                    "Path does not exist".to_string(),
                ));
            }

            let (vr_path, current_permissions) = permissions_vector.into_iter().next().unwrap();

            // Create a writer for the path
            let writer = vector_fs.new_writer(
                requester_shinkai_identity.clone(),
                vr_path,
                requester_shinkai_identity.clone(),
            )?;

            // Set the read permissions to Private while reusing the write permissions using update_permissions_recursively
            vector_fs.update_permissions_recursively(
                &writer,
                ReadPermission::Private,
                current_permissions.write_permission,
            )?;
        }
        {
            // Assuming we have validated the admin and permissions, we proceed to update the DB
            let db = self.db.upgrade().ok_or(SubscriberManagerError::DatabaseNotAvailable(
                "Database instance is not available".to_string(),
            ))?;
            let mut db = db.lock().await;
            db.remove_folder_requirements(&path)
                .map_err(|e| SubscriberManagerError::DatabaseError(e.to_string()))?;
        }

        self.shared_folders_trees.remove(&path);
        Ok(true)
    }

    pub async fn call_subscriber_test_fn(&self) -> Result<String, SubscriberManagerError> {
        Ok("".to_string())
    }

    // Schedule the job to be processed. Update the last time it was processed.
    pub async fn process_action(
        &mut self,
        action: ShinkaiSubscriptionRequest,
        profile: ShinkaiName,
    ) -> Result<String, SubscriberManagerError> {
        // TODO: Transform request to ShinkaiSubscription if it passes validation
        // -> Validation ->

        // it comes from the actual node (API side) -> it should be validated before it gets here
        // vector_db path is valid and "shareable" (here)

        match action.action {
            ShinkaiSubscriptionAction::Subscribe => {
                // Transform request to ShinkaiSubscription if it passes validation
                let subscription = ShinkaiSubscription {
                    action: action.action,
                    subscription_id: action.subscription_id,
                    vector_db_path: action.vector_db_path.ok_or(SubscriberManagerError::InvalidRequest(
                        "vector_db_path is required".to_string(),
                    ))?,
                    subscriber_identity: profile,
                    state: action.state,
                    date_created: Utc::now(),
                    last_modified: Utc::now(),
                    last_sync: None,
                };

                // TODO: vector_db path is valid and public
                let vector_fs = self
                    .vector_fs
                    .upgrade()
                    .ok_or(SubscriberManagerError::VectorFSNotAvailable(
                        "VectorFS instance is not available".to_string(),
                    ))?;
                let vector_fs = vector_fs.lock().await;

                // it's not already registered (here)
                // state is valid (here)
                // delegation is enough (here -> yeah we need to do logic about what is what in terms of shareables)

                // TODO: Add fn to add allowed vector_db paths for externals

                // -> Processing ->
                // add node to the subscription_db
                // schedule the job to be processed
                // Further processing and validation here

                Ok("Subscription processed".to_string())
            }
            // Handle other actions as needed
            _ => Err(SubscriberManagerError::InvalidRequest(
                "Unsupported action type".to_string(),
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::from_str;
    use shinkai_message_primitives::schemas::shinkai_subscription_req::PaymentOption;

    #[test]
    fn test_convert_string_to_shared_folder_info() {
        let json_str = r#"[{"path":"/shared_test_folder","permission":"Public","tree":{"name":"/","path":"/shared_test_folder","last_modified":"2024-03-24T00:11:29.958427+00:00","children":{"crypto":{"name":"crypto","path":"/shared_test_folder/crypto","last_modified":"2024-03-24T00:11:27.905905+00:00","children":{"shinkai_intro":{"name":"shinkai_intro","path":"/shared_test_folder/crypto/shinkai_intro","last_modified":"2024-02-26T23:06:00.019065981+00:00","children":{}}}}}},"subscription_requirement":{"minimum_token_delegation":100,"minimum_time_delegated_hours":100,"monthly_payment":{"USD":10.0},"is_free":false}}]"#;

        let shared_folder_info: Vec<SharedFolderInfo> = from_str(json_str).unwrap();

        assert_eq!(shared_folder_info.len(), 1);
        let folder_info = &shared_folder_info[0];
        assert_eq!(folder_info.path, "/shared_test_folder");
        assert_eq!(folder_info.permission, "Public");
        assert!(folder_info.subscription_requirement.is_some());
        let subscription_requirement = folder_info.subscription_requirement.as_ref().unwrap();
        assert_eq!(subscription_requirement.minimum_token_delegation, Some(100));
        assert_eq!(subscription_requirement.minimum_time_delegated_hours, Some(100));
        assert_eq!(
            match subscription_requirement.monthly_payment {
                Some(PaymentOption::USD(amount)) => Some(amount),
                _ => None,
            },
            Some(10.0)
        );
        assert_eq!(subscription_requirement.is_free, false);
    }
}
