use crate::{
    commands::{CliError, Ctx},
    git::{count_commits, current_ref_for_branch, get_current_branch, is_ancestor},
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

    let mut all_ok = true;
    let mut needs_restack = false;
    let mut inconsistent = false;

    let mut below_tip = current_ref_for_branch(&ctx.cwd, &current_stack.target).await?;
    let mut lower_needs_restack = false;

    for (i, layer) in current_stack.layers.iter().enumerate() {
        let marker = if Some(i) == position { "*" } else { " " };

        let layer_tip = match current_ref_for_branch(&ctx.cwd, &layer.branch).await {
            Ok(tip) => tip,
            Err(_) => {
                all_ok = false;
                inconsistent = true;
                println!("{} {}  ! branch not found", marker, layer.branch);
                continue;
            }
        };

        // Integrity: the layer must sit on top of its recorded base.
        if !is_ancestor(&ctx.cwd, &layer.base_oid, &layer_tip).await? {
            all_ok = false;
            inconsistent = true;
            println!(
                "{} {}  ! inconsistent: base {} is not an ancestor of tip {}",
                marker,
                layer.branch,
                short_oid(&layer.base_oid),
                short_oid(&layer_tip)
            );
            below_tip = layer_tip;
            continue;
        }

        if layer.base_oid == layer_tip {
            all_ok = false;
            println!(
                "{} {}  ! empty: no commits over base {}",
                marker,
                layer.branch,
                short_oid(&layer.base_oid)
            );
            // An empty layer is re-pointed by a restack, so the layers
            // above are unaffected by it moving.
            below_tip = layer_tip;
            continue;
        }

        // Compare this layer's recorded base to the layer below's current tip
        // to see whether the base has shifted
        let (shifted, reason) = if layer.base_oid == below_tip {
            (false, String::new())
        } else if is_ancestor(&ctx.cwd, &layer.base_oid, &below_tip).await? {
            let n = count_commits(&ctx.cwd, &layer.base_oid, &below_tip).await?;
            (true, format!("{n} commit(s) behind"))
        } else {
            let removed = count_commits(&ctx.cwd, &below_tip, &layer.base_oid).await?;
            let reason = if removed > 0 {
                format!("{removed} commit(s) deleted below")
            } else {
                "base below changed".to_string()
            };
            (true, reason)
        };

        below_tip = layer_tip;

        if shifted {
            all_ok = false;
            needs_restack = true;
            lower_needs_restack = true;
            println!("{} {}  {} — needs restack", marker, layer.branch, reason);
        } else if lower_needs_restack {
            all_ok = false;
            needs_restack = true;
            println!(
                "{} {}  needs restack (layers below changed)",
                marker, layer.branch
            );
        } else {
            println!("{} {}", marker, layer.branch);
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

/// Truncates an OID for display (falls back to the full OID if too short).
fn short_oid(oid: &str) -> &str {
    if oid.len() > 7 { &oid[..7] } else { oid }
}
