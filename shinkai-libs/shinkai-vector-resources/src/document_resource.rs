use crate::base_vector_resources::{BaseVectorResource, VRBaseType};
use crate::data_tags::{DataTag, DataTagIndex};
use crate::embeddings::Embedding;
use crate::metadata_index::MetadataIndex;
use crate::model_type::{EmbeddingModelType, TextEmbeddingsInference};
use crate::resource_errors::VRError;
use crate::shinkai_time::ShinkaiTime;
use crate::source::{ShinkaiFileType, SourceReference, VRSource};
use crate::vector_resource::{
    Node, NodeContent, RetrievedNode, TraversalMethod, TraversalOption, VRPath, VectorResource,
};
use crate::vector_search_traversal::VRHeader;
use serde_json;
use std::collections::HashMap;

/// A VectorResource which uses an internal numbered/ordered list data model,  
/// thus providing an ideal interface for document-like content such as PDFs,
/// epubs, web content, written works, and more.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DocumentVectorResource {
    name: String,
    description: Option<String>,
    source: VRSource,
    resource_id: String,
    resource_embedding: Embedding,
    embedding_model_used: EmbeddingModelType,
    resource_base_type: VRBaseType,
    embeddings: Vec<Embedding>,
    node_count: u64,
    nodes: Vec<Node>,
    data_tag_index: DataTagIndex,
    created_datetime: String,
    last_modified_datetime: String,
    metadata_index: MetadataIndex,
}

impl VectorResource for DocumentVectorResource {
    /// RFC3339 Datetime when then Vector Resource was created
    fn created_datetime(&self) -> String {
        self.created_datetime.clone()
    }
    /// RFC3339 Datetime when then Vector Resource was last modified
    fn last_modified_datetime(&self) -> String {
        self.last_modified_datetime.clone()
    }
    /// Set a RFC Datetime of when then Vector Resource was last modified
    fn set_last_modified_datetime(&mut self, datetime: String) -> Result<(), VRError> {
        if ShinkaiTime::validate_datetime_string(&datetime) {
            self.last_modified_datetime = datetime;
            return Ok(());
        }
        return Err(VRError::InvalidDateTimeString(datetime));
    }

    fn data_tag_index(&self) -> &DataTagIndex {
        &self.data_tag_index
    }

    fn metadata_index(&self) -> &MetadataIndex {
        &self.metadata_index
    }

    fn embedding_model_used(&self) -> EmbeddingModelType {
        self.embedding_model_used.clone()
    }

    fn name(&self) -> &str {
        &self.name
    }

    fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    fn source(&self) -> VRSource {
        self.source.clone()
    }

    fn resource_id(&self) -> &str {
        &self.resource_id
    }

    fn resource_embedding(&self) -> &Embedding {
        &self.resource_embedding
    }

    fn resource_base_type(&self) -> VRBaseType {
        self.resource_base_type.clone()
    }

    fn get_embeddings(&self) -> Vec<Embedding> {
        self.embeddings.clone()
    }

    fn to_json(&self) -> Result<String, VRError> {
        serde_json::to_string(self).map_err(|_| VRError::FailedJSONParsing)
    }

    fn set_embedding_model_used(&mut self, model_type: EmbeddingModelType) {
        self.update_last_modified_to_now();
        self.embedding_model_used = model_type;
    }

    fn set_resource_embedding(&mut self, embedding: Embedding) {
        self.update_last_modified_to_now();
        self.resource_embedding = embedding;
    }

    fn set_resource_id(&mut self, id: String) {
        self.update_last_modified_to_now();
        self.resource_id = id;
    }

    /// Efficiently retrieves a Node's matching embedding given its id by fetching it via index.
    fn get_embedding(&self, id: String) -> Result<Embedding, VRError> {
        let id = id.parse::<u64>().map_err(|_| VRError::InvalidNodeId)?;
        if id == 0 || id > self.node_count {
            return Err(VRError::InvalidNodeId);
        }
        let index = id.checked_sub(1).ok_or(VRError::InvalidNodeId)? as usize;
        Ok(self.embeddings[index].clone())
    }

    /// Efficiently retrieves a node given its id by fetching it via index.
    fn get_node(&self, id: String) -> Result<Node, VRError> {
        let id = id.parse::<u64>().map_err(|_| VRError::InvalidNodeId)?;
        if id == 0 || id > self.node_count {
            return Err(VRError::InvalidNodeId);
        }
        let index = id.checked_sub(1).ok_or(VRError::InvalidNodeId)? as usize;
        self.nodes.get(index).cloned().ok_or(VRError::InvalidNodeId)
    }

