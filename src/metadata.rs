use std::{
    fs::File,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const STACK_METADATA_VERSION: i32 = 1;
const STACK_METADATA_PATH: &str = "stack";

/// Represents the metadata associated with a stack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackMetadata {
    /// The version of the stack metadata
    pub version: i32,
    /// The target ref for this stack
    pub target: String,

    /// The layers that make up the stack
    pub layers: Vec<LayerMetadata>,
}

/// Represents the metadata associated with a layer in the stack
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LayerMetadata {
    /// The branch name for this layer
    pub branch: String,
    /// The base object ID for this layer
    pub base_oid: String,
}

impl TryFrom<PathBuf> for StackMetadata {
    type Error = std::io::Error;

    fn try_from(path: PathBuf) -> Result<Self, Self::Error> {
        let file = File::open(path)?;
        let metadata = serde_json::from_reader(file)?;
        Ok(metadata)
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AddLayerError {
    #[error("Layer already exists")]
    LayerAlreadyExists,
}

impl StackMetadata {
    /// Creates a new stack metadata with the specified target
    pub fn new(target: &str) -> Self {
        Self {
            version: STACK_METADATA_VERSION,
            target: target.to_string(),
            layers: Vec::new(),
        }
    }

    /// Writes the stack metadata to the specified file path
    pub fn write(&self, path: PathBuf) -> Result<(), std::io::Error> {
        let file = File::create(path)?;
        serde_json::to_writer(file, self)?;
        Ok(())
    }

    /// Adds a new layer to the stack
    pub fn add_layer(&mut self, branch: &str, base: &str) -> Result<(), AddLayerError> {
        // Check if this brannch is already included
        if self.layers.iter().any(|layer| layer.branch == branch) {
            return Err(AddLayerError::LayerAlreadyExists);
        }

        self.layers.push(LayerMetadata {
            branch: branch.to_string(),
            base_oid: base.to_string(),
        });

        Ok(())
    }
}

pub fn get_stack_metadata_path(repo: &Path, filename: &str) -> Result<PathBuf, std::io::Error> {
    let directory = repo.join(STACK_METADATA_PATH);
    // Ensure the stack metadata directory exists
    std::fs::create_dir_all(&directory)?;
    Ok(directory.join(filename))
}
