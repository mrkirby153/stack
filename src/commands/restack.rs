use std::{
    collections::HashMap,
    fs::File,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::{
    commands::{CliError::{self, NoStack, StackNotFound}, Ctx},
    git::{checkout, current_ref_for_branch, git_status, has_unmerged_paths, update_refs_atomic},
    metadata::{Stack, StackMetadata, get_stack_by_name},
};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Failed to write undo snapshot")]
    FailedToWriteUndoSnapshot,
    #[error("Failed to write the in-progress operation state")]
    FailedToWriteState,
    #[error("Undo snapshot not found")]
    UndoSnapshotNotFound,
    #[error("No in-progress {op} operation to continue")]
    NoInFlightOperation { op: String },
    #[error(
        "A {op} is already in progress. Resume it with `stack {op} --continue`, \
         or restore the previous state with `stack {op} --undo`."
    )]
    OperationInFlight { op: String },
    #[error(
        "The conflicts in '{branch}' have not been resolved yet. \
         Resolve them (e.g. `git add <files>`), then re-run `--continue`."
    )]
    ConflictsNotResolved { branch: String },
    #[error(
        "Layer '{branch}' has a base ({base}) that is not an ancestor of its tip — \
         the stack metadata looks inconsistent. Fix the metadata or the branch, \
         then try again."
    )]
    InconsistentLayer { branch: String, base: String },
    #[error(
        "Layer '{branch}' has no commits over its base (base == tip: {base}). \
         Restacking would re-point the branch and silently drop all of its work. \
         This happens if the branch has no commits yet (make your first commit, \
         then restack), or if the layer's base in the stack metadata is wrong \
         (check how it was added with `stack insert --from <base>`)."
    )]
    EmptyLayer { branch: String, base: String },
    #[error(
        "Layer '{branch}' failed to rebase cleanly. Resolve the conflicts, then resume with \
         `stack {op} --continue` — or restore the previous state with `stack {op} --undo`.\n{details}"
    )]
    RestackConflict { branch: String, op: String, details: String },
    #[error(
        "Target '{target}' ({target_oid}) is behind the bottom layer '{branch}' ({bottom_tip}) — \
         '{branch}'s work is not in '{target}' yet. Advancing would drop '{branch}' from the stack \
         while re-targeting the remaining layers onto a '{target}' that lacks it.\n\n\
         This happens if the bottom layer's PR has not landed in '{target}' yet, or if your local \
         '{target}' is stale. Update the local target first:\n\
             git fetch origin {target}:{target}\n\
         then retry `stack advance`. Use `stack advance --force` to advance anyway."
    )]
    TargetBehindBottom {
        target: String,
        target_oid: String,
        branch: String,
        bottom_tip: String,
    },
}

/// Snapshot file names. Restack and advance keep separate snapshots so
/// that each `--undo` reverts its own operation.
const RESTACK_SNAPSHOT_FILE: &str = "undo_snapshot.json";
const ADVANCE_SNAPSHOT_FILE: &str = "advance_undo_snapshot.json";

/// Single in-flight operation state file. Only one restack/advance can be
/// in progress at a time, so they share this file; `--continue` and
/// `--undo` dispatch on the `op` field.
const STATE_FILE: &str = "restack_state.json";

fn snapshot_file_for(op: &str) -> &'static str {
    match op {
        "advance" => ADVANCE_SNAPSHOT_FILE,
        _ => RESTACK_SNAPSHOT_FILE,
    }
}

#[derive(Debug, clap::Args)]
pub struct RestackArgs {
    #[clap(long, action)]
    /// Continues the restack operation from where it left off
    continue_: bool,
    #[clap(long, action)]
    /// Undoes the last restack operation
    undo: bool,
}

pub async fn restack(ctx: &Ctx, args: RestackArgs) -> Result<(), CliError> {
    if args.continue_ {
        return continue_operation(ctx, "restack").await;
    }
    if args.undo {
        return perform_undo(ctx, "restack", RESTACK_SNAPSHOT_FILE).await;
    }

    // Refuse to start a new operation while one is still in flight (before
    // resolving the current stack: mid-flight we may be on a detached HEAD
    // and the in-flight error is the useful one).
    if let Some(existing) = load_state(ctx).await {
        return Err(Error::OperationInFlight { op: existing.op }.into());
    }

    let current_stack = ctx.current_stack().await?.ok_or(NoStack)?;
    start_operation(ctx, "restack", &current_stack, None).await
}