    /// Returns all nodes in the DocumentVectorResource
    fn get_nodes(&self) -> Vec<Node> {
        self.nodes.iter().cloned().collect()
    }
}

impl DocumentVectorResource {
    /// Create a new MapVectorResource
    pub fn new(
        name: &str,
        desc: Option<&str>,
        source: VRSource,
        resource_embedding: Embedding,
        embeddings: Vec<Embedding>,
        nodes: Vec<Node>,
        embedding_model_used: EmbeddingModelType,
    ) -> Self {
        let current_time = ShinkaiTime::generate_time_now();
        let mut resource = DocumentVectorResource {
            name: String::from(name),
            description: desc.map(String::from),
            source: source,
            resource_id: String::from("default"),
            resource_embedding,
            embeddings,
            node_count: nodes.len() as u64,
            nodes: nodes,
            embedding_model_used,
            resource_base_type: VRBaseType::Document,
            data_tag_index: DataTagIndex::new(),
            created_datetime: current_time.clone(),
            last_modified_datetime: current_time,
            metadata_index: MetadataIndex::new(),
        };

        // Generate a unique resource_id:
        resource.generate_and_update_resource_id();
        resource
    }

    /// Initializes an empty `DocumentVectorResource` with empty defaults.
    pub fn new_empty(name: &str, desc: Option<&str>, source: VRSource) -> Self {
        DocumentVectorResource::new(
            name,
            desc,
            source,
            Embedding::new(&String::new(), vec![]),
            Vec::new(),
            Vec::new(),
            EmbeddingModelType::TextEmbeddingsInference(TextEmbeddingsInference::AllMiniLML6v2),
        )
    }

    /// Performs a vector search using a query embedding, and then
    /// fetches a specific number of Nodes below and above the most
    /// similar Node.
    /// TODO: Update to traverse past root depth and properly fetch
    /// surrounding nodes using path
    pub fn vector_search_proximity(
        &self,
        query: Embedding,
        proximity_window: u64,
    ) -> Result<Vec<RetrievedNode>, VRError> {
        let search_results = self.vector_search_customized(
            query,
            1,
            TraversalMethod::Exhaustive,
            &vec![TraversalOption::UntilDepth(0)],
            // &vec![TraversalOption::LimitTraversalToType(VRBaseType::Document)],
            None,
        );
        let most_similar_node = search_results.first().ok_or(VRError::VectorResourceEmpty)?;
        let most_similar_id = most_similar_node
            .node
            .id
            .parse::<u64>()
            .map_err(|_| VRError::InvalidNodeId)?;

        // Get Start/End ids
        let start_id = if most_similar_id >= proximity_window {
            most_similar_id - proximity_window
        } else {
            1
        };
        let end_id = if let Some(end_boundary) = self.node_count.checked_sub(1) {
            if let Some(potential_end_id) = most_similar_id.checked_add(proximity_window) {
                potential_end_id.min(end_boundary)
            } else {
                end_boundary // Or any appropriate default
            }
        } else {
            1
        };

        // Acquire surrounding nodes
        let mut nodes = Vec::new();
        for id in start_id..=(end_id + 1) {
            if let Ok(node) = self.get_node(id.to_string()) {
                nodes.push(RetrievedNode {
                    node: node.clone(),
                    score: 0.00,
                    resource_header: self.generate_resource_header(None),
                    retrieval_path: VRPath::new(),
                });
            }
        }

        Ok(nodes)
    }

    /// Appends a new node (with a BaseVectorResource) to the document
    /// Uses the supplied Embedding rather than
    /// automatically using the resource's embedding (useful when Embedding models differ).
    pub fn append_vector_resource_node(
        &mut self,
        resource: BaseVectorResource,
        metadata: Option<HashMap<String, String>>,
        embedding: &Embedding,
    ) {
        let tag_names = resource.as_trait_object().data_tag_index().data_tag_names();
        self._append_node_without_tag_validation(NodeContent::Resource(resource), metadata, embedding, &tag_names)
    }

    /// Appends a new node (with a BaseVectorResource) to the document
    /// Automatically uses the existing resource embedding.
    pub fn append_vector_resource_node_auto(
        &mut self,
        resource: BaseVectorResource,
        metadata: Option<HashMap<String, String>>,
    ) {
        let embedding = resource.as_trait_object().resource_embedding().clone();
        self.append_vector_resource_node(resource, metadata, &embedding)
    }

