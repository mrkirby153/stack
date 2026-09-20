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

    for (i, layer) in current_stack.layers.iter().enumerate() {
        let marker = if Some(i) == position { "*" } else { " " };
        println!("{} {}", marker, layer.branch);
    }

    Ok(())
}
