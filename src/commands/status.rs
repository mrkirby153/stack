use crate::{
    commands::{CliError, Ctx},
    git::get_current_branch,
    metadata::{StackMetadata, get_stack_for_branch},
};

pub async fn run(ctx: &Ctx) -> Result<(), CliError> {
    let current_branch = get_current_branch(&ctx.cwd)
        .await?
        .ok_or(CliError::NotOnBranch)?;
    let current_stack =
        get_stack_for_branch(&ctx.git_folder, &current_branch).ok_or(CliError::NoStack)?;

    println!("Current branch: {}", current_branch);
    println!(
        "Current stack: {}",
        current_stack
            .file_name()
            .and_then(|f| f.to_str())
            .and_then(|s| s.strip_suffix(".json"))
            .unwrap_or("<<unknown>>")
    );

    let metadata = StackMetadata::try_from(current_stack.as_path())?;

    println!();
    let position = metadata.get_position(&current_branch);
    println!(
        "Position {} of {}",
        position.map(|p| p + 1).unwrap_or(0),
        metadata.size()
    );

    Ok(())
}
