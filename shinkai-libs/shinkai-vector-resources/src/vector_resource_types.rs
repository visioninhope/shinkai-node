use super::base_vector_resources::BaseVectorResource;
use crate::base_vector_resources::VectorResourceBaseType;
use crate::embeddings::Embedding;
use crate::resource_errors::VectorResourceError;
use crate::source::VRSource;
use crate::vector_resource::VectorResource;
use ordered_float::NotNan;
use std::collections::HashMap;
use std::fmt;

/// A data chunk that was retrieved from a search.
/// Includes extra data like the resource_pointer of the resource it was from
/// and the similarity score from the vector search. Resource pointer is especially
/// helpful when you have multiple layers of VectorResources inside of each other.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RetrievedDataChunk {
    pub chunk: DataChunk,
    pub score: f32,
    pub resource_pointer: VectorResourcePointer,
    pub retrieval_path: VRPath,
}

impl RetrievedDataChunk {
    /// Create a new RetrievedDataChunk
    pub fn new(chunk: DataChunk, score: f32, resource_pointer: VectorResourcePointer, retrieval_path: VRPath) -> Self {
        Self {
            chunk,
            score,
            resource_pointer,
            retrieval_path,
        }
    }

    /// Sorts the list of RetrievedDataChunks based on their scores.
    /// Uses a binary heap for efficiency, returns num_results of highest scored.
    pub fn sort_by_score(retrieved_data: &Vec<RetrievedDataChunk>, num_results: u64) -> Vec<RetrievedDataChunk> {
        // Create a HashMap to store the RetrievedDataChunk instances for post-scoring retrieval
        let mut data_chunks: HashMap<String, RetrievedDataChunk> = HashMap::new();

        // Map the retrieved_data to a vector of tuples (NotNan<f32>, id_ref_key)
        // We create id_ref_key to support sorting RetrievedDataChunks from
        // different Resources together and avoid chunk id collision problems.
        let scores: Vec<(NotNan<f32>, String)> = retrieved_data
            .into_iter()
            .map(|data_chunk| {
                let ref_key = data_chunk.resource_pointer.reference.clone();
                let id_ref_key = format!("{}-{}", data_chunk.chunk.id.clone(), ref_key);
                data_chunks.insert(id_ref_key.clone(), data_chunk.clone());
                (NotNan::new(data_chunks[&id_ref_key].score).unwrap(), id_ref_key)
            })
            .collect();

        // Use the bin_heap_order_scores function to sort the scores
        let sorted_scores = Embedding::bin_heap_order_scores(scores, num_results as usize);

        // Map the sorted_scores back to a vector of RetrievedDataChunk
        let sorted_data: Vec<RetrievedDataChunk> = sorted_scores
            .into_iter()
            .map(|(_, id_db_key)| data_chunks[&id_db_key].clone())
            .collect();

        sorted_data
    }

    /// Formats the retrieval path to a string, adding a trailing `/`
    /// to denote that the retrieval path are the Vector Resources
    /// leading to this RetrievedDataChunk
    pub fn format_path_to_string(&self) -> String {
        let mut path_string = self.retrieval_path.format_to_string();
        if let NodeContent::Resource(_) = self.chunk.content {
            path_string.push('/');
        }
        path_string
    }

    /// Formats the data, source, and metadata of all provided `RetrievedDataChunk`s into a bullet-point
    /// list as a single string. This is to be included inside of a prompt to an LLM.
    /// Includes `max_characters` to allow specifying a hard-cap maximum that will be respected.
    pub fn format_ret_chunks_for_prompt(ret_data_chunks: Vec<RetrievedDataChunk>, max_characters: usize) -> String {
        if ret_data_chunks.is_empty() {
            return String::new();
        }

        let mut result = String::new();
        let mut remaining_chars = max_characters;

        for ret_chunk in ret_data_chunks {
            if let Some(formatted_chunk) = ret_chunk.format_for_prompt(remaining_chars) {
                if formatted_chunk.len() > remaining_chars {
                    break;
                }
                result.push_str(&formatted_chunk);
                result.push_str("\n\n ");
                remaining_chars -= formatted_chunk.len();
            }
        }

        result
    }

