// SPDX-FileCopyrightText: 2025 Aaron Dewes <aaron@nirvati.org>
//
// SPDX-License-Identifier: AGPL-3.0-or-later

use std::num::NonZeroU32;
use std::path::{Path, PathBuf};

use gix::bstr::BStr;
use gix::prepare_clone;
use tempfile::TempDir;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum GitError {
    #[error("Failed to join Tokio task: {0}")]
    TokioJoin(#[from] tokio::task::JoinError),
    #[error("Failed to parse branch name as refspec: {0}")]
    GitBranchParse(#[from] gix::refspec::parse::Error),
    #[error("Failed to validate reference name: {0}")]
    GitRefValidate(#[from] gix::validate::reference::name::Error),
    #[error("Git fetch failed: {0}")]
    GitFetch(#[from] gix::clone::fetch::Error),
    #[error("Git clone failed: {0}")]
    GitClone(#[from] gix::clone::Error),
    #[error("Git checkout failed: {0}")]
    GitCheckout(#[from] gix::clone::checkout::main_worktree::Error),
    #[error("Git URL error: {0}")]
    GitUrl(#[from] gix::url::parse::Error),
    #[error("Target directory already exists and is not empty: {0}")]
    DirExists(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Other Git error: {0}")]
    Other(String),
}

pub async fn clone(repo_url: gix::Url, branch: &str, target: PathBuf) -> Result<(), GitError> {
    tracing::info!("Cloning {repo_url:?} into {target:?}...");
    let rspec = format!("refs/heads/{}", branch);
    tokio::task::spawn_blocking(move || {
        let prepare_clone = prepare_clone(repo_url, target)?;
        let (mut prepare_checkout, _) = prepare_clone
            .with_ref_name(Some(&rspec))?
            .with_shallow(gix::remote::fetch::Shallow::DepthAtRemote(
                NonZeroU32::new(1).unwrap(),
            ))
            .fetch_then_checkout(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)?;
        let _ = prepare_checkout
            .main_worktree(gix::progress::Discard, &gix::interrupt::IS_INTERRUPTED)?;
        Ok(())
    })
    .await?
}

pub fn copy_dir_recursively<'a>(
    src: &'a Path,
    dst: &'a Path,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<(), GitError>> + Send + 'a>> {
    Box::pin(async move {
        tokio::fs::create_dir_all(dst).await?;
        let mut entries = tokio::fs::read_dir(src).await?;
        while let Some(entry) = entries.next_entry().await? {
            let file_type = entry.file_type().await?;
            let dest_path = dst.join(entry.file_name());
            if file_type.is_dir() {
                copy_dir_recursively(&entry.path(), &dest_path).await?;
            } else if file_type.is_file() {
                tokio::fs::copy(&entry.path(), &dest_path).await?;
            } // Ignore symlinks and other types
        }
        Ok(())
    })
}

pub async fn sync_repo(repo_dir: &Path, git_url: &str, git_branch: &str) -> Result<(), GitError> {
    if repo_dir.join(".git").exists() {
        // Repo already exists, re-clone to a tmp dir and then replace
        let temp_dir = TempDir::new()?;
        let temp_path = temp_dir.path().to_path_buf();
        clone(
            gix::Url::from_bytes(BStr::new(git_url))?,
            git_branch,
            temp_path.join("repo"),
        )
        .await?;
        let temp_repo_path = temp_path.join("repo");
        // Remove the old repo
        std::fs::remove_dir_all(repo_dir)?;
        // Move the new repo into place, try rename, otherwise copy
        match std::fs::rename(&temp_repo_path, repo_dir) {
            Ok(_) => (),
            Err(_) => {
                copy_dir_recursively(&temp_repo_path, repo_dir).await?;
            }
        }
        Ok(())
    } else {
        if repo_dir.exists() {
            // If it isn't empty, return an error
            if repo_dir
                .read_dir()
                .map_err(|e| GitError::Other(e.to_string()))?
                .next()
                .is_some()
            {
                return Err(GitError::DirExists(repo_dir.to_string_lossy().to_string()));
            }
        }
        let url = gix::Url::from_bytes(BStr::new(git_url))?;
        clone(url, git_branch, repo_dir.to_path_buf()).await
    }
}

pub struct CommitInfo {
    pub hash: String,
    pub timestamp: u64,
    pub author: String,
    pub title: String,
}

pub fn get_head_commit_info(repo_dir: &std::path::Path) -> Option<CommitInfo> {
    let repo = gix::open(repo_dir).ok()?;
    let mut head = repo.head().ok()?;
    let commit = head.peel_to_commit().ok()?;
    let hash = commit.id().to_string();
    let timestamp = commit.committer().ok()?.time().ok()?.seconds as u64;
    let author = commit.committer().ok()?.name.to_string();
    let title = commit
        .message()
        .map(|m| m.title.to_string())
        .unwrap_or_default();
    Some(CommitInfo {
        hash,
        timestamp,
        author,
        title,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_sync_and_get_commit() {
        let temp_dir = tempfile::tempdir().unwrap();
        let repo_path = temp_dir.path().join("test_repo");
        let git_url = "https://github.com/octocat/Hello-World.git";
        let git_branch = "master";
        let result = sync_repo(&repo_path, git_url, git_branch).await;
        assert!(result.is_ok());
        assert!(repo_path.exists());
        // Ensure a file called README exists in the cloned repo with the content "Hello World!\n"
        let readme_path = repo_path.join("README");
        assert!(readme_path.exists());
        let content = std::fs::read_to_string(readme_path).unwrap();
        assert_eq!(content, "Hello World!\n");
        let commit_info = get_head_commit_info(&repo_path).unwrap();
        assert_eq!(commit_info.hash, "7fd1a60b01f91b314f59955a4e4d4e80d8edf11d");
        assert_eq!(commit_info.author, "The Octocat");
        assert_eq!(
            commit_info.title,
            "Merge pull request #6 from Spaceghost/patch-1"
        );
        assert_eq!(commit_info.timestamp, 1331075210); // 2012-03-06 15:06:50 UTC-0800
    }
}
