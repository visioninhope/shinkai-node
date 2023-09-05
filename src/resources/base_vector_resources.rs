use super::document_resource::DocumentVectorResource;
use super::map_resource::MapVectorResource;
use super::vector_resource::VectorResource;
use crate::resources::data_tags::{DataTag, DataTagIndex};
use crate::resources::embeddings::*;
use crate::resources::resource_errors::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::str::FromStr;

/// The list of base/core VectorResource types which are fully
/// composable within one another
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum BaseVectorResource {
    Document(DocumentVectorResource),
    Map(MapVectorResource),
}

impl BaseVectorResource {
    /// Converts into a Box<&dyn VectorResource>.
    /// Used to access all of the VectorResource trait's methods, ie.
    /// self.as_trait_object().vector_search(...);
    ///
    /// Note this is not a mutable reference so this is only for reading.
    pub fn as_trait_object(&self) -> Box<&dyn VectorResource> {
        match self {
            BaseVectorResource::Document(resource) => Box::new(resource),
            BaseVectorResource::Map(resource) => Box::new(resource),
        }
    }

    /// Converts into a Box<&mut dyn VectorResource>, which provides ability
    /// to mutate the BaseVectorResource using the VectorResource trait's methods.
    pub fn as_trait_object_mut(&mut self) -> Box<&mut dyn VectorResource> {
        match self {
            BaseVectorResource::Document(resource) => Box::new(resource),
            BaseVectorResource::Map(resource) => Box::new(resource),
        }
    }

    /// Attempts to convert the BaseVectorResource into a DocumentVectorResource
    pub fn as_document_resource(&self) -> Result<&DocumentVectorResource, VectorResourceError> {
        match self {
            BaseVectorResource::Document(resource) => Ok(resource),
            _ => Err(VectorResourceError::InvalidVectorResourceBaseType),
        }
    }

    /// Attempts to convert the BaseVectorResource into a MapVectorResource
    pub fn as_map_resource(&self) -> Result<&MapVectorResource, VectorResourceError> {
        match self {
            BaseVectorResource::Map(resource) => Ok(resource),
            _ => Err(VectorResourceError::InvalidVectorResourceBaseType),
        }
    }

    /// Returns the base type of the VectorResource
    pub fn resource_base_type(&self) -> VectorResourceBaseType {
        self.as_trait_object().resource_base_type()
    }
}

impl From<DocumentVectorResource> for BaseVectorResource {
    fn from(resource: DocumentVectorResource) -> Self {
        BaseVectorResource::Document(resource)
    }
}

impl From<MapVectorResource> for BaseVectorResource {
    fn from(resource: MapVectorResource) -> Self {
        BaseVectorResource::Map(resource)
    }
}

/// Enum used for all VectorResources to self-attest their base type.
/// Used primarily when dealing with Trait objects, and self-attesting
/// JSON serialized VectorResources
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum VectorResourceBaseType {
    Document,
    Map,
}

impl VectorResourceBaseType {
    pub fn to_str(&self) -> &str {
        match self {
            VectorResourceBaseType::Document => "Document",
            VectorResourceBaseType::Map => "Map",
        }
    }
}

impl FromStr for VectorResourceBaseType {
    type Err = VectorResourceError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "Document" => Ok(VectorResourceBaseType::Document),
            "Map" => Ok(VectorResourceBaseType::Map),
            _ => Err(VectorResourceError::InvalidVectorResourceBaseType),
        }
    }
}
