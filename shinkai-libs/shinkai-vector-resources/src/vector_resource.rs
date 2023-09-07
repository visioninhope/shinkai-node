use super::base_vector_resources::BaseVectorResource;
use crate::base_vector_resources::VectorResourceBaseType;
use crate::data_tags::DataTagIndex;
use crate::embedding_generator::EmbeddingGenerator;
use crate::embeddings::Embedding;
use crate::embeddings::MAX_EMBEDDING_STRING_SIZE;
use crate::model_type::EmbeddingModelType;
use crate::resource_errors::VectorResourceError;
use ordered_float::NotNan;
use std::collections::HashMap;

/// Contents of a DataChunk. Either the String data itself, or
/// another VectorResource
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum DataContent {
    Data(String),
    Resource(BaseVectorResource),
}

/// A data chunk that was retrieved from a vector search.
/// Includes extra data like the resource_id of the resource it was from
/// and the vector search score.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct RetrievedDataChunk {
    pub chunk: DataChunk,
    pub score: f32,
    pub resource_pointer: VectorResourcePointer,
}

impl RetrievedDataChunk {
    /// Sorts the list of RetrievedDataChunks based on their scores.
    /// Uses a binary heap for efficiency, returns num_results of highest scored.
    pub fn sort_by_score(retrieved_data: &Vec<RetrievedDataChunk>, num_results: u64) -> Vec<RetrievedDataChunk> {
        // Create a HashMap to store the RetrievedDataChunk instances for post-scoring retrieval
        let mut data_chunks: HashMap<String, RetrievedDataChunk> = HashMap::new();

        // Map the retrieved_data to a vector of tuples (NotNan<f32>, id_db_key)
        // We create id_db_key to support sorting RetrievedDataChunks from
        // different Resources together and avoid chunk id collision problems.
        let scores: Vec<(NotNan<f32>, String)> = retrieved_data
            .into_iter()
            .map(|data_chunk| {
                let db_key = data_chunk.resource_pointer.shinkai_db_key.clone();
                let id_db_key = format!("{}-{}", data_chunk.chunk.id.clone(), db_key);
                data_chunks.insert(id_db_key.clone(), data_chunk.clone());
                (NotNan::new(data_chunks[&id_db_key].score).unwrap(), id_db_key)
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
}

/// Represents a data chunk with an id, data, and optional metadata.
/// Note: `DataTag` type is excessively heavy when we convert to JSON, thus we just use the
/// data tag names instead in the DataChunk.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DataChunk {
    pub id: String,
    pub data: DataContent,
    pub metadata: Option<HashMap<String, String>>,
    pub data_tag_names: Vec<String>,
}

impl DataChunk {
    pub fn new(
        id: String,
        data: &str,
        metadata: Option<HashMap<String, String>>,
        data_tag_names: &Vec<String>,
    ) -> Self {
        Self {
            id,
            data: DataContent::Data(data.to_string()),
            metadata,
            data_tag_names: data_tag_names.clone(),
        }
    }

    pub fn new_with_integer_id(
        id: u64,
        data: &str,
        metadata: Option<HashMap<String, String>>,
        data_tag_names: &Vec<String>,
    ) -> Self {
        Self::new(id.to_string(), data, metadata, data_tag_names)
    }

    pub fn new_vector_resource(
        id: String,
        vector_resource: &BaseVectorResource,
        metadata: Option<HashMap<String, String>>,
    ) -> Self {
        DataChunk {
            id: id,
            data: DataContent::Resource(vector_resource.clone()),
            metadata: metadata,
            data_tag_names: vector_resource.as_trait_object().data_tag_index().data_tag_names(),
        }
    }

    pub fn new_vector_resource_with_integer_id(
        id: u64,
        vector_resource: &BaseVectorResource,
        metadata: Option<HashMap<String, String>>,
    ) -> Self {
        Self::new_vector_resource(id.to_string(), vector_resource, metadata)
    }

    /// Attempts to read the data String from the DataChunk. Errors if data is a VectorResource
    pub fn get_data_string(&self) -> Result<String, VectorResourceError> {
        match &self.data {
            DataContent::Data(s) => Ok(s.clone()),
            DataContent::Resource(_) => Err(VectorResourceError::DataIsNonMatchingType),
        }
    }

    /// Attempts to read the BaseVectorResource from the DataChunk. Errors if data is an actual String
    pub fn get_data_vector_resource(&self) -> Result<BaseVectorResource, VectorResourceError> {
        match &self.data {
            DataContent::Data(_) => Err(VectorResourceError::DataIsNonMatchingType),
            DataContent::Resource(resource) => Ok(resource.clone()),
        }
    }
}

/// Type which holds data about a stored resource in the DB.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct VectorResourcePointer {
    pub shinkai_db_key: String,
    pub resource_base_type: VectorResourceBaseType,
    data_tag_names: Vec<String>,
    resource_embedding: Option<Embedding>,
}

impl VectorResourcePointer {
    /// Create a new VectorResourcePointer
    pub fn new(
        shinkai_db_key: &str,
        resource_base_type: VectorResourceBaseType,
        resource_embedding: Option<Embedding>,
        data_tag_names: Vec<String>,
    ) -> Self {
        Self {
            shinkai_db_key: shinkai_db_key.to_string(),
            resource_base_type,
            resource_embedding: resource_embedding.clone(),
            data_tag_names: data_tag_names,
        }
    }
}

impl From<Box<dyn VectorResource>> for VectorResourcePointer {
    fn from(resource: Box<dyn VectorResource>) -> Self {
        resource.get_resource_pointer()
    }
}

/// Represents a VectorResource as an abstract trait that anyone can implement new variants of.
/// Of note, when working with multiple VectorResources/the Shinkai DB, the `name` field can have duplicates,
/// but `resource_id` is expected to be unique.
pub trait VectorResource {
    fn name(&self) -> &str;
    fn description(&self) -> Option<&str>;
    fn source(&self) -> Option<&str>;
    fn resource_id(&self) -> &str;
    fn resource_embedding(&self) -> &Embedding;
    fn set_resource_embedding(&mut self, embedding: Embedding);
    fn resource_base_type(&self) -> VectorResourceBaseType;
    fn embedding_model_used(&self) -> EmbeddingModelType;
    fn set_embedding_model_used(&mut self, model_type: EmbeddingModelType);
    fn chunk_embeddings(&self) -> Vec<Embedding>;
    fn data_tag_index(&self) -> &DataTagIndex;
    fn get_chunk_embedding(&self, id: String) -> Result<Embedding, VectorResourceError>;
    /// Retrieves a data chunk given its id.
    fn get_data_chunk(&self, id: String) -> Result<DataChunk, VectorResourceError>;
    // Note we cannot add from_json in the trait due to trait object limitations
    fn to_json(&self) -> Result<String, VectorResourceError>;

