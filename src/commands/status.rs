use crate::{
    commands::{CliError, Ctx},
    git::get_current_branch,
};

pub async fn run(ctx: &Ctx) -> Result<(), CliError> {
    let current_stack = ctx.current_stack().await?.ok_or(CliError::NoStack)?;
    let current_branch = get_current_branch(&ctx.cwd)
        .await?
        .ok_or(CliError::NotOnBranch)?;
    println!("Current stack: {}", current_stack.name());

    println!();
    let position = current_stack.get_position(&current_branch);
    println!(
        "Position {} of {}",
        position.map(|p| p + 1).unwrap_or(0),
        current_stack.size()
    );

    Ok(())
}
