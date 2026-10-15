use crate::{
    commands::{CliError, Ctx},
    git::{foreground_git, git_status, list_remotes},
};

/// `stack publish` — force-push (with lease) every branch in the current
/// stack to a remote, bottom-up.  The target branch (e.g. `main`) is *not*
/// pushed; it is managed by normal git.
#[derive(Debug, clap::Args)]
pub struct Args {
    /// The remote to publish to. Defaults to `origin`.
    #[clap(long, default_value = "origin")]
    remote: String,

    /// Push with a plain `--force`, bypassing the `--force-with-lease`
    /// protection (overwrites whatever is on the remote). Use with care.
    #[clap(long, action)]
    force: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(
        "Remote '{0}' does not exist. Add it (e.g. `git remote add {0} <url>`) \
         or point --remote at an existing one."
    )]
    RemoteNotFound(String),
    #[error(
        "Branch '{0}' does not exist locally, so it cannot be pushed. \
         Create it, or fix the stack's metadata before publishing."
    )]
    LocalBranchMissing(String),
    #[error(
        "Push of '{branch}' to '{remote}' was rejected.\n\
         With --force-with-lease, git refuses to overwrite remote changes you \
         have not fetched since your last push. If that is the case, fetch (and \
         re-run `stack restack` if needed) and try again — or override with \
         `stack publish --force`."
    )]
    PushRejected { branch: String, remote: String },
}

pub async fn run(ctx: &Ctx, args: Args) -> Result<(), CliError> {
    let stack = ctx.current_stack().await?.ok_or(CliError::NoStack)?;

    if stack.layers.is_empty() {
        println!("Stack '{}' has no layers to publish", stack.name());
        return Ok(());
    }

    // Verify the remote is real before we start pushing anything.
    let remotes = list_remotes(&ctx.cwd).await?;
    if !remotes.iter().any(|r| r == &args.remote) {
        return Err(Error::RemoteNotFound(args.remote.clone()).into());
    }

    let force_flag = if args.force {
        "--force"
    } else {
        "--force-with-lease"
    };

    println!(
        "Publishing {} layer(s) of stack '{}' to '{}':",
        stack.layers.len(),
        stack.name(),
        args.remote
    );

    for layer in &stack.layers {
        let branch = &layer.branch;

        // The local branch must exist before we can push it.
        let (status, _) = git_status(
            &ctx.cwd,
            vec![
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
        )
        .await?;
        if status != 0 {
            return Err(Error::LocalBranchMissing(branch.clone()).into());
        }

        println!("=> Pushing '{branch}'…");
        if foreground_git(
            &ctx.cwd,
            vec![
                "push",
                force_flag,
                &args.remote,
                &format!("{branch}:{branch}"),
            ],
        )
        .await
        .is_err()
        {
            // `foreground_git` already streamed git's own error to the
            // terminal; add which branch failed and what to do about it.
            return Err(Error::PushRejected {
                branch: branch.clone(),
                remote: args.remote.clone(),
            }
            .into());
        }
        println!("==> Published '{branch}'");
    }

    println!(
        "Published {} layer(s) of stack '{}' to '{}'",
        stack.layers.len(),
        stack.name(),
        args.remote
    );
    Ok(())
}
