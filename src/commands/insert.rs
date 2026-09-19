use crate::{
    commands::{CliError, Ctx},
    git::{current_ref, get_current_branch},
    metadata::get_stack_by_name,
};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// The name of the stack to insert the new layer into.
    pub name: String,

    /// The name of the branch to insert into the stack
    pub branch: Option<String>,

    /// The position at which to insert the new layer. If omitted, the new layer
    /// will be added to the top of the stack.
    #[clap(long, conflicts_with_all = &["before", "after"])]
    pub at: Option<usize>,

    /// The name of the layer that this layer should be inserted before.
    /// Mutually exclusive with `at` and `after`.
    #[clap(long, conflicts_with_all = &["at", "after"])]
    pub before: Option<String>,

    /// The name of the layer that this layer should be inserted after.
    /// Mutually exclusive with `at` and `before`.
    #[clap(long, conflicts_with_all = &["at", "before"])]
    pub after: Option<String>,

    /// The ref from which to create the new layer. If omitted, the current HEAD will be used.
    #[clap(long)]
    pub from: Option<String>,
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<(), CliError> {
    let mut target_stack = get_stack_by_name(&ctx.git_folder, &args.name)
        .ok_or_else(|| CliError::StackNotFound(args.name.clone()))?;

    let from = if let Some(from) = args.from {
        from
    } else {
        current_ref(&ctx.cwd).await?
    };

    let branch = if let Some(branch) = args.branch {
        branch
    } else {
        get_current_branch(&ctx.cwd)
            .await?
            .ok_or(CliError::NotOnBranch)?
    };

    // Find the position to insert the new layer
    let position = if let Some(at) = args.at {
        Some(at)
    } else if let Some(before) = args.before {
        let position = target_stack.get_position(&before);
        if let Some(pos) = position {
            Some(pos)
        } else {
            return Err(CliError::LayerNotFound(before));
        }
    } else if let Some(after) = args.after {
        let position = target_stack.get_position(&after);
        if let Some(pos) = position {
            Some(pos + 1)
        } else {
            return Err(CliError::LayerNotFound(after));
        }
    } else {
        None
    };

    match position {
        Some(pos) => target_stack.add_layer_at(&branch, &from, pos)?,
        None => target_stack.add_layer(&branch, &from)?,
    }
    target_stack.save()?;
    println!("Layer '{}' inserted into stack '{}'", &branch, args.name);

    Ok(())
}
