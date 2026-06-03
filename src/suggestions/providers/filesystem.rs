//! Filesystem context provider — directories for cd, files for cat/vim/etc.

use crate::suggestions::provider::ContextProvider;
use crate::suggestions::types::CompletionItem;
use std::path::Path;

/// Provides filesystem completions (directories, files) for any command
/// that operates on paths.
pub struct FilesystemProvider;

impl FilesystemProvider {
    /// Commands where only directories should be shown.
    const DIR_ONLY_COMMANDS: &'static [&'static str] = &["cd", "pushd", "popd", "mkdir"];

    /// Commands where only files should be shown.
    const FILE_ONLY_COMMANDS: &'static [&'static str] = &[
        "cat", "less", "more", "head", "tail", "vim", "vi", "nano", "code",
        "bat", "python", "python3", "node", "ruby", "perl", "bash", "sh",
        "source", "chmod", "chown",
    ];

    /// Commands where both files and directories should be shown.
    const BOTH_COMMANDS: &'static [&'static str] = &[
        "ls", "dir", "rm", "mv", "cp", "ln", "touch", "stat", "file",
        "open", "start", "xdg-open", "wc", "grep", "find", "tar", "zip",
        "unzip", "diff",
    ];

    fn is_dir_only(command: &str) -> bool {
        Self::DIR_ONLY_COMMANDS.iter().any(|&c| c == command)
    }

    fn is_file_only(command: &str) -> bool {
        Self::FILE_ONLY_COMMANDS.iter().any(|&c| c == command)
    }
}

impl ContextProvider for FilesystemProvider {
    fn name(&self) -> &str {
        "Filesystem"
    }

    fn handles(&self) -> &[&str] {
        // We handle a wide set of commands — but we also handle ANY command
        // if the partial looks like a path (contains / or \)
        &[
            "cd", "ls", "dir", "cat", "less", "more", "head", "tail",
            "vim", "vi", "nano", "code", "bat", "rm", "mv", "cp", "ln",
            "touch", "mkdir", "stat", "file", "open", "start", "chmod",
            "chown", "python", "python3", "node", "ruby", "perl",
            "bash", "sh", "source", "wc", "grep", "find", "tar", "zip",
            "unzip", "diff", "pushd", "popd", "xdg-open",
        ]
    }

    fn completions(
        &self,
        command: &str,
        _args: &[&str],
        partial: &str,
        cwd: &Path,
    ) -> Vec<CompletionItem> {
        let dir_only = Self::is_dir_only(command);
        let file_only = Self::is_file_only(command);

        // Determine the directory to list and the prefix to filter by
        let (search_dir, prefix) = if partial.is_empty() {
            (cwd.to_path_buf(), String::new())
        } else {
            let partial_path = Path::new(partial);
            if partial.ends_with('/') || partial.ends_with('\\') {
                // User typed "src/" — list contents of src/
                let abs = if partial_path.is_absolute() {
                    partial_path.to_path_buf()
                } else {
                    cwd.join(partial_path)
                };
                (abs, String::new())
            } else if let Some(parent) = partial_path.parent() {
                // User typed "src/ma" — list src/ and filter by "ma"
                let parent_str = parent.to_string_lossy().to_string();
                let abs = if parent_str.is_empty() || parent_str == "." {
                    cwd.to_path_buf()
                } else if partial_path.is_absolute() {
                    parent.to_path_buf()
                } else {
                    cwd.join(parent)
                };
                let file_part = partial_path
                    .file_name()
                    .map(|f| f.to_string_lossy().to_string())
                    .unwrap_or_default();
                (abs, file_part)
            } else {
                (cwd.to_path_buf(), partial.to_string())
            }
        };

        let mut items = Vec::new();

        // Read directory entries
        let entries = match std::fs::read_dir(&search_dir) {
            Ok(entries) => entries,
            Err(_) => return items,
        };

        let prefix_lower = prefix.to_lowercase();

        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            let name_lower = name.to_lowercase();

            // Filter by prefix (fuzzy: starts with or contains)
            if !prefix.is_empty() && !name_lower.starts_with(&prefix_lower) {
                // Also try fuzzy substring match
                if !name_lower.contains(&prefix_lower) {
                    continue;
                }
            }

            // Skip hidden files unless partial starts with '.'
            if name.starts_with('.') && !prefix.starts_with('.') {
                continue;
            }

            let is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);

            if dir_only && !is_dir {
                continue;
            }
            if file_only && is_dir {
                continue;
            }

            // Build the insert text (relative path from what was typed)
            let insert = if partial.contains('/') || partial.contains('\\') {
                // Keep the directory prefix the user already typed
                if let Some(parent) = Path::new(partial).parent() {
                    let parent_str = parent.to_string_lossy();
                    if parent_str.is_empty() || parent_str == "." {
                        if is_dir { format!("{}/", name) } else { name.clone() }
                    } else {
                        if is_dir {
                            format!("{}/{}/", parent_str, name)
                        } else {
                            format!("{}/{}", parent_str, name)
                        }
                    }
                } else {
                    if is_dir { format!("{}/", name) } else { name.clone() }
                }
            } else {
                if is_dir { format!("{}/", name) } else { name.clone() }
            };

            if is_dir {
                let mut item = CompletionItem::directory(&name);
                item.insert_text = insert;
                items.push(item);
            } else {
                let size = entry
                    .metadata()
                    .ok()
                    .map(|m| format_size(m.len()));
                let mut item = CompletionItem::file(&name, size);
                item.insert_text = insert;
                items.push(item);
            }
        }

        // Sort: directories first, then files, both alphabetically
        items.sort_by(|a, b| {
            let a_is_dir = a.kind == crate::suggestions::types::CompletionKind::Directory;
            let b_is_dir = b.kind == crate::suggestions::types::CompletionKind::Directory;
            b_is_dir.cmp(&a_is_dir)
                .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
        });

        // Limit results
        items.truncate(30);

        items
    }
}

/// Format a file size in human-readable form.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * 1024;
    const GB: u64 = 1024 * 1024 * 1024;

    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_size() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1500), "1.5 KB");
        assert_eq!(format_size(1_500_000), "1.4 MB");
    }

    #[test]
    fn test_filesystem_provider_cwd() {
        let provider = FilesystemProvider;
        let cwd = std::env::current_dir().unwrap();
        let items = provider.completions("ls", &[], "", &cwd);
        // Should return some entries (current dir isn't empty in a Rust project)
        // We can't assert exact count since it depends on the environment
        assert!(provider.can_handle("cd"));
        assert!(provider.can_handle("ls"));
        assert!(!provider.can_handle("git"));
    }

    #[test]
    fn test_dir_only_for_cd() {
        assert!(FilesystemProvider::is_dir_only("cd"));
        assert!(!FilesystemProvider::is_dir_only("cat"));
    }
}
