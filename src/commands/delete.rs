use crate::{
    commands::{CliError, Ctx},
    metadata::get_stack_metadata_path,
};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// The name of the stack to delete
    name: String,
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<(), CliError> {
    let stack_metadata_folder = get_stack_metadata_path(&ctx.git_folder, "stacks")?;
    let stack_metadata_file = stack_metadata_folder.join(format!("{}.json", args.name));
    if stack_metadata_file.exists() {
        std::fs::remove_file(stack_metadata_file)?;
        println!("Deleted stack: {}", args.name);
        Ok(())
    } else {
        Err(CliError::StackNotFound(args.name))
    }
}