#[derive(Debug, clap::Args)]
pub struct AdvanceArgs {
    #[clap(long, action)]
    /// Continues the advance operation from where it left off
    continue_: bool,
    #[clap(long, action)]
    /// Undoes the last advance operation
    undo: bool,
    #[clap(long, action)]
    /// Advance even if the target is behind the bottom layer's work
    force: bool,
}

/// Advances the current stack forward by one layer: the bottom layer
/// (closest to the target) is assumed to have been merged into the target
/// externally, so it is dropped from the stack and the remaining layers are
/// re-targeted onto the target.
///
/// The dropped bottom layer's branch is left untouched — it is excluded from
/// the rebase entirely, so the new bottom lands directly on the target.
pub async fn advance(ctx: &Ctx, args: AdvanceArgs) -> Result<(), CliError> {
    if args.continue_ {
        return continue_operation(ctx, "advance").await;
    }
    if args.undo {
        return perform_undo(ctx, "advance", ADVANCE_SNAPSHOT_FILE).await;
    }

    if let Some(existing) = load_state(ctx).await {
        return Err(Error::OperationInFlight { op: existing.op }.into());
    }

    let current_stack = ctx.current_stack().await?.ok_or(NoStack)?;
    let bottom_branch = current_stack
        .layers
        .first()
        .map(|l| l.branch.clone())
        .expect("stack always has at least one layer");

    // Guard: the target must not sit *behind* the bottom layer's work, or we
    // would drop the bottom and re-target the rest onto a target that lacks
    // it. This check is TOCTOU-safe: it only distinguishes "target is missing
    // the bottom's commits" from "target contains them (possibly squashed)",
    // and a target that already contains them can only move further ahead —
    // so a passing check cannot become stale as the target advances.
    let target_oid = current_ref_for_branch(&ctx.cwd, &current_stack.target).await?;
    let bottom_tip = current_ref_for_branch(&ctx.cwd, &bottom_branch).await?;
    if !args.force
        && target_oid != bottom_tip
        && is_ancestor(ctx, &target_oid, &bottom_tip).await?
    {
        return Err(Error::TargetBehindBottom {
            target: current_stack.target.clone(),
            target_oid,
            branch: bottom_branch,
            bottom_tip,
        }
        .into());
    }

    start_operation(ctx, "advance", &current_stack, Some(bottom_branch)).await
}

/// Starts a new operation (the caller has verified none is in flight):
/// takes the undo snapshot, builds the pending layer plan (bottom-up),
/// persists the state, and runs it to completion or to the first conflict
/// (which leaves the state resumable).
async fn start_operation(
    ctx: &Ctx,
    op: &str,
    stack: &Stack,
    dropped: Option<String>,
) -> Result<(), CliError> {
    let original_branch = ctx.current_branch().await?;
    let undo_snapshot = UndoSnapshot::new(&ctx.cwd, stack, &original_branch).await?;

    let target_oid = current_ref_for_branch(&ctx.cwd, &stack.target).await?;
    let mut pending = Vec::with_capacity(stack.layers.len());
    for layer in &stack.layers {
        // For advance, the dropped (bottom) layer has already been merged
        // into the target; skip it so the new bottom rebases directly onto
        // the target.
        if dropped.as_deref() == Some(layer.branch.as_str()) {
            continue;
        }
        let old_tip = current_ref_for_branch(&ctx.cwd, &layer.branch).await?;
        // A layer whose base is its own tip has an empty replay range —
        // restacking it would re-point the branch and drop all of its work.
        // That is a metadata error (e.g. `stack insert` with the wrong base),
        // so fail loudly before mutating anything.
        if layer.base_oid == old_tip {
            return Err(Error::EmptyLayer {
                branch: layer.branch.clone(),
                base: layer.base_oid.clone(),
            }
            .into());
        }
        // The rebase range is `old_base..old_tip`; require the base to be an
        // ancestor of the tip, otherwise the range would silently include
        // other layers' commits.
        if !is_ancestor(ctx, &layer.base_oid, &old_tip).await? {
            return Err(Error::InconsistentLayer {
                branch: layer.branch.clone(),
                base: layer.base_oid.clone(),
            }
            .into());
        }
        pending.push(PendingLayer {
            branch: layer.branch.clone(),
            old_base: layer.base_oid.clone(),
            old_tip,
        });
    }

    let state = OpState {
        op: op.to_string(),
        target_oid: target_oid.clone(),
        new_base_for_next: target_oid,
        pending,
        dropped,
        undo: undo_snapshot,
    };
    save_state(ctx, &state).await?;

    let mut state = state;
    process_pending(ctx, &mut state, op).await
}

