use std::path::PathBuf;
use tokio::process::Command;

pub const DEFAULT_BRANCH_TARGET: &str = "main";

#[derive(Debug, thiserror::Error)]
pub enum GitRepoError {
    #[error("Git repository error")]
    GitRepositoryError,
    #[error("Not on any branch")]
    NotOnBranch,
    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),
}

pub async fn get_repo_git_folder(dir: &PathBuf) -> Result<PathBuf, GitRepoError> {
    let mut toplevel_command = Command::new("git");
    toplevel_command
        .arg("rev-parse")
        .arg("--show-toplevel")
        .current_dir(dir);

    let mut dot_git_dir = Command::new("git");
    dot_git_dir
        .arg("rev-parse")
        .arg("--git-dir")
        .current_dir(dir);

    let toplevel_output = toplevel_command.output().await?;
    let dot_git_output = dot_git_dir.output().await?;

    if !toplevel_output.status.success() || !dot_git_output.status.success() {
        return Err(GitRepoError::GitRepositoryError);
    }
    let repo_path = String::from_utf8(toplevel_output.stdout)
        .map_err(|_| GitRepoError::GitRepositoryError)?
        .trim()
        .to_string();
    let git_dir = String::from_utf8(dot_git_output.stdout)
        .map_err(|_| GitRepoError::GitRepositoryError)?
        .trim()
        .to_string();
    let path: PathBuf = repo_path.into();
    Ok(path.join(git_dir))
}

pub async fn get_current_branch(dir: &PathBuf) -> Result<String, GitRepoError> {
    let mut branch_command = Command::new("git");
    branch_command
        .arg("branch")
        .arg("--show-current")
        .current_dir(dir);

    let branch_output = branch_command.output().await?;

    if !branch_output.status.success() {
        return Err(GitRepoError::GitRepositoryError);
    }

    let branch_name = String::from_utf8(branch_output.stdout)
        .map_err(|_| GitRepoError::GitRepositoryError)?
        .trim()
        .to_string();

    if branch_name.is_empty() {
        return Err(GitRepoError::NotOnBranch);
    }

    Ok(branch_name)
}
