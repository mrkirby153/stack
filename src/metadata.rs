use std::{
    fs::File,
    ops::{Deref, DerefMut},
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

const STACK_METADATA_VERSION: i32 = 1;
const STACK_METADATA_PATH: &str = "stack";

pub struct Stack {
    file: PathBuf,
    metadata: StackMetadata,
}

impl Deref for Stack {
    type Target = StackMetadata;

    fn deref(&self) -> &Self::Target {
        &self.metadata
    }
}

impl DerefMut for Stack {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.metadata
    }
}

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

impl TryFrom<&Path> for Stack {
    type Error = std::io::Error;

    fn try_from(path: &Path) -> Result<Self, Self::Error> {
        let file = File::open(path)?;
        let metadata = serde_json::from_reader(file)?;
        Ok(Stack {
            file: path.to_path_buf(),
            metadata,
        })
    }
}

impl Stack {
    pub fn new(file: &Path, target: &str) -> Self {
        Self {
            file: file.to_path_buf(),
            metadata: StackMetadata::new(target),
        }
    }

    pub fn save(&self) -> Result<(), std::io::Error> {
        self.metadata.write(&self.file)
    }

    pub fn name(&self) -> &str {
        self.file
            .file_name()
            .and_then(|f| f.to_str())
            .and_then(|s| s.strip_suffix(".json"))
            .unwrap_or("<<unknown>>")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AddLayerError {
    #[error("Layer already exists")]
    LayerAlreadyExists,
}

impl StackMetadata {
    /// Creates a new stack metadata with the specified target
    fn new(target: &str) -> Self {
        Self {
            version: STACK_METADATA_VERSION,
            target: target.to_string(),
            layers: Vec::new(),
        }
    }

    /// Writes the stack metadata to the specified file path
    pub fn write(&self, path: &Path) -> Result<(), std::io::Error> {
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

    pub fn add_layer_at(
        &mut self,
        branch: &str,
        base: &str,
        position: usize,
    ) -> Result<(), AddLayerError> {
        // Check if this branch is already included
        if self.layers.iter().any(|layer| layer.branch == branch) {
            return Err(AddLayerError::LayerAlreadyExists);
        }

        if position > self.layers.len() {
            self.layers.push(LayerMetadata {
                branch: branch.to_string(),
                base_oid: base.to_string(),
            });
        } else {
            self.layers.insert(
                position,
                LayerMetadata {
                    branch: branch.to_string(),
                    base_oid: base.to_string(),
                },
            );
        }

        Ok(())
    }

    pub fn remove_layer(&mut self, branch: &str) -> Option<LayerMetadata> {
        if let Some(pos) = self.get_position(branch) {
            Some(self.layers.remove(pos))
        } else {
            None
        }
    }

    pub fn get_position(&self, branch: &str) -> Option<usize> {
        self.layers.iter().position(|layer| layer.branch == branch)
    }

    pub fn size(&self) -> usize {
        self.layers.len()
    }
}

pub fn get_stack_metadata_path(repo: &Path, filename: &str) -> Result<PathBuf, std::io::Error> {
    let directory = repo.join(STACK_METADATA_PATH);
    // Ensure the stack metadata directory exists
    std::fs::create_dir_all(&directory)?;
    Ok(directory.join(filename))
}

pub fn get_stack_for_branch(repo: &Path, branch_name: &str) -> Option<Stack> {
    let stack_metadata_folder = repo.join(STACK_METADATA_PATH).join("stacks");

    if !stack_metadata_folder.exists() {
        return None;
    }

    let read_dir = stack_metadata_folder.read_dir();
    match read_dir {
        Ok(read_dir) => {
            for entry in read_dir.flatten() {
                let path = entry.path();
                if path.is_file()
                    && let Ok(stack) = Stack::try_from(path.as_path())
                    && stack
                        .metadata
                        .layers
                        .iter()
                        .any(|layer| layer.branch == branch_name)
                {
                    return Some(stack);
                }
            }
        }
        Err(_) => return None,
    }
    None
}

pub fn get_stack_by_name(git_folder: &Path, name: &str) -> Option<Stack> {
    let stack_metadata_folder = git_folder
        .join(STACK_METADATA_PATH)
        .join("stacks")
        .join(format!("{}.json", name));
    if !stack_metadata_folder.exists() {
        return None;
    }

    stack_metadata_folder.as_path().try_into().ok()
}
