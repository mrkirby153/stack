use crate::{
    commands::{CliError, Ctx},
    git::{get_current_branch, merge_base, resolve_ref},
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

    /// The base of the new layer (the commit it was forked from). If omitted,
    /// it is inferred as the merge-base of the branch and the layer directly
    /// below the insertion point (or the stack target at the bottom).
    /// Whatever is given is resolved to a SHA before being stored.
    #[clap(long)]
    pub from: Option<String>,
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<(), CliError> {
    let mut target_stack = get_stack_by_name(&ctx.git_folder, &args.name)
        .ok_or_else(|| CliError::StackNotFound(args.name.clone()))?;

    let branch = if let Some(branch) = args.branch {
        branch
    } else {
        get_current_branch(&ctx.cwd)
            .await?
            .ok_or(CliError::NotOnBranch)?
    };

    // Find the position to insert the new layer (`None` = top of the stack).
    let position = if let Some(at) = args.at {
        Some(at)
    } else if let Some(before) = args.before {
        Some(
            target_stack
                .get_position(&before)
                .ok_or(CliError::LayerNotFound(before))?,
        )
    } else if let Some(after) = args.after {
        Some(
            target_stack
                .get_position(&after)
                .map(|pos| pos + 1)
                .ok_or(CliError::LayerNotFound(after))?,
        )
    } else {
        None
    };

    // Determine the layer's base. The base defines the layer's replay range
    // (`base..tip`), so it must be the commit the branch was forked from —
    // never the branch tip itself.
    let base = if let Some(from) = &args.from {
        // Explicit: resolve to a SHA so the metadata doesn't depend on where
        // the ref points in the future.
        resolve_ref(&ctx.cwd, from).await?
    } else {
        // Inferred: merge-base with the layer directly below the insertion
        // point (the stack target at the bottom). This is correct both when
        // the branch was forked from the target and when it was forked from
        // the layer below.
        let pos = position.unwrap_or(target_stack.size());
        let (candidate, candidate_desc) = if pos == 0 {
            (
                target_stack.target.clone(),
                String::from("the stack target"),
            )
        } else {
            let below = target_stack
                .layers
                .get(pos.saturating_sub(1))
                .or(target_stack.layers.last());
            match below {
                Some(layer) => (layer.branch.clone(), format!("'{}'", layer.branch)),
                None => (
                    target_stack.target.clone(),
                    String::from("the stack target"),
                ),
            }
        };
        merge_base(&ctx.cwd, &branch, &candidate)
            .await?
            .ok_or_else(|| CliError::BaseUndeterminable {
                branch: branch.clone(),
                candidate: candidate_desc,
            })?
    };

    match position {
        Some(pos) => target_stack.add_layer_at(&branch, &base, pos)?,
        None => target_stack.add_layer(&branch, &base)?,
    }
    target_stack.save()?;
    println!(
        "Layer '{}' inserted into stack '{}' (base {})",
        &branch,
        args.name,
        &base[..7]
    );

    Ok(())
}