/// Resumes an in-progress operation of kind `expected_op`.
///
/// If a cherry-pick from the failed layer is still active, the user must
/// have resolved the conflicts: finish it, record the new tip, then keep
/// draining the remaining pending layers.
async fn continue_operation(ctx: &Ctx, expected_op: &str) -> Result<(), CliError> {
    let mut state = load_state(ctx)
        .await
        .ok_or_else(|| Error::NoInFlightOperation {
            op: expected_op.to_string(),
        })?;
    if state.op != expected_op {
        return Err(Error::OperationInFlight { op: state.op.clone() }.into());
    }

    let (has_pick_head, _) = git_status(
        &ctx.cwd,
        vec!["rev-parse", "-q", "--verify", "CHERRY_PICK_HEAD"],
    )
    .await?;
    if has_pick_head == 0 {
        let conflicted = state.pending.first().cloned().ok_or_else(|| {
            Error::NoInFlightOperation {
                op: expected_op.to_string(),
            }
        })?;

        if has_unmerged_paths(&ctx.cwd).await? {
            return Err(Error::ConflictsNotResolved {
                branch: conflicted.branch.clone(),
            }
            .into());
        }

        let (status, stderr) = git_status(&ctx.cwd, vec!["cherry-pick", "--continue"]).await?;
        if status != 0 {
            return Err(Error::RestackConflict {
                branch: conflicted.branch,
                op: state.op.clone(),
                details: stderr,
            }
            .into());
        }

        let new_tip = current_ref_for_branch(&ctx.cwd, "HEAD").await?;
        update_refs_atomic(&ctx.cwd, &[(conflicted.branch.as_str(), new_tip.as_str())]).await?;
        state.new_base_for_next = new_tip;
        state.pending.remove(0);
        save_state(ctx, &state).await?;
    }

    let op = state.op.clone();
    process_pending(ctx, &mut state, &op).await
}

/// Processes pending layers (bottom-up) until the operation is complete or
/// a layer fails to rebase. The state file is updated after every layer —
/// and the failed layer is kept at the front of `pending` — so
/// `--continue` picks up exactly where this left off.
async fn process_pending(ctx: &Ctx, state: &mut OpState, op: &str) -> Result<(), CliError> {
    while let Some(next) = state.pending.first().cloned() {
        let new_base = state.new_base_for_next.clone();

        if next.old_tip == next.old_base {
            // Empty layer (branch already at its old base): nothing to
            // re-apply, just re-point it at the new base.
            let new_tip = new_base;
            update_refs_atomic(&ctx.cwd, &[(next.branch.as_str(), new_tip.as_str())]).await?;
        } else if new_base == next.old_base {
            // Fast path: the base didn't move (e.g. the layer below didn't
            // change), so the layer is already correctly positioned. The
            // branch keeps its tip.
            //
            // Note: this must be an exact match, not "new_base is an ancestor
            // of old_tip" — the latter also fires when the new base is *before*
            // the layer's own base (a stale target), which would silently make
            // this layer own the commits of the dropped layers below it.
            let new_tip = next.old_tip;
            update_refs_atomic(&ctx.cwd, &[(next.branch.as_str(), new_tip.as_str())]).await?;
        } else if is_ancestor(ctx, &next.old_tip, &new_base).await? {
            // Fast path: every commit of this layer is already contained in
            // the new base (e.g. it was merged into the target in the
            // meantime). Re-applying would produce an empty cherry-pick, so
            // just fast-forward the branch to the new base.
            let new_tip = new_base;
            update_refs_atomic(&ctx.cwd, &[(next.branch.as_str(), new_tip.as_str())]).await?;
        } else {
            // Detach so updating the branch ref below never fights with the
            // user's checked-out branch, then cherry-pick the layer's
            // commits (old_base..old_tip) onto the new base.
            checkout(&ctx.cwd, &new_base).await?;
            let (status, stderr) = git_status(
                &ctx.cwd,
                vec!["cherry-pick", &format!("{}..{}", next.old_base, next.old_tip)],
            )
            .await?;
            if status != 0 {
                // Sequencer state is left intact; the state file still has
                // this layer at the front of `pending` for `--continue`.
                return Err(Error::RestackConflict {
                    branch: next.branch,
                    op: op.to_string(),
                    details: stderr,
                }
                .into());
            }
            let new_tip = current_ref_for_branch(&ctx.cwd, "HEAD").await?;
            update_refs_atomic(&ctx.cwd, &[(next.branch.as_str(), new_tip.as_str())]).await?;
        }

        state.new_base_for_next = current_ref_for_branch(&ctx.cwd, &next.branch).await?;
        state.pending.remove(0);
        save_state(ctx, state).await?;
    }

    finalize(ctx, state).await
}

