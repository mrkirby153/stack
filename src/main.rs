use std::{env, fs::create_dir_all};

use clap::Parser;

use stack::{
    git::{DEFAULT_BRANCH_TARGET, current_ref, get_current_branch, get_repo_git_folder},
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
        #[clap(long)]
        target: Option<String>,
        /// The name of the new stack. Defaults to the current branch name.
        #[clap(long)]
        name: Option<String>,
        /// Overwrite the stack if it already exists
        #[clap(long, default_value_t = false)]
        force: bool,

        /// The git reference for the base of the new stack. Defaults to the current HEAD.
        #[clap(long)]
        base: Option<String>,
    },
    /// Deletes an existing stack
    Delete {
        /// The name of the stack to delete
        name: String,
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
    #[error("Stack not found: {0}")]
    StackNotFound(String),

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
        Command::Init {
            name,
            target,
            force,
            base,
        } => {
            let target = target.unwrap_or(DEFAULT_BRANCH_TARGET.to_string());
            let current_branch = get_current_branch(&cwd).await?;

            if current_branch == target {
                return Err(CliError::TargetBranchMatchesCurrent);
            }
            let name = name.unwrap_or(current_branch.clone());
            let stack_metadata_folder = get_stack_metadata_path(&git_folder, "stacks")?;
            create_dir_all(&stack_metadata_folder)?;
            let stack_metadata_file = stack_metadata_folder.join(format!("{}.json", name));

            if stack_metadata_file.exists() && !force {
                return Err(CliError::StackExists(name));
            }

            let mut metadata = StackMetadata::new(&target);
            let base = base.unwrap_or(current_ref(&cwd).await?);
            metadata.add_layer(&current_branch, &base).unwrap();

            metadata.write(stack_metadata_file)?;

            println!("Initialized new stack with the target branch: {}", target);
            Ok(())
        }
        Command::Delete { name } => {
            let stack_metadata_folder = get_stack_metadata_path(&git_folder, "stacks")?;
            let stack_metadata_file = stack_metadata_folder.join(format!("{}.json", name));
            if stack_metadata_file.exists() {
                std::fs::remove_file(stack_metadata_file)?;
                println!("Deleted stack: {}", name);
                Ok(())
            } else {
                Err(CliError::StackNotFound(name))
            }
        }
        _ => Err(CliError::UnsupportedSubcommand),
    }
}
