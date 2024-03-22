use super::{BaseVectorResource, MapVectorResource, Node, NodeContent, VRKai, VRPath, VRSource};
use crate::{
    embeddings::Embedding,
    resource_errors::VRError,
    source::{DistributionOrigin, SourceFileMap},
};
use base64::decode;
use base64::encode;
use lz4_flex::{compress_prepend_size, decompress_size_prepended};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fmt;

// Versions of VRPack that are supported
#[derive(Debug, Serialize, Deserialize, PartialEq, Clone)]
pub enum VRPackVersion {
    #[serde(rename = "V1")]
    V1,
}

impl VRPackVersion {
    pub fn to_string(&self) -> String {
        serde_json::to_string(self).unwrap()
    }
}

/// Represents a parsed VRPack file, which contains a Map Vector Resource that holds a tree structure of folders & encoded VRKai nodes.
/// In other words, a `.vrpack` file is akin to a "compressed archive" of internally held VRKais with folder structure preserved.
/// Of note, VRPacks are not compressed at the top level because the VRKais held inside already are. This improves performance for large VRPacks.
/// To save as a file or transfer the VRPack, call one of the `encode_as_` methods. To parse from a file/transfer, use the `from_` methods.
#[derive(Debug, Serialize, Deserialize, Clone, PartialEq)]
pub struct VRPack {
    pub resource: BaseVectorResource,
    pub version: VRPackVersion,
}

impl VRPack {
    /// The default VRPack version which is used when creating new VRPacks
    pub fn default_vrpack_version() -> VRPackVersion {
        VRPackVersion::V1
    }

    /// Creates a new VRPack with the provided BaseVectorResource and the default version.
    pub fn new(resource: BaseVectorResource) -> Self {
        VRPack {
            resource,
            version: Self::default_vrpack_version(),
        }
    }

    /// Creates a new empty VRPack with an empty BaseVectorResource and the default version.
    pub fn new_empty() -> Self {
        VRPack {
            resource: BaseVectorResource::Map(MapVectorResource::new_empty("vrpack", None, VRSource::None, true)),
            version: Self::default_vrpack_version(),
        }
    }

    /// Prepares the VRPack to be saved or transferred as bytes.
    /// Of note, this is the bytes of the UTF-8 base64 string. This allows for easy compatibility between the two.
    pub fn encode_as_bytes(&self) -> Result<Vec<u8>, VRError> {
        if let VRPackVersion::V1 = self.version {
            let base64_encoded = self.encode_as_base64()?;
            return Ok(base64_encoded.into_bytes());
        }
        return Err(VRError::UnsupportedVRPackVersion(self.version.to_string()));
    }

    /// Prepares the VRPack to be saved or transferred across the network as a base64 encoded string.
    pub fn encode_as_base64(&self) -> Result<String, VRError> {
        if let VRPackVersion::V1 = self.version {
            let json_str = serde_json::to_string(self)?;
            let base64_encoded = encode(json_str.as_bytes());
            return Ok(base64_encoded);
        }
        return Err(VRError::UnsupportedVRPackVersion(self.version.to_string()));
    }

    /// Parses a VRPack from an array of bytes, assuming the bytes are a Base64 encoded string.
    pub fn from_bytes(base64_bytes: &[u8]) -> Result<Self, VRError> {
        // If it is Version V1
        if let Ok(base64_str) = String::from_utf8(base64_bytes.to_vec())
            .map_err(|e| VRError::VRPackParsingError(format!("UTF-8 conversion error: {}", e)))
        {
            return Self::from_base64(&base64_str);
        }

        return Err(VRError::UnsupportedVRPackVersion("".to_string()));
    }

    /// Parses a VRPack from a Base64 encoded string without compression.
    pub fn from_base64(base64_encoded: &str) -> Result<Self, VRError> {
        // If it is Version V1
        if let Ok(vrkai) = Self::from_base64_v1(base64_encoded) {
            return Ok(vrkai);
        }

        return Err(VRError::UnsupportedVRPackVersion("".to_string()));
    }

    /// Parses a VRPack from a Base64 encoded string using V1 logic without compression.
    fn from_base64_v1(base64_encoded: &str) -> Result<Self, VRError> {
        let bytes =
            decode(base64_encoded).map_err(|e| VRError::VRPackParsingError(format!("Base64 decoding error: {}", e)))?;
        let json_str = String::from_utf8(bytes)
            .map_err(|e| VRError::VRPackParsingError(format!("UTF-8 conversion error: {}", e)))?;
        let vrkai = serde_json::from_str(&json_str)
            .map_err(|e| VRError::VRPackParsingError(format!("JSON parsing error: {}", e)))?;
        Ok(vrkai)
    }

    /// Parses the VRPack into human-readable JSON (intended for readability in non-production use cases)
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    /// Parses into a VRPack from human-readable JSON (intended for readability in non-production use cases)
    pub fn from_json(json_str: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(json_str)
    }

    /// Adds a VRKai into the VRPack inside of the specified parent path (folder or root).
    pub fn insert_vrkai(&mut self, vrkai: &VRKai, parent_path: VRPath) -> Result<(), VRError> {
        let resource_name = vrkai.resource.as_trait_object().name().to_string();
        let embedding = vrkai.resource.as_trait_object().resource_embedding().clone();
        let metadata = None;
        let enc_vrkai = vrkai.encode_as_base64()?;
        let node = Node::new_text(resource_name.clone(), enc_vrkai, metadata, &vec![]);

        self.resource
            .as_trait_object_mut()
            .insert_node_at_path(parent_path, resource_name, node, embedding)?;

        Ok(())
    }

    /// Creates a folder inside the VRPack at the specified parent path.
    pub fn create_folder(&mut self, folder_name: &str, parent_path: VRPath) -> Result<(), VRError> {
        let resource = BaseVectorResource::Map(MapVectorResource::new_empty("vrpack", None, VRSource::None, true));
        let node = Node::new_vector_resource(folder_name.to_string(), &resource, None);
        let embedding = Embedding::new_empty();

        self.resource.as_trait_object_mut().insert_node_at_path(
            parent_path,
            folder_name.to_string(),
            node,
            embedding,
        )?;

        Ok(())
    }

    /// Parses a node into a VRKai.
    fn parse_node_to_vrkai(node: &Node) -> Result<VRKai, VRError> {
        match &node.content {
            NodeContent::Text(content) => {
                let vrkai_bytes = decode(content)
                    .map_err(|e| VRError::VRPackParsingError(format!("Base64 decoding error: {}", e)))?;
                let vrkai_str = String::from_utf8(vrkai_bytes)
                    .map_err(|e| VRError::VRPackParsingError(format!("UTF-8 conversion error: {}", e)))?;
                serde_json::from_str(&vrkai_str)
                    .map_err(|e| VRError::VRPackParsingError(format!("VRKai parsing error: {}", e)))
            }
            _ => Err(VRError::VRKaiParsingError("Invalid node content type".to_string())),
        }
    }

    /// Fetches the VRKai node at the specified path and parses it into a VRKai.
    pub fn get_vrkai(&self, path: VRPath) -> Result<VRKai, VRError> {
        let node = self.resource.as_trait_object().retrieve_node_at_path(path.clone())?;
        Self::parse_node_to_vrkai(&node.node)
    }

    /// Removes a node (VRKai or folder) from the VRPack at the specified path.
    pub fn remove_at_path(&mut self, path: VRPath) -> Result<(), VRError> {
        self.resource.as_trait_object_mut().remove_node_at_path(path)?;
        Ok(())
    }
}
