use crate::{
    commands::{CliError, Ctx},
    git::checkout_branch,
};

pub async fn up(ctx: &Ctx) -> Result<(), CliError> {
    let current_stack = ctx.current_stack().await?.ok_or(CliError::NoStack)?;
    // Branch has been asserted by current_stack
    let current_branch = ctx.current_branch().await?.expect("not on a branch");

    let current_position = current_stack
        .get_position(&current_branch)
        .ok_or(CliError::NoStack)?;
    // `up` moves toward the top (furthest from the target).
    if current_position + 1 >= current_stack.size() {
        return Ok(());
    }
    let new_branch = current_stack
        .layers
        .get(current_position + 1)
        .ok_or(CliError::NoStack)?;
    checkout_branch(&ctx.cwd, &new_branch.branch).await?;
    Ok(())
}

pub async fn down(ctx: &Ctx) -> Result<(), CliError> {
    let current_stack = ctx.current_stack().await?.ok_or(CliError::NoStack)?;
    // Branch has been asserted by current_stack
    let current_branch = ctx.current_branch().await?.expect("not on a branch");

    let current_position = current_stack
        .get_position(&current_branch)
        .ok_or(CliError::NoStack)?;
    // `down` moves toward the bottom (closest to the target).
    if current_position == 0 {
        return Ok(());
    }
    let new_branch = current_stack
        .layers
        .get(current_position - 1)
        .ok_or(CliError::NoStack)?;
    checkout_branch(&ctx.cwd, &new_branch.branch).await?;
    Ok(())
}

pub async fn top(ctx: &Ctx) -> Result<(), CliError> {
    let current_stack = ctx.current_stack().await?.ok_or(CliError::NoStack)?;

    let top_branch = current_stack.layers.last().ok_or(CliError::NoStack)?;
    checkout_branch(&ctx.cwd, &top_branch.branch).await?;
    Ok(())
}

pub async fn bottom(ctx: &Ctx) -> Result<(), CliError> {
    let current_stack = ctx.current_stack().await?.ok_or(CliError::NoStack)?;
    let bottom_branch = current_stack.layers.first().ok_or(CliError::NoStack)?;
    checkout_branch(&ctx.cwd, &bottom_branch.branch).await?;
    Ok(())
}
