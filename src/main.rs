use clap::Parser;

use crate::Command::Init;
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
    },
}

#[derive(Debug, thiserror::Error)]
enum CliError {
    #[error("Unsupported subcommand")]
    UnsupportedSubcommand,
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

    match cli.command {
        Init { target } => {
            todo!()
        }
        _ => Err(CliError::UnsupportedSubcommand),
    }
}
