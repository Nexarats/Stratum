//! Git context provider — branches, tags, staged files, remotes.

use crate::suggestions::provider::ContextProvider;
use crate::suggestions::types::CompletionItem;
use std::path::Path;
use std::process::Command;

/// Provides Git-specific completions (branches, files, etc.).
pub struct GitProvider {
    /// Cached branch list (cleared on each call for freshness).
    _cache: (),
}

impl GitProvider {
    pub fn new() -> Self {
        Self { _cache: () }
    }

    /// Get local branches.
    fn get_branches(cwd: &Path) -> Vec<String> {
        let output = Command::new("git")
            .args(["branch", "--format=%(refname:short)"])
            .current_dir(cwd)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Get remote branches.
    fn get_remote_branches(cwd: &Path) -> Vec<String> {
        let output = Command::new("git")
            .args(["branch", "-r", "--format=%(refname:short)"])
            .current_dir(cwd)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty() && !l.contains("HEAD"))
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Get unstaged/modified files.
    fn get_modified_files(cwd: &Path) -> Vec<String> {
        let output = Command::new("git")
            .args(["diff", "--name-only"])
            .current_dir(cwd)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Get untracked files.
    fn get_untracked_files(cwd: &Path) -> Vec<String> {
        let output = Command::new("git")
            .args(["ls-files", "--others", "--exclude-standard"])
            .current_dir(cwd)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Get tags.
    fn get_tags(cwd: &Path) -> Vec<String> {
        let output = Command::new("git")
            .args(["tag", "--list"])
            .current_dir(cwd)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    /// Get remotes.
    fn get_remotes(cwd: &Path) -> Vec<String> {
        let output = Command::new("git")
            .args(["remote"])
            .current_dir(cwd)
            .output();

        match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .map(|l| l.trim().to_string())
                    .filter(|l| !l.is_empty())
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}

impl ContextProvider for GitProvider {
    fn name(&self) -> &str {
        "Git"
    }

    fn handles(&self) -> &[&str] {
        &["git"]
    }

    fn completions(
        &self,
        _command: &str,
        args: &[&str],
        partial: &str,
        cwd: &Path,
    ) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let partial_lower = partial.to_lowercase();

        // Determine the git subcommand
        let subcommand = args.first().copied().unwrap_or("");

        match subcommand {
            "" => {
                // Show git subcommands
                let subcommands = [
                    ("add", "Add file contents to the index"),
                    ("branch", "List, create, or delete branches"),
                    ("checkout", "Switch branches or restore files"),
                    ("commit", "Record changes to the repository"),
                    ("diff", "Show changes between commits"),
                    ("fetch", "Download objects and refs"),
                    ("log", "Show commit logs"),
                    ("merge", "Join development histories"),
                    ("pull", "Fetch and integrate remote changes"),
                    ("push", "Update remote refs"),
                    ("rebase", "Reapply commits on top of another base"),
                    ("reset", "Reset current HEAD to specified state"),
                    ("stash", "Stash changes in a dirty working directory"),
                    ("status", "Show the working tree status"),
                    ("switch", "Switch branches"),
                    ("tag", "Create, list, delete tags"),
                ];

                for (name, desc) in subcommands {
                    if partial.is_empty() || name.starts_with(&partial_lower) {
                        items.push(CompletionItem::subcommand(name, desc));
                    }
                }
            }
            "checkout" | "switch" | "merge" | "rebase" => {
                // Show branches
                for branch in Self::get_branches(cwd) {
                    if partial.is_empty() || branch.to_lowercase().starts_with(&partial_lower)
                        || branch.to_lowercase().contains(&partial_lower)
                    {
                        items.push(CompletionItem::branch(&branch));
                    }
                }
                // Show remote branches for checkout
                if subcommand == "checkout" {
                    for branch in Self::get_remote_branches(cwd) {
                        if partial.is_empty() || branch.to_lowercase().contains(&partial_lower) {
                            let mut item = CompletionItem::branch(&branch);
                            item.detail = Some("(remote)".into());
                            item.priority = 15;
                            items.push(item);
                        }
                    }
                }
                // Show tags
                for tag in Self::get_tags(cwd) {
                    if partial.is_empty() || tag.to_lowercase().starts_with(&partial_lower) {
                        let mut item = CompletionItem::branch(&tag);
                        item.kind = crate::suggestions::types::CompletionKind::Tag;
                        item.detail = Some("(tag)".into());
                        item.icon = '\u{1F3F7}'; // 🏷
                        items.push(item);
                    }
                }
            }
            "add" => {
                // Show modified + untracked files
                for file in Self::get_modified_files(cwd) {
                    if partial.is_empty() || file.to_lowercase().contains(&partial_lower) {
                        let mut item = CompletionItem::file(&file, Some("modified".into()));
                        item.icon = '\u{270F}'; // ✏
                        item.priority = 5;
                        items.push(item);
                    }
                }
                for file in Self::get_untracked_files(cwd) {
                    if partial.is_empty() || file.to_lowercase().contains(&partial_lower) {
                        let mut item = CompletionItem::file(&file, Some("untracked".into()));
                        item.icon = '?';
                        item.priority = 10;
                        items.push(item);
                    }
                }
            }
            "push" | "pull" | "fetch" => {
                // Show remotes
                if args.len() <= 1 || (args.len() == 2 && !partial.is_empty()) {
                    for remote in Self::get_remotes(cwd) {
                        if partial.is_empty() || remote.to_lowercase().starts_with(&partial_lower) {
                            items.push(CompletionItem::host(&remote, Some("(remote)".into())));
                        }
                    }
                } else {
                    // After remote, show branches
                    for branch in Self::get_branches(cwd) {
                        if partial.is_empty() || branch.to_lowercase().starts_with(&partial_lower) {
                            items.push(CompletionItem::branch(&branch));
                        }
                    }
                }
            }
            "branch" => {
                // Show existing branches (for -d, etc.)
                for branch in Self::get_branches(cwd) {
                    if partial.is_empty() || branch.to_lowercase().starts_with(&partial_lower) {
                        items.push(CompletionItem::branch(&branch));
                    }
                }
            }
            _ => {}
        }

        items.truncate(25);
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_provider_handles() {
        let provider = GitProvider::new();
        assert!(provider.can_handle("git"));
        assert!(!provider.can_handle("docker"));
    }

    #[test]
    fn test_git_subcommands() {
        let provider = GitProvider::new();
        let items = provider.completions("git", &[], "", Path::new("."));
        // Should return subcommands
        assert!(!items.is_empty());
        assert!(items.iter().any(|i| i.label == "commit"));
        assert!(items.iter().any(|i| i.label == "push"));
    }

    #[test]
    fn test_git_subcommand_filter() {
        let provider = GitProvider::new();
        let items = provider.completions("git", &[], "co", Path::new("."));
        assert!(items.iter().all(|i| i.label.starts_with("co")));
    }
}