/// Completes the operation: writes the final metadata (with the dropped
/// layer removed, if any) using the now-updated branch refs, keeps the
/// undo snapshot for `--undo`, restores the checked-out branch, and clears
/// the in-flight state.
async fn finalize(ctx: &Ctx, state: &OpState) -> Result<(), CliError> {
    let mut stack = get_stack_by_name(&ctx.git_folder, &state.undo.stack_name)
        .ok_or(StackNotFound(state.undo.stack_name.clone()))?;

    // Advance: the dropped bottom layer leaves the stack (its branch is
    // untouched).
    if let Some(dropped) = &state.dropped {
        stack.remove_layer(dropped);
    }

    // Every layer now sits on the new tip of the layer below it (the
    // target for the bottom layer) — record that in the metadata.
    let mut new_base = state.target_oid.clone();
    for layer in &mut stack.layers {
        layer.base_oid = new_base;
        new_base = current_ref_for_branch(&ctx.cwd, &layer.branch).await?;
    }
    stack.save_atomic()?;

    // Keep the pre-operation snapshot around so `--undo` can restore this
    // exact state; it is removed by `--undo`.
    write_snapshot(ctx, snapshot_file_for(&state.op), &state.undo).await?;
    restore_checkout(ctx, state.undo.checked_out.clone()).await?;
    clear_state(ctx).await?;

    if let Some(dropped) = &state.dropped {
        println!(
            "Advanced stack '{}' (dropped layer '{}')",
            state.undo.stack_name,
            dropped
        );
    } else {
        println!("Restacked stack '{}'", state.undo.stack_name);
    }

    Ok(())
}

/// Restores the state captured by the given undo snapshot: aborts any
/// in-flight cherry-pick, puts the layer refs and stack metadata back, and
/// re-checks-out the branch the user was on. Finally clears the snapshot.
///
/// If an operation of this kind is still in flight, its (fresher) embedded
/// snapshot is used instead of the last completed one.
async fn perform_undo(ctx: &Ctx, expected_op: &str, snapshot_file: &str) -> Result<(), CliError> {
    let (undo_snapshot, had_inflight) = match load_state(ctx).await {
        Some(state) if state.op == expected_op => (state.undo, true),
        _ => (
            load_snapshot(ctx, snapshot_file)
                .await
                .ok_or(Error::UndoSnapshotNotFound)?,
            false,
        ),
    };

    println!("Undoing the last {expected_op} operation");

    // If a cherry-pick from a failed operation is still in progress,
    // abort it first so the tree is clean when we restore refs.
    // (No-op if there is nothing in progress.)
    let _ = git_status(&ctx.cwd, vec!["cherry-pick", "--abort"]).await;

    let mut stack = get_stack_by_name(&ctx.git_folder, &undo_snapshot.stack_name)
        .ok_or(StackNotFound(undo_snapshot.stack_name.clone()))?;

    // Update the refs
    let previous_refs: Vec<(&str, &str)> = undo_snapshot
        .previous_refs
        .iter()
        .map(|(branch, reference)| (branch.as_str(), reference.as_str()))
        .collect();
    println!("Previous refs to restore: {:?}", previous_refs);
    update_refs_atomic(&ctx.cwd, &previous_refs).await?;
    stack.set_metadata(undo_snapshot.metadata);

    // Atomically update the stack metadata
    stack.save_atomic()?;
    restore_checkout(ctx, undo_snapshot.checked_out).await?;
    if had_inflight {
        clear_state(ctx).await?;
    }
    clear_snapshot(ctx, snapshot_file).await?;
    println!("Successfully undone the last {expected_op} operation");

    Ok(())
}

/// Restores the branch the user was on before the operation (which works on
/// a detached HEAD). No-op if they were already detached.
async fn restore_checkout(ctx: &Ctx, branch: Option<String>) -> Result<(), CliError> {
    if let Some(branch) = branch {
        crate::git::checkout_branch(&ctx.cwd, &branch).await?;
    }
    Ok(())
}

