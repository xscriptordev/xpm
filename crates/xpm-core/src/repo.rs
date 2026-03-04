//! Repository manager — handles predefined and user-added temporary repos.
//!
//! Predefined repos live in `/etc/xpm.conf`. User-added repos are stored
//! as individual TOML files under `/etc/xpm.d/`.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::config::SigLevel;
use crate::error::ConfigError;

/// Default directory for user-added repository files.
const DEFAULT_REPOS_DIR: &str = "/etc/xpm.d";

/// A user-managed repository entry stored in `/etc/xpm.d/<name>.toml`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRepo {
    /// Repository name.
    pub name: String,
    /// Mirror URLs.
    pub server: Vec<String>,
    /// Signature verification level.
    #[serde(default)]
    pub sig_level: Option<SigLevel>,
}

/// Manages the lifecycle of user-added repositories.
#[derive(Debug)]
pub struct RepoManager {
    /// Path to the directory containing user repo files.
    repos_dir: PathBuf,
}

impl RepoManager {
    /// Create a new repo manager pointing to the given directory.
    pub fn new(repos_dir: &Path) -> Self {
        Self {
            repos_dir: repos_dir.to_path_buf(),
        }
    }

    /// Create a repo manager using the default path (`/etc/xpm.d/`).
    pub fn default_dir() -> Self {
        Self::new(Path::new(DEFAULT_REPOS_DIR))
    }

    /// Ensure the repos directory exists.
    fn ensure_dir(&self) -> Result<(), ConfigError> {
        if !self.repos_dir.exists() {
            fs::create_dir_all(&self.repos_dir)
                .map_err(|e| ConfigError::ReadError { source: e })?;
        }
        Ok(())
    }

    /// Path to a specific repo file.
    fn repo_path(&self, name: &str) -> PathBuf {
        self.repos_dir.join(format!("{name}.toml"))
    }

    /// Add a new user repository.
    pub fn add(&self, name: &str, url: &str) -> Result<(), ConfigError> {
        self.ensure_dir()?;

        let path = self.repo_path(name);
        if path.exists() {
            return Err(ConfigError::Validation {
                message: format!("repository '{name}' already exists — remove it first"),
            });
        }

        let repo = UserRepo {
            name: name.to_string(),
            server: vec![url.to_string()],
            sig_level: None,
        };

        let contents = toml::to_string_pretty(&repo).map_err(|e| ConfigError::Validation {
            message: format!("failed to serialize repo config: {e}"),
        })?;

        fs::write(&path, contents).map_err(|e| ConfigError::ReadError { source: e })?;

        tracing::info!(name, url, path = %path.display(), "added user repository");
        Ok(())
    }

    /// Remove a user repository by name.
    pub fn remove(&self, name: &str) -> Result<(), ConfigError> {
        let path = self.repo_path(name);
        if !path.exists() {
            return Err(ConfigError::NotFound { path: path.clone() });
        }

        fs::remove_file(&path).map_err(|e| ConfigError::ReadError { source: e })?;

        tracing::info!(name, path = %path.display(), "removed user repository");
        Ok(())
    }

    /// List all user-added repositories.
    pub fn list(&self) -> Result<Vec<UserRepo>, ConfigError> {
        if !self.repos_dir.exists() {
            return Ok(Vec::new());
        }

        let mut repos = Vec::new();

        let entries =
            fs::read_dir(&self.repos_dir).map_err(|e| ConfigError::ReadError { source: e })?;

        for entry in entries {
            let entry = entry.map_err(|e| ConfigError::ReadError { source: e })?;
            let path = entry.path();

            if path.extension().is_some_and(|ext| ext == "toml") {
                let contents =
                    fs::read_to_string(&path).map_err(|e| ConfigError::ReadError { source: e })?;

                let repo: UserRepo =
                    toml::from_str(&contents).map_err(|e| ConfigError::ParseError { source: e })?;

                repos.push(repo);
            }
        }

        repos.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(repos)
    }

    /// Check if a user repo with the given name exists.
    pub fn exists(&self, name: &str) -> bool {
        self.repo_path(name).exists()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_manager() -> (RepoManager, TempDir) {
        let dir = TempDir::new().unwrap();
        let manager = RepoManager::new(dir.path());
        (manager, dir)
    }

    #[test]
    fn test_add_and_list() {
        let (mgr, _dir) = test_manager();

        mgr.add("test-repo", "https://example.com/repo/os/x86_64")
            .unwrap();

        let repos = mgr.list().unwrap();
        assert_eq!(repos.len(), 1);
        assert_eq!(repos[0].name, "test-repo");
        assert_eq!(repos[0].server[0], "https://example.com/repo/os/x86_64");
    }

    #[test]
    fn test_add_duplicate_fails() {
        let (mgr, _dir) = test_manager();

        mgr.add("dup", "https://example.com/a").unwrap();
        let result = mgr.add("dup", "https://example.com/b");
        assert!(result.is_err());
    }

    #[test]
    fn test_remove() {
        let (mgr, _dir) = test_manager();

        mgr.add("removeme", "https://example.com/repo").unwrap();
        assert!(mgr.exists("removeme"));

        mgr.remove("removeme").unwrap();
        assert!(!mgr.exists("removeme"));
    }

    #[test]
    fn test_remove_missing_fails() {
        let (mgr, _dir) = test_manager();
        let result = mgr.remove("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_list_empty() {
        let (mgr, _dir) = test_manager();
        let repos = mgr.list().unwrap();
        assert!(repos.is_empty());
    }

    #[test]
    fn test_list_nonexistent_dir() {
        let mgr = RepoManager::new(Path::new("/tmp/xpm_test_nonexistent_dir_12345"));
        let repos = mgr.list().unwrap();
        assert!(repos.is_empty());
    }

    #[test]
    fn test_multiple_repos_sorted() {
        let (mgr, _dir) = test_manager();

        mgr.add("zebra", "https://example.com/z").unwrap();
        mgr.add("alpha", "https://example.com/a").unwrap();
        mgr.add("middle", "https://example.com/m").unwrap();

        let repos = mgr.list().unwrap();
        assert_eq!(repos.len(), 3);
        assert_eq!(repos[0].name, "alpha");
        assert_eq!(repos[1].name, "middle");
        assert_eq!(repos[2].name, "zebra");
    }
}
