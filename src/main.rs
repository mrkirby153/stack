use std::{env, fs::create_dir_all};

use clap::Parser;

use crate::Command::Init;
use stack::{
    git::{DEFAULT_BRANCH_TARGET, get_current_branch, get_repo_git_folder},
    metadata::{StackMetadata, get_stack_metadata_path},
};
/// A tool for managing git stacks
#[derive(Debug, clap::Parser)]
struct StackCli {
    #[clap(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// View the current status of a stack
    Status,

    /// Moves one layer up in the stack
    Up,
    /// Moves one layer down in the stack
    Down,
    /// Moves to the top of the stack
    Top,
    /// Moves to the bottom of the stack
    Bottom,

    /// Restacks the layers of the stack
    Restack,
    /// Advances the stack by one layer
    Advance,
    /// Inserts a new layer into the stack at a specified position
    Insert,
    /// Removes a layer from the stack
    Remove,
    /// Initializes a new stack using the current branch as the base
    Init {
        /// The target branch for the stack. Defaults to `main`
        target: Option<String>,
        /// The name of the new stack. Defaults to the current branch name.
        name: Option<String>,
    },
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("Unsupported subcommand")]
    UnsupportedSubcommand,
    #[error("Target branch matches the current branch")]
    TargetBranchMatchesCurrent,
    #[error("Stack already exists: {0}")]
    StackExists(String),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Git repository error: {0}")]
    GitRepoError(#[from] stack::git::GitRepoError),
}

#[tokio::main]
async fn main() -> std::process::ExitCode {
    match run().await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("Error: {err}");
            std::process::ExitCode::FAILURE
        }
    }
}

async fn run() -> Result<(), CliError> {
    let cli = StackCli::parse();
    let cwd = env::current_dir()?;
    let git_folder = get_repo_git_folder(&cwd).await?;

    match cli.command {
        Init { name, target } => {
            let target = target.unwrap_or(DEFAULT_BRANCH_TARGET.to_string());
            let current_branch = get_current_branch(&cwd).await?;

            if current_branch == target {
                return Err(CliError::TargetBranchMatchesCurrent);
            }
            let name = name.unwrap_or(current_branch.clone());
            let stack_metadata_folder = get_stack_metadata_path(&git_folder, "stacks")?;
            create_dir_all(&stack_metadata_folder)?;
            let stack_metadata_file = stack_metadata_folder.join(format!("{}.json", name));

            if stack_metadata_file.exists() {
                return Err(CliError::StackExists(name));
            }

            let metadata = StackMetadata::new(&target);
            metadata.write(stack_metadata_file)?;

            println!("Initialized new stack with the target branch: {}", target);
            Ok(())
        }
        _ => Err(CliError::UnsupportedSubcommand),
    }
}
