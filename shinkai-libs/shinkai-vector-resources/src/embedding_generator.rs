use crate::embeddings::Embedding;
use crate::model_type::{EmbeddingModelType, LocalModel, RemoteModel};
use crate::resource_errors::VectorResourceError;
use byteorder::{LittleEndian, ReadBytesExt};
use lazy_static::lazy_static;
#[cfg(feature = "native-http")]
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use std::io::prelude::*;
use std::io::Cursor;
use std::net::TcpStream;
// use llm::load_progress_callback_stdout as load_callback;
// use llm::Model;
// use llm::ModelArchitecture;

lazy_static! {
    pub static ref DEFAULT_LOCAL_EMBEDDINGS_PORT: &'static str = "7999";
}
const N_EMBD: usize = 384;

/// A trait for types that can generate embeddings from text.
pub trait EmbeddingGenerator {
    fn model_type(&self) -> EmbeddingModelType;

    /// Generates an embedding from the given input string, and assigns the
    /// provided id.
    fn generate_embedding(&self, input_string: &str, id: &str) -> Result<Embedding, VectorResourceError>;

    /// Generate an Embedding for an input string, sets id to a default value
    /// of empty string.
    fn generate_embedding_default(&self, input_string: &str) -> Result<Embedding, VectorResourceError> {
        self.generate_embedding(input_string, "")
    }

    /// Generates embeddings from the given list of input strings and ids.
    fn generate_embeddings(&self, input_strings: &[&str], ids: &[&str]) -> Result<Vec<Embedding>, VectorResourceError> {
        input_strings
            .iter()
            .zip(ids)
            .map(|(input, id)| self.generate_embedding(input, id))
            .collect()
    }

    /// Generate Embeddings for a list of input strings, sets ids to default
    fn generate_embeddings_default(&self, input_strings: &[&str]) -> Result<Vec<Embedding>, VectorResourceError> {
        input_strings
            .iter()
            .map(|input| self.generate_embedding_default(input))
            .collect()
    }
}

#[derive(Serialize)]
struct EmbeddingRequestBody {
    input: String,
    model: String,
}

#[derive(Deserialize)]
struct EmbeddingResponseData {
    embedding: Vec<f32>,
    index: usize,
    object: String,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    object: String,
    model: String,
    data: Vec<EmbeddingResponseData>,
    usage: serde_json::Value, // or define a separate struct for this if you need to use these values
}

#[derive(Debug, Clone, PartialEq)]
#[cfg(feature = "native-http")]
pub struct RemoteEmbeddingGenerator {
    model_type: EmbeddingModelType,
    api_url: String,
    api_key: Option<String>,
}

#[cfg(feature = "native-http")]
impl EmbeddingGenerator for RemoteEmbeddingGenerator {
    /// Generate an Embedding for an input string by using the external API.
    fn generate_embedding(&self, input_string: &str, id: &str) -> Result<Embedding, VectorResourceError> {
        // If we're using a Bert model with a Bert-CPP server
        if self.model_type == EmbeddingModelType::RemoteModel(RemoteModel::AllMiniLML12v2)
            || self.model_type == EmbeddingModelType::RemoteModel(RemoteModel::AllMiniLML6v2)
            || self.model_type == EmbeddingModelType::RemoteModel(RemoteModel::MultiQAMiniLML6)
        {
            let vector = self.generate_embedding_bert_cpp(input_string)?;
            return Ok(Embedding {
                vector,
                id: id.to_string(),
            });
        }
        // Else we're using OpenAI API
        else {
            return self.generate_embedding_open_ai(input_string, id);
        }
    }

    fn model_type(&self) -> EmbeddingModelType {
        self.model_type.clone()
    }
}

#[cfg(feature = "native-http")]
impl RemoteEmbeddingGenerator {
    /// Create a RemoteEmbeddingGenerator
    pub fn new(model_type: EmbeddingModelType, api_url: &str, api_key: Option<&str>) -> RemoteEmbeddingGenerator {
        RemoteEmbeddingGenerator {
            model_type,
            api_url: api_url.to_string(),
            api_key: api_key.map(|a| a.to_string()),
        }
    }

    /// Create a RemoteEmbeddingGenerator that automatically attempts to connect
    /// to the webserver of a local running instance of BertCPP using the
    /// default set port.
    ///
    /// Expected to have downloaded & be using the MultiQAMiniLML6 model.
    pub fn new_default() -> RemoteEmbeddingGenerator {
        let model_architecture = EmbeddingModelType::RemoteModel(RemoteModel::MultiQAMiniLML6);
        let url = format!("localhost:{}", DEFAULT_LOCAL_EMBEDDINGS_PORT.to_string());
        RemoteEmbeddingGenerator {
            model_type: model_architecture,
            api_url: url,
            api_key: None,
        }
    }

    /// This function takes a string and a TcpStream and sends the string to the Bert-CPP server
    fn bert_cpp_embeddings_fetch(input_text: &str, server: &mut TcpStream) -> Result<Vec<f32>, VectorResourceError> {
        // Send the input text to the server
        server.write_all(input_text.as_bytes()).map_err(|_| {
            VectorResourceError::FailedEmbeddingGeneration("Failed writing input text to TcpStream".to_string())
        })?;

        // Receive the data from the server
        let mut data = vec![0u8; N_EMBD * 4];
        server.read_exact(&mut data).map_err(|_| {
            VectorResourceError::FailedEmbeddingGeneration(
                "Failed reading embedding generation response from TcpStream".to_string(),
            )
        })?;

        // Convert the data into a vector of floats
        let mut rdr = Cursor::new(data);
        let mut embeddings = Vec::new();

        while let Ok(x) = rdr.read_f32::<LittleEndian>() {
            embeddings.push(x);
        }

        Ok(embeddings)
    }

