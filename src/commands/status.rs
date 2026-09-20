use crate::{
    commands::{
        CliError, Ctx,
        consistency::{self, LayerIssue},
    },
    git::get_current_branch,
};

pub async fn run(ctx: &Ctx) -> Result<(), CliError> {
    let current_stack = ctx.current_stack().await?.ok_or(CliError::NoStack)?;
    let current_branch = get_current_branch(&ctx.cwd)
        .await?
        .ok_or(CliError::NotOnBranch)?;
    let position = current_stack.get_position(&current_branch);

    println!(
        "Current stack: {} (target: {})",
        current_stack.name(),
        current_stack.target
    );
    println!();

    let checks = consistency::evaluate(ctx, &current_stack).await?;

    let mut all_ok = true;
    let mut needs_restack = false;
    let mut inconsistent = false;
    let mut lower_needs_restack = false;

    for (i, check) in checks.iter().enumerate() {
        let marker = if Some(i) == position { "*" } else { " " };

        let issue = match check.issue.as_ref() {
            Some(issue) => issue,
            None => {
                if lower_needs_restack {
                    all_ok = false;
                    needs_restack = true;
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
                inconsistent = true;
            }
            LayerIssue::EmptyLayer { .. } => {
                all_ok = false;
            }
            LayerIssue::BehindBelow { .. } | LayerIssue::BelowChanged { .. } => {
                all_ok = false;
                needs_restack = true;
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