    /// Returns a String representing the Key that this VectorResource
    /// will be/is saved to in the Topic::VectorResources in the DB.
    /// The db key is: `{name}.{resource_id}`
    fn shinkai_db_key(&self) -> String {
        let name = self.name().replace(" ", "_");
        let resource_id = self.resource_id().replace(" ", "_");
        format!("{}.{}", name, resource_id)
    }

    /// Validates whether the VectorResource has a valid  BaseVectorResourceType by checking its .resource_base_type()
    fn is_base_vector_resource(&self) -> Result<(), VectorResourceError> {
        VectorResourceBaseType::is_base_vector_resource(self.resource_base_type())
    }

    /// Regenerates and updates the resource's embedding.
    fn update_resource_embedding(
        &mut self,
        generator: &dyn EmbeddingGenerator,
        keywords: Vec<String>,
    ) -> Result<(), VectorResourceError> {
        let formatted = self.format_embedding_string(keywords);
        let new_embedding = generator.generate_embedding(&formatted, "RE")?;
        self.set_resource_embedding(new_embedding);
        Ok(())
    }

    /// Generates a formatted string that represents the data to be used for the
    /// resource embedding.
    fn format_embedding_string(&self, keywords: Vec<String>) -> String {
        let name = format!("Name: {}", self.name());
        let desc = self
            .description()
            .map(|description| format!(", Description: {}", description))
            .unwrap_or_default();
        let source = self
            .source()
            .map(|source| format!(", Source: {}", source))
            .unwrap_or_default();

        // Take keywords until we hit an upper 500 character cap to ensure
        // we do not go past the embedding LLM context window.
        let pre_keyword_length = name.len() + desc.len() + source.len();
        let mut keyword_string = String::new();
        for phrase in keywords {
            if pre_keyword_length + keyword_string.len() + phrase.len() <= MAX_EMBEDDING_STRING_SIZE {
                keyword_string = format!("{}, {}", keyword_string, phrase);
            }
        }

        format!("{}{}{}, Keywords: [{}]", name, desc, source, keyword_string)
    }