    /// Formats the data, source, and metadata together into a single string that is ready
    /// to be included as part of a prompt to an LLM.
    /// Includes `max_characters` to allow specifying a hard-cap maximum that will be respected.
    pub fn format_for_prompt(&self, max_characters: usize) -> Option<String> {
        let source_string = self.resource_pointer.resource_source.format_source_string();
        let metadata_string = self.format_metadata_string();

        let base_length = source_string.len() + metadata_string.len() + 20; // 20 chars of actual content as a minimum amount to bother including

        if base_length > max_characters {
            return None;
        }

        let data_string = self.chunk.get_data_string().ok()?;
        let data_length = max_characters - base_length;

        let data_string = if data_string.len() > data_length {
            data_string[..data_length].to_string()
        } else {
            data_string
        };

        let formatted_string = if metadata_string.len() > 0 {
            format!("- {} (Source: {}, {})", data_string, source_string, metadata_string)
        } else {
            format!("- {} (Source: {})", data_string, source_string)
        };

        Some(formatted_string)
    }

    /// Parses the metdata of the data chunk, and outputs a readable string which includes
    /// any metadata relevant to provide to an LLM as context about the retrieved chunk.
    pub fn format_metadata_string(&self) -> String {
        match &self.chunk.metadata {
            Some(metadata) => {
                if let Some(page_numbers) = metadata.get("page_numbers") {
                    format!("Pgs: {}", page_numbers)
                } else {
                    String::new()
                }
            }
            None => String::new(),
        }
    }
}

/// Represents a data chunk with an id, data, and optional metadata.
/// Note: `DataTag` type is excessively heavy when we convert to JSON, thus we just use the
/// data tag names instead in the DataChunk.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DataChunk {
    pub id: String,
    pub content: NodeContent,
    pub metadata: Option<HashMap<String, String>>,
    pub data_tag_names: Vec<String>,
}

impl DataChunk {
    /// Create a new String-holding DataChunk with a provided String id
    pub fn new(
        id: String,
        data: &str,
        metadata: Option<HashMap<String, String>>,
        data_tag_names: &Vec<String>,
    ) -> Self {
        Self {
            id,
            content: NodeContent::Text(data.to_string()),
            metadata,
            data_tag_names: data_tag_names.clone(),
        }
    }

    /// Create a new String-holding DataChunk with a provided u64 id, which gets converted to string internally
    pub fn new_with_integer_id(
        id: u64,
        data: &str,
        metadata: Option<HashMap<String, String>>,
        data_tag_names: &Vec<String>,
    ) -> Self {
        Self::new(id.to_string(), data, metadata, data_tag_names)
    }

    /// Create a new BaseVectorResource-holding DataChunk with a provided String id
    pub fn new_vector_resource(
        id: String,
        vector_resource: &BaseVectorResource,
        metadata: Option<HashMap<String, String>>,
    ) -> Self {
        DataChunk {
            id: id,
            content: NodeContent::Resource(vector_resource.clone()),
            metadata: metadata,
            data_tag_names: vector_resource.as_trait_object().data_tag_index().data_tag_names(),
        }
    }

    /// Create a new BaseVectorResource-holding DataChunk with a provided u64 id, which gets converted to string internally
    pub fn new_vector_resource_with_integer_id(
        id: u64,
        vector_resource: &BaseVectorResource,
        metadata: Option<HashMap<String, String>>,
    ) -> Self {
        Self::new_vector_resource(id.to_string(), vector_resource, metadata)
    }

    /// Attempts to read the data String from the DataChunk. Errors if data is a VectorResource
    pub fn get_data_string(&self) -> Result<String, VectorResourceError> {
        match &self.content {
            NodeContent::Text(s) => Ok(s.clone()),
            NodeContent::Resource(_) => Err(VectorResourceError::DataIsNonMatchingType),
        }
    }

