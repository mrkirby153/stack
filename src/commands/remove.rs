use crate::{
    commands::{CliError, Ctx},
    metadata::get_stack_by_name,
};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Name of the stack to manipulate
    pub name: String,

    /// Name of the branch to remove from the stack
    pub branch: String,
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<(), CliError> {
    let mut current_stack = get_stack_by_name(&ctx.git_folder, &args.name)
        .ok_or(CliError::StackNotFound(args.name.clone()))?;
    if current_stack.remove_layer(&args.branch).is_none() {
        return Err(CliError::LayerNotFound(args.branch.clone()));
    }
    current_stack.save()?;
    println!(
        "Removed branch '{}' from stack '{}'",
        args.branch, args.name
    );
    Ok(())
}