    /// Generates a pointer out of the resource.
    fn get_resource_pointer(&self) -> VectorResourcePointer {
        let shinkai_db_key = self.shinkai_db_key();
        let resource_type = self.resource_base_type();
        let embedding = self.resource_embedding().clone();

        // Fetch list of data tag names from the index
        let tag_names = self.data_tag_index().data_tag_names();

        VectorResourcePointer::new(&shinkai_db_key, resource_type, Some(embedding), tag_names)
    }

    /// Performs a vector search that returns the most similar data chunks based on the query. Of note this goes over all
    /// Vector Resources held inside of self, and only searches inside of them if their resource embedding is sufficiently
    /// similar to meet the `num_of_results` at that given level of depth.
    fn vector_search(&self, query: Embedding, num_of_results: u64) -> Vec<RetrievedDataChunk> {
        // Fetch the ordered scores from the abstracted function
        let scores = query.score_similarities(&self.chunk_embeddings(), num_of_results);

        self._order_vector_search_results(scores, query, num_of_results, &vec![])
    }

    /// Performs a syntactic vector search, aka efficiently pre-filtering to only search through DataChunks matching the list of data tag names.
    /// Of note this goes over all Vector Resources held inside of self, and only searches inside of them if they both have
    /// matching data tags and their resource embedding is sufficiently similar to meet the `num_of_results` at that given level of depth.
    fn syntactic_vector_search(
        &self,
        query: Embedding,
        num_of_results: u64,
        data_tag_names: &Vec<String>,
    ) -> Vec<RetrievedDataChunk> {
        // Fetch all data chunks with matching data tags
        let mut matching_data_tag_embeddings = vec![];
        let ids = self._syntactic_search_id_fetch(data_tag_names);
        for id in ids {
            if let Ok(embedding) = self.get_chunk_embedding(id) {
                matching_data_tag_embeddings.push(embedding);
            }
        }
        // Score the embeddings and return only num_of_results most similar
        let scores = query.score_similarities(&matching_data_tag_embeddings, num_of_results);

        self._order_vector_search_results(scores, query, num_of_results, data_tag_names)
    }

    /// Fetches all data chunks which contain tags matching the input name list
    /// (including fetching inside all levels of Vector Resources, akin to vector searches)
    fn syntactic_search(&self, data_tag_names: &Vec<String>) -> Vec<RetrievedDataChunk> {
        // Fetch all data chunks with matching data tags
        let mut matching_data_chunks = vec![];
        let ids = self._syntactic_search_id_fetch(data_tag_names);
        for id in ids {
            if let Ok(data_chunk) = self.get_data_chunk(id.clone()) {
                match data_chunk.data {
                    DataContent::Resource(resource) => {
                        let sub_results = resource.as_trait_object().syntactic_search(data_tag_names);
                        matching_data_chunks.extend(sub_results);
                    }
                    DataContent::Data(_) => {
                        let resource_pointer = self.get_resource_pointer();
                        let retrieved_data_chunk = RetrievedDataChunk {
                            chunk: data_chunk,
                            score: 0.0,
                            resource_pointer,
                        };
                        matching_data_chunks.push(retrieved_data_chunk);
                    }
                }
            }
        }

        matching_data_chunks
    }

