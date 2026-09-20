use std::{env, path::PathBuf};

use crate::{
    git::{GitRepoError, get_current_branch, get_repo_git_folder},
    metadata::{
        AddLayerError, STACK_METADATA_PATH, Stack, get_stack_for_branch, get_stack_metadata_path,
    },
};

pub mod delete;
pub mod init;
pub mod insert;
pub mod list;
pub mod movement;
pub mod remove;
pub mod restack;
pub mod status;

#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("Not on any branch")]
    NotOnBranch,
    #[error("No stack found for the current branch")]
    NoStack,
    #[error("Unsupported subcommand")]
    UnsupportedSubcommand,
    #[error("Target branch matches the current branch")]
    TargetBranchMatchesCurrent,
    #[error("Stack already exists: {0}")]
    StackExists(String),
    #[error("Stack not found: {0}")]
    StackNotFound(String),
    #[error("Layer not found: {0}")]
    LayerNotFound(String),
    #[error(
        "Could not determine a base for layer '{branch}': it shares no common \
         ancestor with {candidate}. Pass --from <ref> to set the base explicitly."
    )]
    BaseUndeterminable { branch: String, candidate: String },

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Stack metadata error: {0}")]
    AddLayerError(#[from] AddLayerError),
    #[error("Git repository error: {0}")]
    GitRepoError(#[from] GitRepoError),
    #[error("{0}")]
    RestackError(#[from] crate::commands::restack::Error),
}

pub struct Ctx {
    pub cwd: PathBuf,
    pub git_folder: PathBuf,
}

impl Ctx {
    pub async fn new() -> Result<Self, CliError> {
        let cwd = env::current_dir()?;
        let git_folder = get_repo_git_folder(&cwd).await?;
        Ok(Self { cwd, git_folder })
    }

    pub fn stack_folder(&self) -> Result<PathBuf, CliError> {
        Ok(get_stack_metadata_path(&self.git_folder, "stacks")?)
    }

    pub async fn current_stack(&self) -> Result<Option<Stack>, CliError> {
        let current_branch = get_current_branch(&self.cwd)
            .await?
            .ok_or(CliError::NotOnBranch)?;
        let current_stack = get_stack_for_branch(&self.git_folder, &current_branch);
        Ok(current_stack)
    }

    pub async fn current_branch(&self) -> Result<Option<String>, CliError> {
        let branch = get_current_branch(&self.cwd).await?;
        Ok(branch)
    }

    pub async fn stack_datadir(&self) -> Result<PathBuf, CliError> {
        let directory = self.git_folder.join(STACK_METADATA_PATH);
        // Ensure the stack metadata directory exists
        std::fs::create_dir_all(&directory)?;
        Ok(directory)
    }
}