    /// Attempts to read the BaseVectorResource from the DataChunk. Errors if data is an actual String
    pub fn get_data_vector_resource(&self) -> Result<BaseVectorResource, VectorResourceError> {
        match &self.content {
            NodeContent::Text(_) => Err(VectorResourceError::DataIsNonMatchingType),
            NodeContent::Resource(resource) => Ok(resource.clone()),
        }
    }
}

/// Contents of a DataChunk. Either the String text itself, or
/// another VectorResource
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum NodeContent {
    Text(String),
    Resource(BaseVectorResource),
}

/// Type which holds referential data about a given resource.
/// `reference` holds a string which points back to the original resource that
/// the pointer was created out of.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VectorResourcePointer {
    pub reference: String,
    pub resource_base_type: VectorResourceBaseType,
    pub resource_source: VRSource,
    pub data_tag_names: Vec<String>,
    pub resource_embedding: Option<Embedding>,
    // pub metadata: HashMap<String, String>,
}

impl VectorResourcePointer {
    /// Create a new VectorResourcePointer
    pub fn new(
        reference: &str,
        resource_base_type: VectorResourceBaseType,
        resource_embedding: Option<Embedding>,
        data_tag_names: Vec<String>,
        resource_source: VRSource,
    ) -> Self {
        Self {
            reference: reference.to_string(),
            resource_base_type,
            resource_embedding: resource_embedding.clone(),
            data_tag_names: data_tag_names,
            resource_source,
        }
    }

    /// Returns the name of the referenced resource, which is the part of the reference before the first ':'.
    /// If no ':' is found, the whole reference is returned.
    pub fn name(&self) -> String {
        match self.reference.find(':') {
            Some(index) => self.reference[..index].to_string(),
            None => self.reference.clone(),
        }
    }
}

impl From<Box<dyn VectorResource>> for VectorResourcePointer {
    fn from(resource: Box<dyn VectorResource>) -> Self {
        resource.get_resource_pointer()
    }
}

/// A path inside of a Vector Resource to an internal DataChunk.
/// Internally it is made up of an ordered list of data chunk ids.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VRPath {
    pub path_ids: Vec<String>,
}

impl VRPath {
    /// Create a new VRPath
    pub fn new() -> Self {
        Self { path_ids: vec![] }
    }

    /// Get the depth of the VRPath. Of note, this will return 0 in both cases if
    /// the path is empty, or if it is in the root path (because depth starts at 0
    /// for Vector Resources). This matches the TraversalMethod::UntilDepth interface.
    pub fn depth(&self) -> u64 {
        if self.path_ids.is_empty() {
            0
        } else {
            (self.path_ids.len() - 1) as u64
        }
    }

    /// Get the inclusive depth of the VRPath, meaning we include all parts of the path, including
    /// the final id. (In practice, generally +1 compared to .depth())
    pub fn depth_inclusive(&self) -> u64 {
        self.path_ids.len() as u64
    }

    /// Adds an element to the end of the path_ids
    pub fn push(&mut self, element: String) {
        self.path_ids.push(element);
    }

    /// Removes an element from the end of the path_ids
    pub fn pop(&mut self) -> Option<String> {
        self.path_ids.pop()
    }

    /// Creates a cloned VRPath and adds an element to the end
    pub fn push_cloned(&self, element: String) -> Self {
        let mut new_path = self.clone();
        new_path.push(element);
        new_path
    }

    /// Creates a cloned VRPath and removes an element from the end
    pub fn pop_cloned(&self) -> Self {
        let mut new_path = self.clone();
        new_path.pop();
        new_path
    }

    /// Create a VRPath from a path string
    pub fn from_path_string(path_ids_string: &str) -> Self {
        let path_ids_string = path_ids_string.trim_start_matches('/').trim_end_matches('/');
        let elements: Vec<&str> = path_ids_string.split('/').collect();
        let mut path = Self::new();
        for element in elements {
            path.push(element.to_string());
        }
        path
    }

    /// Formats the VRPath to a string
    pub fn format_to_string(&self) -> String {
        format!("/{}", self.path_ids.join("/"))
    }
}

impl fmt::Display for VRPath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", &self.format_to_string())
    }
}
