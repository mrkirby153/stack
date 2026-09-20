use std::path::PathBuf;
use tokio::process::Command;

use crate::git::GitRepoError::CommandUtfError;

pub const DEFAULT_BRANCH_TARGET: &str = "main";

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
}

pub async fn get_repo_git_folder(dir: &PathBuf) -> Result<PathBuf, GitRepoError> {
    let repo_path = git(dir, vec!["rev-parse", "--show-toplevel"]).await?;
    let git_dir = git(dir, vec!["rev-parse", "--git-dir"]).await?;
    let path: PathBuf = repo_path.into();
    Ok(path.join(git_dir))
}

pub async fn get_current_branch(dir: &PathBuf) -> Result<Option<String>, GitRepoError> {
    let branch_name = git(dir, vec!["branch", "--show-current"]).await?;

    if branch_name.is_empty() {
        return Ok(None);
    }

    Ok(Some(branch_name))
}

pub async fn current_ref(dir: &PathBuf) -> Result<String, GitRepoError> {
    let reference = git(dir, vec!["rev-parse", "HEAD"]).await?;
    Ok(reference)
}

pub async fn checkout_branch(dir: &PathBuf, branch: &str) -> Result<(), GitRepoError> {
    foreground_git(dir, vec!["checkout", branch]).await
}

async fn git(dir: &PathBuf, args: Vec<&str>) -> Result<String, GitRepoError> {
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

async fn foreground_git(dir: &PathBuf, args: Vec<&str>) -> Result<(), GitRepoError> {
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