    /// Generates embeddings for a given text using a local BERT C++ server.
    /// Of note, requires using TcpStream as the server has an arbitrary
    /// implementation that is not proper HTTP.
    fn generate_embedding_bert_cpp(&self, input_text: &str) -> Result<Vec<f32>, VectorResourceError> {
        let mut server_connection = TcpStream::connect(self.api_url.clone()).map_err(|_| {
            VectorResourceError::FailedEmbeddingGeneration("Failed connecting to TcpStream".to_string())
        })?;
        let mut buffer = [0; 4];
        server_connection.read_exact(&mut buffer).map_err(|_| {
            VectorResourceError::FailedEmbeddingGeneration("Failed reading initial buffer from TcpStream".to_string())
        })?;

        let embedding = Self::bert_cpp_embeddings_fetch(&input_text, &mut server_connection);
        match embedding {
            Ok(embed) => Ok(embed),
            Err(e) => Err(e),
        }
    }

    // TODO: Add authorization logic/flesh out to fully working
    /// Generate an Embedding for an input string by using the external OpenAI-matching API.
    #[cfg(feature = "native-http")]
    fn generate_embedding_open_ai(&self, input_string: &str, id: &str) -> Result<Embedding, VectorResourceError> {
        // Prepare the request body
        let request_body = EmbeddingRequestBody {
            input: String::from(input_string),
            model: self.model_type().to_string(),
        };

        // Create the HTTP client
        let client = Client::new();

        // Build the request
        let request = client
            .post(&format!("{}/v1/embeddings", self.api_url))
            .header("Content-Type", "application/json")
            .json(&request_body);

        // Send the request and check for errors
        let response = request.send().map_err(|err| {
            // Handle any HTTP client errors here (e.g., request creation failure)
            VectorResourceError::RequestFailed(format!("HTTP request failed: {}", err))
        })?;

        println!("Received response");

        // Check if the response is successful
        if response.status().is_success() {
            println!("Is successful");
            // Deserialize the response JSON into a struct (assuming you have an
            // EmbeddingResponse struct)
            let embedding_response: EmbeddingResponse = response.json().map_err(|err| {
                VectorResourceError::RequestFailed(format!("Failed to deserialize response JSON: {}", err))
            })?;

            // Use the response to create an Embedding instance
            Ok(Embedding {
                id: String::from(id),
                vector: embedding_response.data[0].embedding.clone(),
            })
        } else {
            // Handle non-successful HTTP responses (e.g., server error)
            Err(VectorResourceError::RequestFailed(format!(
                "HTTP request failed with status: {}",
                response.status()
            )))
        }
    }
}

// /// An Embedding Generator for Local LLMs, such as LLama, Bloom, Pythia, etc.
// pub struct LocalEmbeddingGenerator {
//     model: Box<dyn Model>,
//     model_type: EmbeddingModelType,
// }

// impl EmbeddingGenerator for LocalEmbeddingGenerator {
//     /// Generate an Embedding for an input string.
//     /// - `id`: The id to be associated with the embeddings.
//     fn generate_embedding(&self, input_string: &str, id: &str) -> Result<Embedding, VectorResourceError> {
//         let mut session = self.model.start_session(Default::default());
//         let mut output_request = llm::OutputRequest {
//             all_logits: None,
//             embeddings: Some(Vec::new()),
//         };
//         let vocab = self.model.tokenizer();
//         let beginning_of_sentence = true;

//         let tokens = vocab
//             .tokenize(input_string, beginning_of_sentence)
//             .map_err(|_| VectorResourceError::FailedEmbeddingGeneration)?;

//         let query_token_ids = tokens.iter().map(|(_, tok)| *tok).collect::<Vec<_>>();

//         self.model.evaluate(&mut session, &query_token_ids, &mut output_request);

//         let vector = output_request
//             .embeddings
//             .ok_or_else(|| VectorResourceError::FailedEmbeddingGeneration)?;

//         Ok(Embedding {
//             id: String::from(id),
//             vector,
//         })
//     }

//     fn model_type(&self) -> EmbeddingModelType {
//         self.model_type.clone()
//     }
// }

// impl LocalEmbeddingGenerator {
//     /// Create a new LocalEmbeddingGenerator with a specified model.
//     pub fn new(model: Box<dyn Model>, model_architecture: ModelArchitecture) -> Self {
//         Self {
//             model,
//             model_type: EmbeddingModelType::LocalModel(LocalModel::from_model_architecture(model_architecture)),
//         }
//     }

//     /// Create a new LocalEmbeddingGenerator that uses the default model.
//     /// Intended to be used just for testing.
//     pub fn new_default() -> Self {

//         let DEFAULT_LOCAL_MODEL_PATH: &'static str = "models/pythia-160m-q4_0.bin";
//         let model_architecture = llm::ModelArchitecture::GptNeoX;
//         let model = llm::load_dynamic(
//             Some(model_architecture),
//             std::path::Path::new(&*DEFAULT_LOCAL_MODEL_PATH),
//             llm::TokenizerSource::Embedded,
//             Default::default(),
//             load_callback,
//         )
//         .unwrap_or_else(|err| panic!("Failed to load model: {}", err));
//         LocalEmbeddingGenerator::new(model, model_architecture)
//     }
// }
