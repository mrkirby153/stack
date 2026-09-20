use std::{
    path::{Path, PathBuf},
    process::Stdio,
};
use tokio::process::Command;

use crate::git::GitRepoError::CommandUtfError;

pub const DEFAULT_BRANCH_TARGET: &str = "main";

const ZERO_SHA: &str = "0000000000000000000000000000000000000000";

#[derive(Debug, thiserror::Error)]
pub enum GitRepoError {
    #[error("Error executing git command \"{0}\": {1}")]
    GitCommandError(String, String),
    #[error("Error executing git command in the foreground: {0}")]
    ForegroundGitError(String),
    #[error("Error converting command output to UTF-8: {0}")]
    CommandUtfError(#[from] std::string::FromUtf8Error),
    #[error("Not on any branch")]
    NotOnBranch,
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Could not open stdin for git command")]
    CouldNotOpenStdin,
}

pub async fn get_repo_git_folder(dir: &Path) -> Result<PathBuf, GitRepoError> {
    let repo_path = git(dir, vec!["rev-parse", "--show-toplevel"]).await?;
    let git_dir = git(dir, vec!["rev-parse", "--git-dir"]).await?;
    let path: PathBuf = repo_path.into();
    Ok(path.join(git_dir))
}

pub async fn get_current_branch(dir: &Path) -> Result<Option<String>, GitRepoError> {
    let branch_name = git(dir, vec!["branch", "--show-current"]).await?;

    if branch_name.is_empty() {
        return Ok(None);
    }

    Ok(Some(branch_name))
}

pub async fn current_ref(dir: &Path) -> Result<String, GitRepoError> {
    let reference = git(dir, vec!["rev-parse", "HEAD"]).await?;
    Ok(reference)
}

pub async fn current_ref_for_branch(dir: &Path, branch: &str) -> Result<String, GitRepoError> {
    let reference = git(dir, vec!["rev-parse", branch]).await?;
    Ok(reference)
}

pub async fn checkout_branch(dir: &Path, branch: &str) -> Result<(), GitRepoError> {
    foreground_git(dir, vec!["checkout", branch]).await
}

pub async fn update_refs_atomic(dir: &Path, updates: &[(&str, &str)]) -> Result<(), GitRepoError> {
    use tokio::io::AsyncWriteExt;
    let mut command = Command::new("git");
    command.args(["update-ref", "--stdin"]).current_dir(dir);
    command.stdin(Stdio::piped());

    let mut child = command.spawn()?;
    let mut stdin = child.stdin.take().ok_or(GitRepoError::CouldNotOpenStdin)?;
    let mut input = String::new();
    for (branch, sha) in updates {
        input.push_str(&format!("update refs/heads/{branch} {sha}\n"));
    }
    stdin.write_all(input.as_bytes()).await?;
    stdin.flush().await?;
    drop(stdin); // close the write end so git sees EOF and processes the batch

    let status = child.wait().await?;
    if !status.success() {
        return Err(GitRepoError::ForegroundGitError(
            "git update-ref --stdin".to_string(),
        ));
    }

    Ok(())
}

async fn git(dir: &Path, args: Vec<&str>) -> Result<String, GitRepoError> {
    let mut command = Command::new("git");
    command.args(args.clone()).current_dir(dir);

    let output = command.output().await?;

    if !output.status.success() {
        return Err(GitRepoError::GitCommandError(
            format!("git {}", args.join(" ")),
            String::from_utf8_lossy(&output.stderr).to_string(),
        ));
    }

    let result = String::from_utf8(output.stdout)
        .map_err(CommandUtfError)?
        .trim()
        .to_string();

    Ok(result)
}

async fn foreground_git(dir: &Path, args: Vec<&str>) -> Result<(), GitRepoError> {
    let mut command = Command::new("git");
    command.args(args.clone()).current_dir(dir);

    let mut child = command.spawn()?;

    let status = child.wait().await?;
    if !status.success() {
        return Err(GitRepoError::ForegroundGitError(format!(
            "git {}",
            args.join(" ")
        )));
    }

    Ok(())
}