/// True if `ancestor` is an ancestor of `descendant`.
async fn is_ancestor(ctx: &Ctx, ancestor: &str, descendant: &str) -> Result<bool, CliError> {
    let (status, _) = git_status(
        &ctx.cwd,
        vec!["merge-base", "--is-ancestor", ancestor, descendant],
    )
    .await?;
    Ok(status == 0)
}

async fn state_path(ctx: &Ctx) -> Result<PathBuf, CliError> {
    Ok(ctx.stack_datadir().await?.join(STATE_FILE))
}

async fn save_state(ctx: &Ctx, state: &OpState) -> Result<(), CliError> {
    let path = state_path(ctx).await?;
    let file = File::create(path)?;
    serde_json::to_writer(file, state).map_err(|_| Error::FailedToWriteState)?;
    Ok(())
}

async fn load_state(ctx: &Ctx) -> Option<OpState> {
    let path = state_path(ctx).await.ok()?;
    let file = File::open(path).ok()?;
    serde_json::from_reader(file).ok()
}

async fn clear_state(ctx: &Ctx) -> Result<(), CliError> {
    let path = state_path(ctx).await?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}

async fn snapshot_path(ctx: &Ctx, file: &str) -> Result<PathBuf, CliError> {
    Ok(ctx.stack_datadir().await?.join(file))
}

async fn write_snapshot(ctx: &Ctx, file: &str, snapshot: &UndoSnapshot) -> Result<(), CliError> {
    let path = snapshot_path(ctx, file).await?;
    let undo_file = File::create(path)?;
    serde_json::to_writer(undo_file, snapshot)
        .map_err(|_| Error::FailedToWriteUndoSnapshot)?;
    Ok(())
}

async fn load_snapshot(ctx: &Ctx, file: &str) -> Option<UndoSnapshot> {
    let undo_file = File::open(&snapshot_path(ctx, file).await.ok()?).ok()?;
    let undo_snapshot: UndoSnapshot = serde_json::from_reader(undo_file).ok()?;
    Some(undo_snapshot)
}

async fn clear_snapshot(ctx: &Ctx, file: &str) -> Result<(), CliError> {
    let undo_snapshot_path = snapshot_path(ctx, file).await?;
    if undo_snapshot_path.exists() {
        std::fs::remove_file(&undo_snapshot_path)?;
    }
    Ok(())
}

/// A layer that still needs to be re-applied onto its new base.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct PendingLayer {
    branch: String,
    /// The layer's base when the operation started (the boundary between
    /// this layer's commits and the layer below).
    old_base: String,
    /// The layer's tip when the operation started.
    old_tip: String,
}

/// The in-flight restack/advance operation, persisted after every step so
/// it can be resumed (`--continue`) or rolled back (`--undo`).
#[derive(Debug, Serialize, Deserialize)]
struct OpState {
    /// The operation kind: "restack" or "advance".
    op: String,
    /// The stack target's OID when the operation started.
    target_oid: String,
    /// The base the next pending layer must be re-applied onto.
    new_base_for_next: String,
    /// Layers left to process, in bottom-up order. After a conflict, the
    /// failed layer is at the front.
    pending: Vec<PendingLayer>,
    /// For advance: the bottom layer to remove from the final metadata.
    dropped: Option<String>,
    /// The pre-operation snapshot, used by `--undo`.
    undo: UndoSnapshot,
}

#[derive(Debug, Serialize, Deserialize)]
struct UndoSnapshot {
    /// The name of the stack
    stack_name: String,
    /// The previous references of the stack layers
    previous_refs: HashMap<String, String>,
    /// The branch that was checked out before the operation (if any)
    #[serde(default)]
    checked_out: Option<String>,
    /// A snapshot of the stack metadata before the operation
    metadata: StackMetadata,
}

impl UndoSnapshot {
    pub async fn new(
        repo: &Path,
        stack: &Stack,
        checked_out: &Option<String>,
    ) -> Result<Self, CliError> {
        let mut previous_refs = HashMap::new();
        for layer in &stack.layers {
            let current_ref = current_ref_for_branch(repo, &layer.branch).await?;
            previous_refs.insert(layer.branch.clone(), current_ref);
        }

        Ok(Self {
            stack_name: stack.name().to_string(),
            previous_refs,
            checked_out: checked_out.clone(),
            metadata: stack.metadata().clone(),
        })
    }
}
