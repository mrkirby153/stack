use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use std::fs::File;

use crate::{
    commands::{
        CliError::{self, StackNotFound},
        Ctx,
    },
    git::{current_ref_for_branch, update_refs_atomic},
    metadata::{Stack, StackMetadata, get_stack_by_name},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to write undo snapshot")]
    FailedToWriteUndoSnapshot,
    #[error("Undo snapshot not found")]
    UndoSnapshotNotFound,
}

pub async fn advance(ctx: &Ctx) -> Result<(), CliError> {
    // Advances the current stack forward by one layer.
    // Implementation: Drop the top of the current stack. Re-stack the remaining layers.
    todo!()
}

#[derive(Debug, clap::Args)]
pub struct RestackArgs {
    #[clap(long, action)]
    /// Continues the restack operation from where it left off
    continue_: bool,
    #[clap(long, action)]
    /// Undos the last restack operation
    undo: bool,
}

pub async fn restack(ctx: &Ctx, args: RestackArgs) -> Result<(), CliError> {
    // Re-stacks the current stack
    if args.continue_ {
        todo!("Continue the restack operation");
        // return Ok(());
    }
    if args.undo {
        let undo_snapshot = get_undo_snapshot(ctx).await;
        if let Some(undo_snapshot) = undo_snapshot {
            // Restore the previous stack state using the undo snapshot
            println!("Undoing the last restack operation");
            let mut stack = get_stack_by_name(&ctx.git_folder, &undo_snapshot.stack_name)
                .ok_or(StackNotFound(undo_snapshot.stack_name.clone()))?;

            // Update the refs
            let previous_refs: Vec<(&str, &str)> = undo_snapshot
                .previous_refs
                .iter()
                .map(|(branch, reference)| (branch.as_str(), reference.as_str()))
                .collect();
            println!("Previous refs to restore: {:?}", previous_refs);
            update_refs_atomic(&ctx.git_folder, &previous_refs).await?;
            stack.set_metadata(undo_snapshot.metadata);

            // Atomically update the stack metadata
            stack.save_atomic()?;
            clear_undo_snapshot(ctx).await?;
            println!("Successfully undone the last restack operation");
            return Ok(());
        } else {
            return Err(Error::UndoSnapshotNotFound)?;
        }
    }

    let current_stack = ctx.current_stack().await?.ok_or(CliError::NoStack)?;

    // Step 1: Ensure that the stack is valid (All base hashes point to commits in the repository)
    // Step 2: Starting from the bottom of the stack, re-apply each layer on top of the previous layer.
    // Step 3: Update the stack metadata

    let undo_snapshot = UndoSnapshot::new(&ctx.git_folder, current_stack).await?;
    let undo_snapshot_path = undo_snapshot_path(ctx).await?;
    let undo_file = File::create(&undo_snapshot_path)?;
    serde_json::to_writer(undo_file, &undo_snapshot)
        .map_err(|_| Error::FailedToWriteUndoSnapshot)?;

    todo!()
}

async fn undo_snapshot_path(ctx: &Ctx) -> Result<PathBuf, CliError> {
    Ok(ctx.stack_datadir().await?.join("undo_snapshot.json"))
}

async fn get_undo_snapshot(ctx: &Ctx) -> Option<UndoSnapshot> {
    let undo_file = File::open(&undo_snapshot_path(ctx).await.ok()?).ok()?;
    let undo_snapshot: UndoSnapshot = serde_json::from_reader(undo_file).ok()?;
    Some(undo_snapshot)
}

async fn clear_undo_snapshot(ctx: &Ctx) -> Result<(), std::io::Error> {
    let undo_snapshot_path = undo_snapshot_path(ctx).await;
    if let Ok(undo_snapshot_path) = undo_snapshot_path
        && undo_snapshot_path.exists()
    {
        std::fs::remove_file(undo_snapshot_path)?;
    }
    Ok(())
}

#[derive(Debug, Serialize, Deserialize)]
struct UndoSnapshot {
    /// The name of the stack
    stack_name: String,
    /// The previous references of the stack layers
    previous_refs: HashMap<String, String>,
    /// A snapshot of the stack metadata before the restack operation
    metadata: StackMetadata,
}

impl UndoSnapshot {
    pub async fn new(repo: &Path, stack: Stack) -> Result<Self, CliError> {
        let mut previous_refs = HashMap::new();
        for layer in &stack.layers {
            let current_ref = current_ref_for_branch(repo, &layer.branch).await?;
            previous_refs.insert(layer.branch.clone(), current_ref);
        }

        Ok(Self {
            stack_name: stack.name().to_string(),
            previous_refs,
            metadata: stack.metadata().clone(),
        })
    }
}