    /// Internal method to fetch all chunk ids for syntactic searches
    fn _syntactic_search_id_fetch(&self, data_tag_names: &Vec<String>) -> Vec<String> {
        let mut ids = vec![];
        for name in data_tag_names {
            if let Some(chunk_ids) = self.data_tag_index().get_chunk_ids(&name) {
                ids.extend(chunk_ids.iter().map(|id| id.to_string()));
            }
        }
        ids
    }

    /// Internal method shared by vector_search() and syntactic_vector_search() that
    /// orders all scores, and importantly resolves any BaseVectorResources which were
    /// in the DataChunks of the most similar results.
    fn _order_vector_search_results(
        &self,
        scores: Vec<(f32, String)>,
        query: Embedding,
        num_of_results: u64,
        data_tag_names: &Vec<String>,
    ) -> Vec<RetrievedDataChunk> {
        let mut first_level_results: Vec<RetrievedDataChunk> = vec![];
        let mut vector_resource_count = 0;
        for (score, id) in scores {
            if let Ok(chunk) = self.get_data_chunk(id) {
                match chunk.data {
                    DataContent::Resource(resource) => {
                        vector_resource_count += 1;
                        // If no data tag names provided, it means we are doing a normal vector search
                        let sub_results = if data_tag_names.is_empty() {
                            resource.as_trait_object().vector_search(query.clone(), num_of_results)
                        } else {
                            resource.as_trait_object().syntactic_vector_search(
                                query.clone(),
                                num_of_results,
                                data_tag_names,
                            )
                        };
                        first_level_results.extend(sub_results);
                    }
                    DataContent::Data(_) => {
                        first_level_results.push(RetrievedDataChunk {
                            chunk: chunk.clone(),
                            score,
                            resource_pointer: self.get_resource_pointer(),
                        });
                    }
                }
            }
        }

        // If at least one vector resource exists in the DataChunks then re-sort
        // after fetching deeper level results to ensure ordering are correct
        if vector_resource_count >= 1 {
            return RetrievedDataChunk::sort_by_score(&first_level_results, num_of_results);
        }
        // Otherwise just return 1st level results
        first_level_results
    }

    /// Performs a vector search using a query embedding and returns
    /// the most similar data chunks within a specific range.
    ///
    /// * `tolerance_range` - A float between 0 and 1, inclusive, that
    ///   determines the range of acceptable similarity scores as a percentage
    ///   of the highest score.
    fn vector_search_tolerance_ranged(&self, query: Embedding, tolerance_range: f32) -> Vec<RetrievedDataChunk> {
        // Get top 100 results
        let results = self.vector_search(query.clone(), 100);

        // Calculate the top similarity score
        let top_similarity_score = results.first().map_or(0.0, |ret_chunk| ret_chunk.score);

        // Find the range of acceptable similarity scores
        self.vector_search_tolerance_ranged_score(query, tolerance_range, top_similarity_score)
    }

    /// Performs a vector search using a query embedding and returns
    /// the most similar data chunks within a specific range of the provided top similarity score.
    ///
    /// * `top_similarity_score` - A float that represents the top similarity score.
    fn vector_search_tolerance_ranged_score(
        &self,
        query: Embedding,
        tolerance_range: f32,
        top_similarity_score: f32,
    ) -> Vec<RetrievedDataChunk> {
        // Clamp the tolerance_range to be between 0 and 1
        let tolerance_range = tolerance_range.max(0.0).min(1.0);

        let mut results = self.vector_search(query, 100);

        // Calculate the range of acceptable similarity scores
        let lower_bound = top_similarity_score * (1.0 - tolerance_range);

        // Filter the results to only include those within the range of the top similarity score
        results.retain(|ret_chunk| ret_chunk.score >= lower_bound && ret_chunk.score <= top_similarity_score);

        results
    }
}
