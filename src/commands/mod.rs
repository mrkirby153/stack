use std::{env, path::PathBuf};

use crate::{
    git::{GitRepoError, get_repo_git_folder},
    metadata::get_stack_metadata_path,
};

pub mod delete;
pub mod init;
pub mod list;
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

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Git repository error: {0}")]
    GitRepoError(#[from] GitRepoError),
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
}
