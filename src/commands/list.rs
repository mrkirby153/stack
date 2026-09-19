use crate::commands::{CliError, Ctx};

pub async fn run(ctx: &Ctx) -> Result<(), CliError> {
    let stack_folder = ctx.stack_folder()?;
    let stacks = if !stack_folder.exists() || !stack_folder.is_dir() {
        vec![]
    } else {
        stack_folder
            .read_dir()?
            .collect::<Result<Vec<_>, std::io::Error>>()?
    };
    if stacks.is_empty() {
        println!("No stacks available");
    } else {
        println!("Available stacks:");
        for stack in stacks {
            if let Some(name) = stack.file_name().to_str() {
                // Trim .json
                let name = name.strip_suffix(".json").unwrap_or(name);
                println!(" - {}", name);
            }
        }
    }
    Ok(())
}