    /// Appends a new text node and an associated embedding to the document.
    pub fn append_text_node(
        &mut self,
        text: &str,
        metadata: Option<HashMap<String, String>>,
        embedding: &Embedding,
        parsing_tags: &Vec<DataTag>, // list of datatags you want to parse the data with
    ) {
        let validated_data_tags = DataTag::validate_tag_list(text, parsing_tags);
        let data_tag_names = validated_data_tags.iter().map(|tag| tag.name.clone()).collect();
        self._append_node_without_tag_validation(
            NodeContent::Text(text.to_string()),
            metadata,
            embedding,
            &data_tag_names,
        )
    }

    /// Appends a new node (with ExternalContent) to the document.
    /// Uses the supplied Embedding.
    pub fn append_external_content_node(
        &mut self,
        external_content: SourceReference,
        metadata: Option<HashMap<String, String>>,
        embedding: &Embedding,
    ) {
        // As ExternalContent doesn't have data tags, we pass an empty vector
        self._append_node_without_tag_validation(
            NodeContent::ExternalContent(external_content),
            metadata,
            embedding,
            &Vec::new(),
        )
    }

    /// Appends a new node (with VRHeader) to the document.
    /// Uses the supplied Embedding.
    pub fn append_vr_header_node(
        &mut self,
        vr_header: VRHeader,
        metadata: Option<HashMap<String, String>>,
        embedding: &Embedding,
    ) {
        let data_tag_names = vr_header.data_tag_names.clone();
        self._append_node_without_tag_validation(NodeContent::VRHeader(vr_header), metadata, embedding, &data_tag_names)
    }

    /// Appends a new text node and associated embedding to the document
    /// without checking if tags are valid. Used for internal purposes.
    pub fn _append_node_without_tag_validation(
        &mut self,
        data: NodeContent,
        metadata: Option<HashMap<String, String>>,
        embedding: &Embedding,
        tag_names: &Vec<String>,
    ) {
        let id = self.node_count + 1;
        let node = Node::from_node_content_with_integer_id(id, data.clone(), metadata.clone(), tag_names.clone());
        // Embedding details
        let mut embedding = embedding.clone();
        embedding.set_id_with_integer(id);

        self.data_tag_index.add_node(&node);
        self.metadata_index.add_node(&node);
        self._append_node(node);
        self.embeddings.push(embedding);
        self.update_last_modified_to_now();
    }

    /// Replaces an existing node and associated embedding in the Document resource
    /// with a BaseVectorResource in the new Node.
    pub fn replace_with_vector_resource_node(
        &mut self,
        id: u64,
        new_resource: BaseVectorResource,
        new_metadata: Option<HashMap<String, String>>,
        embedding: &Embedding,
    ) -> Result<Node, VRError> {
        let tag_names = new_resource.as_trait_object().data_tag_index().data_tag_names();
        self._replace_node_without_tag_validation(
            id,
            NodeContent::Resource(new_resource),
            new_metadata,
            &embedding,
            &tag_names,
        )
    }

    /// Replaces an existing node & associated embedding.
    /// * `id` - The id of the node to be replaced.
    pub fn replace_with_text_node(
        &mut self,
        id: u64,
        new_data: &str,
        new_metadata: Option<HashMap<String, String>>,
        embedding: &Embedding,
        parsing_tags: &Vec<DataTag>, // list of datatags you want to parse the new data with
    ) -> Result<Node, VRError> {
        // Validate which tags will be saved with the new data
        let validated_data_tags = DataTag::validate_tag_list(new_data, parsing_tags);
        let data_tag_names = validated_data_tags.iter().map(|tag| tag.name.clone()).collect();
        self._replace_node_without_tag_validation(
            id,
            NodeContent::Text(new_data.to_string()),
            new_metadata,
            embedding,
            &data_tag_names,
        )
    }

    /// Replaces an existing node & associated embedding with a new ExternalContent node.
    pub fn replace_with_external_content_node(
        &mut self,
        id: u64,
        new_external_content: SourceReference,
        new_metadata: Option<HashMap<String, String>>,
        embedding: &Embedding,
    ) -> Result<Node, VRError> {
        // As ExternalContent doesn't have data tags, we pass an empty vector
        self._replace_node_without_tag_validation(
            id,
            NodeContent::ExternalContent(new_external_content),
            new_metadata,
            embedding,
            &Vec::new(),
        )
    }

