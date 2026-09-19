use clap::Parser;

use stack::commands::{self, CliError, Ctx};
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
    Insert(commands::insert::Args),
    /// Removes a layer from the stack
    Remove(commands::remove::Args),
    /// Initializes a new stack using the current branch as the base
    Init(commands::init::Args),
    /// Deletes an existing stack
    Delete(commands::delete::Args),
    /// Lists all stacks
    List,
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
    let ctx = Ctx::new().await?;

    match cli.command {
        Command::Init(args) => commands::init::run(&ctx, args).await,
        Command::Delete(args) => commands::delete::run(&ctx, args).await,
        Command::List => commands::list::run(&ctx).await,
        Command::Status => commands::status::run(&ctx).await,
        Command::Insert(args) => commands::insert::run(&ctx, args).await,
        Command::Remove(args) => commands::remove::run(&ctx, args).await,
        _ => Err(CliError::UnsupportedSubcommand),
    }
}
