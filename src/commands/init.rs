use std::fs::create_dir_all;

use crate::{
    commands::{CliError, Ctx},
    git::{DEFAULT_BRANCH_TARGET, current_ref, get_current_branch},
    metadata::{StackMetadata, get_stack_metadata_path},
};

#[derive(Debug, clap::Args)]
pub struct Args {
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
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<(), CliError> {
    let target = args.target.unwrap_or(DEFAULT_BRANCH_TARGET.to_string());
    let current_branch = get_current_branch(&ctx.cwd).await?;

    if current_branch == target {
        return Err(CliError::TargetBranchMatchesCurrent);
    }
    let name = args.name.unwrap_or(current_branch.clone());
    let stack_metadata_folder = get_stack_metadata_path(&ctx.git_folder, "stacks")?;
    create_dir_all(&stack_metadata_folder)?;
    let stack_metadata_file = stack_metadata_folder.join(format!("{}.json", name));

    if stack_metadata_file.exists() && !args.force {
        return Err(CliError::StackExists(name));
    }

    let mut metadata = StackMetadata::new(&target);
    let base = args.base.unwrap_or(current_ref(&ctx.cwd).await?);
    metadata.add_layer(&current_branch, &base).unwrap();

    metadata.write(stack_metadata_file)?;

    println!("Initialized new stack with the target branch: {}", target);
    Ok(())
}
