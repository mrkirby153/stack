use crate::{
    commands::{CliError, Ctx},
    git::{count_commits, current_ref_for_branch, is_ancestor},
    metadata::Stack,
};

/// A single consistency problem found in a layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayerIssue {
    /// The layer's branch could not be resolved.
    BranchNotFound,
    /// The recorded base is not an ancestor of the branch's tip — the replay
    /// range would include other layers' commits.
    BaseNotAncestor { base: String, tip: String },
    /// The layer has no commits of its own (base == tip).
    EmptyLayer { base: String },
    /// The layer's base is behind the current tip of the layer below it.
    BehindBelow { commits: u64 },
    /// The layer's base is not on top of the current tip of the layer below
    /// it (history below the layer was rewritten, or commits removed).
    BelowChanged { removed: u64 },
}

impl LayerIssue {
    /// A short human-readable description of the problem.
    pub fn describe(&self) -> String {
        match self {
            LayerIssue::BranchNotFound => "branch not found".to_string(),
            LayerIssue::BaseNotAncestor { base, tip } => format!(
                "inconsistent: base {} is not an ancestor of tip {}",
                short_oid(base),
                short_oid(tip)
            ),
            LayerIssue::EmptyLayer { base } => {
                format!("empty: no commits over base {}", short_oid(base))
            }
            LayerIssue::BehindBelow { commits } => format!("{commits} commit(s) behind"),
            LayerIssue::BelowChanged { removed } => {
                if *removed > 0 {
                    format!("{removed} commit(s) deleted below")
                } else {
                    "base below changed".to_string()
                }
            }
        }
    }
}

/// The consistency result for one layer; a `None` issue means the layer is
/// up to date.
#[derive(Debug, Clone)]
pub struct LayerCheck {
    pub branch: String,
    pub issue: Option<LayerIssue>,
}

/// Evaluates every layer bottom-up, performing the same checks that
/// `stack status` uses to decide whether the stack is up to date.
///
/// Each layer's recorded base is compared against the current tip of the
/// layer below it (the stack target for the bottom layer).
///
/// The bottom layer being *behind* the target is not an issue: it is the
/// normal post-merge state (its work just landed in the target, usually
/// squash-merged), and both `stack status` and `stack advance` treat it
/// as up to date. Every other problem, on the bottom or above, still is.
pub async fn evaluate(ctx: &Ctx, stack: &Stack) -> Result<Vec<LayerCheck>, CliError> {
    let mut checks = Vec::with_capacity(stack.layers.len());
    let mut below_tip = current_ref_for_branch(&ctx.cwd, &stack.target).await?;

    for (i, layer) in stack.layers.iter().enumerate() {
        let check = match current_ref_for_branch(&ctx.cwd, &layer.branch).await {
            Err(_) => LayerCheck {
                branch: layer.branch.clone(),
                issue: Some(LayerIssue::BranchNotFound),
            },
            Ok(tip) => {
                let issue = if !is_ancestor(&ctx.cwd, &layer.base_oid, &tip).await? {
                    Some(LayerIssue::BaseNotAncestor {
                        base: layer.base_oid.clone(),
                        tip: tip.clone(),
                    })
                } else if layer.base_oid == tip {
                    Some(LayerIssue::EmptyLayer {
                        base: layer.base_oid.clone(),
                    })
                } else if layer.base_oid == below_tip {
                    None
                } else if is_ancestor(&ctx.cwd, &layer.base_oid, &below_tip).await? {
                    let commits = count_commits(&ctx.cwd, &layer.base_oid, &below_tip).await?;
                    Some(LayerIssue::BehindBelow { commits })
                } else {
                    let removed = count_commits(&ctx.cwd, &below_tip, &layer.base_oid).await?;
                    Some(LayerIssue::BelowChanged { removed })
                };
                // The bottom being behind the target is the expected
                // post-merge state, not an out-of-date layer.
                let issue = if i == 0 {
                    match issue {
                        Some(LayerIssue::BehindBelow { .. }) => None,
                        other => other,
                    }
                } else {
                    issue
                };
                below_tip = tip;
                LayerCheck {
                    branch: layer.branch.clone(),
                    issue,
                }
            }
        };
        checks.push(check);
    }

    Ok(checks)
}

/// Truncates an OID for display (falls back to the full OID if too short).
fn short_oid(oid: &str) -> &str {
    if oid.len() > 7 { &oid[..7] } else { oid }
}