    /// Replaces an existing node & associated embedding with a new VRHeader node.
    pub fn replace_with_vr_header_node(
        &mut self,
        id: u64,
        new_vr_header: VRHeader,
        new_metadata: Option<HashMap<String, String>>,
        embedding: &Embedding,
    ) -> Result<Node, VRError> {
        let data_tag_names = new_vr_header.data_tag_names.clone();
        self._replace_node_without_tag_validation(
            id,
            NodeContent::VRHeader(new_vr_header),
            new_metadata,
            embedding,
            &data_tag_names,
        )
    }

    /// Replaces an existing node & associated embedding in the Document resource
    /// without checking if tags are valid.
    pub fn _replace_node_without_tag_validation(
        &mut self,
        id: u64,
        new_data: NodeContent,
        new_metadata: Option<HashMap<String, String>>,
        embedding: &Embedding,
        new_tag_names: &Vec<String>,
    ) -> Result<Node, VRError> {
        // Id + index
        if id > self.node_count {
            return Err(VRError::InvalidNodeId);
        }
        let index = (id - 1) as usize;

        // Next create the new node, and replace the old node in the nodes list
        let new_node =
            Node::from_node_content_with_integer_id(id, new_data.clone(), new_metadata.clone(), new_tag_names.clone());
        let old_node = std::mem::replace(&mut self.nodes[index], new_node.clone());

        // Then deletion of old node from indexes and addition of new node
        if old_node.data_tag_names != new_node.data_tag_names {
            self.data_tag_index.remove_node(&old_node);
            self.data_tag_index.add_node(&new_node);
        }
        if old_node.metadata_keys() != new_node.metadata_keys() {
            self.metadata_index.remove_node(&old_node);
            self.metadata_index.add_node(&new_node);
        }

        // Finally replacing the embedding
        let mut embedding = embedding.clone();
        embedding.set_id_with_integer(id);
        self.embeddings[index] = embedding;

        self.update_last_modified_to_now();

        Ok(old_node)
    }

    /// Pops and returns the last node and associated embedding.
    pub fn pop_node(&mut self) -> Result<(Node, Embedding), VRError> {
        let popped_node = self.nodes.pop();
        let popped_embedding = self.embeddings.pop();

        match (popped_node, popped_embedding) {
            (Some(node), Some(embedding)) => {
                // Remove node from indexes
                self.metadata_index.remove_node(&node);
                self.data_tag_index.remove_node(&node);
                self.node_count -= 1;
                self.update_last_modified_to_now();
                Ok((node, embedding))
            }
            _ => Err(VRError::VectorResourceEmpty),
        }
    }

    /// Deletes a node and associated embedding from the resource.
    pub fn remove_node(&mut self, id: u64) -> Result<(Node, Embedding), VRError> {
        let deleted_node = self._remove_node(id)?;
        self.data_tag_index.remove_node(&deleted_node);
        self.metadata_index.remove_node(&deleted_node);

        let index = (id - 1) as usize;
        let deleted_embedding = self.embeddings.remove(index);

        // Adjust the ids of the remaining embeddings
        for i in index..self.embeddings.len() {
            self.embeddings[i].set_id_with_integer((i + 1) as u64);
        }

        Ok((deleted_node, deleted_embedding))
    }

    /// Internal node deletion
    fn _remove_node(&mut self, id: u64) -> Result<Node, VRError> {
        if id > self.node_count {
            return Err(VRError::InvalidNodeId);
        }
        let index = (id - 1) as usize;
        let removed_node = self.nodes.remove(index);
        self.node_count -= 1;
        for node in self.nodes.iter_mut().skip(index) {
            let node_id: u64 = node.id.parse().unwrap();
            node.id = format!("{}", node_id - 1);
        }
        self.update_last_modified_to_now();
        Ok(removed_node)
    }

    /// Internal node appending
    fn _append_node(&mut self, mut node: Node) {
        self.node_count += 1;
        node.id = self.node_count.to_string();
        self.update_last_modified_to_now();
        self.nodes.push(node);
    }

    pub fn from_json(json: &str) -> Result<Self, VRError> {
        Ok(serde_json::from_str(json)?)
    }

    pub fn set_resource_id(&mut self, resource_id: String) {
        self.resource_id = resource_id;
        self.update_last_modified_to_now();
    }
}
