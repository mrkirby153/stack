use serde::Serialize;

use crate::{
    commands::{
        CliError, Ctx,
        consistency::{self, LayerCheck, LayerIssue},
    },
    git::get_current_branch,
};

#[derive(Debug, clap::Args)]
pub struct Args {
    /// Emit machine-readable JSON (for prompts / scripts) instead of the
    /// human-readable summary.
    #[clap(long)]
    json: bool,
}

/// Machine-readable form of `stack status`, stable for prompts and scripts
/// (e.g. a Starship custom command).
#[derive(Debug, Serialize)]
struct StatusJson {
    /// The stack's name.
    stack: String,
    /// The stack's target branch (e.g. `main`).
    target: String,
    /// The branch currently checked out (a layer of this stack).
    branch: String,
    /// Index of the current layer, 0 = bottom, `size - 1` = top.
    position: usize,
    /// Number of layers in the stack.
    size: usize,
    /// True when every layer sits exactly on the tip of the layer below it.
    up_to_date: bool,
    /// True when at least one layer is behind / needs to be re-applied.
    needs_restack: bool,
    /// True when at least one layer's metadata is broken (branch missing or
    /// base not an ancestor of the tip).
    inconsistent: bool,
    /// Coarse summary: one of `up_to_date`, `inconsistent`, `needs_restack`,
    /// or `error` (any other not-up-to-date state, e.g. an empty layer).
    status: String,
    /// The layers, bottom-up, each with its specific problem (if any).
    layers: Vec<LayerJson>,
}

#[derive(Debug, Serialize)]
struct LayerJson {
    branch: String,
    /// True for the layer of the currently checked-out branch.
    current: bool,
    /// The specific problem with this layer, or `null` when it is fine.
    issue: Option<&'static str>,
}

/// A stable snake_case tag for each kind of layer problem.
fn issue_tag(issue: &LayerIssue) -> &'static str {
    match issue {
        LayerIssue::BranchNotFound => "branch_not_found",
        LayerIssue::BaseNotAncestor { .. } => "base_not_ancestor",
        LayerIssue::EmptyLayer { .. } => "empty_layer",
        LayerIssue::BehindBelow { .. } => "behind_below",
        LayerIssue::BelowChanged { .. } => "below_changed",
    }
}

/// The three summary states, independent of the current branch:
/// - `up_to_date`    : no layer has any problem.
/// - `needs_restack` : some layer is behind / its base was rewritten.
/// - `inconsistent`  : some layer's metadata is broken.
fn summarize(checks: &[LayerCheck]) -> (bool, bool, bool) {
    let inconsistent = checks.iter().any(|c| {
        matches!(
            c.issue,
            Some(LayerIssue::BranchNotFound) | Some(LayerIssue::BaseNotAncestor { .. })
        )
    });
    let needs_restack = checks.iter().any(|c| {
        matches!(
            c.issue,
            Some(LayerIssue::BehindBelow { .. }) | Some(LayerIssue::BelowChanged { .. })
        )
    });
    let up_to_date = checks.iter().all(|c| c.issue.is_none());
    (up_to_date, needs_restack, inconsistent)
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<(), CliError> {
    let current_stack = ctx.current_stack().await?.ok_or(CliError::NoStack)?;
    let current_branch = get_current_branch(&ctx.cwd)
        .await?
        .ok_or(CliError::NotOnBranch)?;
    let position = current_stack.get_position(&current_branch);

    let checks = consistency::evaluate(ctx, &current_stack).await?;
    let (up_to_date, needs_restack, inconsistent) = summarize(&checks);

    if args.json {
        let status = if up_to_date {
            "up_to_date"
        } else if inconsistent {
            "inconsistent"
        } else if needs_restack {
            "needs_restack"
        } else {
            "error"
        };
        let out = StatusJson {
            stack: current_stack.name().to_string(),
            target: current_stack.target.clone(),
            branch: current_branch.clone(),
            position: position.unwrap_or(usize::MAX),
            size: current_stack.size(),
            up_to_date,
            needs_restack,
            inconsistent,
            status: status.to_string(),
            layers: checks
                .iter()
                .map(|c| LayerJson {
                    branch: c.branch.clone(),
                    current: c.branch == current_branch,
                    issue: c.issue.as_ref().map(issue_tag),
                })
                .collect(),
        };
        println!("{}", serde_json::to_string(&out)?);
        return Ok(());
    }

    println!(
        "Current stack: {} (target: {})",
        current_stack.name(),
        current_stack.target
    );
    println!();

    let mut all_ok = true;
    let mut lower_needs_restack = false;

    for (i, check) in checks.iter().enumerate() {
        let marker = if Some(i) == position { "*" } else { " " };

        let issue = match check.issue.as_ref() {
            Some(issue) => issue,
            None => {
                if lower_needs_restack {
                    all_ok = false;
                    println!(
                        "{} {}  needs restack (layers below changed)",
                        marker, check.branch
                    );
                } else {
                    println!("{} {}", marker, check.branch);
                }
                continue;
            }
        };

        match issue {
            LayerIssue::BranchNotFound | LayerIssue::BaseNotAncestor { .. } => {
                all_ok = false;
            }
            LayerIssue::EmptyLayer { .. } => {
                all_ok = false;
            }
            LayerIssue::BehindBelow { .. } | LayerIssue::BelowChanged { .. } => {
                all_ok = false;
                lower_needs_restack = true;
            }
        }

        let needs_restack_suffix = matches!(
            issue,
            LayerIssue::BehindBelow { .. } | LayerIssue::BelowChanged { .. }
        );
        if needs_restack_suffix {
            println!(
                "{} {}  {} — needs restack",
                marker,
                check.branch,
                issue.describe()
            );
        } else {
            println!("{} {}  ! {}", marker, check.branch, issue.describe());
        }
    }

    println!();
    if all_ok {
        println!("Stack is up to date");
    } else if inconsistent {
        println!("Stack metadata is inconsistent — fix the affected layers or their metadata.");
    } else if needs_restack {
        println!("Stack needs to be restacked");
    }

    Ok(())
}
