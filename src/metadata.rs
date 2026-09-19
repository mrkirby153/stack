use std::{fs::File, path::PathBuf};

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

impl StackMetadata {
    /// Creates a new stack metadata with the specified target
    pub fn new(target: String) -> Self {
        Self {
            version: 1,
            target,
            layers: Vec::new(),
        }
    }

    /// Writes the stack metadata to the specified file path
    pub fn write(&self, path: PathBuf) -> Result<(), std::io::Error> {
        let file = File::create(path)?;
        serde_json::to_writer(file, self)?;
        Ok(())
    }
}
