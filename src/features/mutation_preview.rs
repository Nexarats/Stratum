//! Live Mutation Preview — simulates filesystem changes before execution.
//!
//! Before any filesystem-mutating command runs, Stratum builds a shadow
//! view of what will change using copy-on-write semantics. The user sees
//! a diff of additions, deletions, and modifications before committing.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A single predicted filesystem change.
#[derive(Debug, Clone, PartialEq)]
pub enum FsChange {
    /// File or directory will be created.
    Create {
        path: PathBuf,
        is_dir: bool,
    },
    /// File or directory will be deleted.
    Delete {
        path: PathBuf,
        is_dir: bool,
        size: Option<u64>,
    },
    /// File will be modified (content change).
    Modify {
        path: PathBuf,
        old_size: Option<u64>,
        new_size: Option<u64>,
    },
    /// File or directory will be moved/renamed.
    Move {
        from: PathBuf,
        to: PathBuf,
    },
    /// Permissions will change.
    Chmod {
        path: PathBuf,
        old_mode: Option<String>,
        new_mode: String,
    },
    /// Ownership will change.
    Chown {
        path: PathBuf,
        new_owner: String,
    },
}

impl FsChange {
    /// Get the primary path affected by this change.
    pub fn primary_path(&self) -> &Path {
        match self {
            FsChange::Create { path, .. } => path,
            FsChange::Delete { path, .. } => path,
            FsChange::Modify { path, .. } => path,
            FsChange::Move { from, .. } => from,
            FsChange::Chmod { path, .. } => path,
            FsChange::Chown { path, .. } => path,
        }
    }

    /// Human-readable description of the change.
    pub fn description(&self) -> String {
        match self {
            FsChange::Create { path, is_dir } => {
                let kind = if *is_dir { "directory" } else { "file" };
                format!("CREATE {}: {}", kind, path.display())
            }
            FsChange::Delete { path, is_dir, size } => {
                let kind = if *is_dir { "directory" } else { "file" };
                let size_str = size
                    .map(|s| format!(" ({})", format_size(s)))
                    .unwrap_or_default();
                format!("DELETE {}: {}{}", kind, path.display(), size_str)
            }
            FsChange::Modify { path, old_size, new_size } => {
                let delta = match (old_size, new_size) {
                    (Some(o), Some(n)) => {
                        let diff = *n as i64 - *o as i64;
                        if diff >= 0 {
                            format!(" (+{})", format_size(diff as u64))
                        } else {
                            format!(" (-{})", format_size((-diff) as u64))
                        }
                    }
                    _ => String::new(),
                };
                format!("MODIFY: {}{}", path.display(), delta)
            }
            FsChange::Move { from, to } => {
                format!("MOVE: {} → {}", from.display(), to.display())
            }
            FsChange::Chmod { path, new_mode, .. } => {
                format!("CHMOD: {} → {}", path.display(), new_mode)
            }
            FsChange::Chown { path, new_owner } => {
                format!("CHOWN: {} → {}", path.display(), new_owner)
            }
        }
    }

    /// Is this change destructive (data loss)?
    pub fn is_destructive(&self) -> bool {
        matches!(self, FsChange::Delete { .. })
    }
}

/// Summary of all predicted changes for a command.
#[derive(Debug, Clone)]
pub struct MutationPreview {
    /// The command being previewed.
    pub command: String,
    /// All predicted changes, ordered by path.
    pub changes: Vec<FsChange>,
    /// Whether the command was fully analyzed.
    pub fully_analyzed: bool,
    /// Warning messages from the analysis.
    pub warnings: Vec<String>,
}

impl MutationPreview {
    /// Count of each change type.
    pub fn summary(&self) -> MutationSummary {
        let mut summary = MutationSummary::default();
        for change in &self.changes {
            match change {
                FsChange::Create { .. } => summary.creates += 1,
                FsChange::Delete { size, .. } => {
                    summary.deletes += 1;
                    if let Some(s) = size {
                        summary.bytes_deleted += s;
                    }
                }
                FsChange::Modify { .. } => summary.modifies += 1,
                FsChange::Move { .. } => summary.moves += 1,
                FsChange::Chmod { .. } => summary.chmods += 1,
                FsChange::Chown { .. } => summary.chowns += 1,
            }
        }
        summary.has_destructive = self.changes.iter().any(|c| c.is_destructive());
        summary
    }
}

/// Aggregated counts for a mutation preview.
#[derive(Debug, Clone, Default)]
pub struct MutationSummary {
    pub creates: usize,
    pub deletes: usize,
    pub modifies: usize,
    pub moves: usize,
    pub chmods: usize,
    pub chowns: usize,
    pub bytes_deleted: u64,
    pub has_destructive: bool,
}

impl MutationSummary {
    /// One-line summary string.
    pub fn one_line(&self) -> String {
        let mut parts = Vec::new();
        if self.creates > 0 {
            parts.push(format!("+{} created", self.creates));
        }
        if self.deletes > 0 {
            parts.push(format!("-{} deleted", self.deletes));
        }
        if self.modifies > 0 {
            parts.push(format!("~{} modified", self.modifies));
        }
        if self.moves > 0 {
            parts.push(format!("→{} moved", self.moves));
        }

        if parts.is_empty() {
            "No filesystem changes detected".to_string()
        } else {
            parts.join(", ")
        }
    }
}

/// Analyzes commands to predict filesystem mutations.
pub struct MutationAnalyzer {
    /// Known command patterns and their mutation behaviors.
    patterns: Vec<MutationPattern>,
}

struct MutationPattern {
    /// Command prefix to match.
    prefix: String,
    /// Function to analyze arguments and predict changes.
    analyzer: fn(&str, &[String]) -> Vec<FsChange>,
}

impl MutationAnalyzer {
    /// Create a new analyzer with built-in command knowledge.
    pub fn new() -> Self {
        Self {
            patterns: vec![
                MutationPattern {
                    prefix: "rm".to_string(),
                    analyzer: analyze_rm,
                },
                MutationPattern {
                    prefix: "mv".to_string(),
                    analyzer: analyze_mv,
                },
                MutationPattern {
                    prefix: "cp".to_string(),
                    analyzer: analyze_cp,
                },
                MutationPattern {
                    prefix: "mkdir".to_string(),
                    analyzer: analyze_mkdir,
                },
                MutationPattern {
                    prefix: "touch".to_string(),
                    analyzer: analyze_touch,
                },
                MutationPattern {
                    prefix: "chmod".to_string(),
                    analyzer: analyze_chmod,
                },
                MutationPattern {
                    prefix: "chown".to_string(),
                    analyzer: analyze_chown,
                },
            ],
        }
    }

    /// Analyze a command and predict its filesystem mutations.
    pub fn analyze(&self, command: &str) -> MutationPreview {
        let parts: Vec<String> = shell_split(command);
        if parts.is_empty() {
            return MutationPreview {
                command: command.to_string(),
                changes: vec![],
                fully_analyzed: true,
                warnings: vec![],
            };
        }

        let base_cmd = parts[0]
            .rsplit('/')
            .next()
            .unwrap_or(&parts[0])
            .to_string();
        let args: Vec<String> = parts[1..].to_vec();

        let mut changes = Vec::new();
        let mut warnings = Vec::new();
        let mut fully_analyzed = false;

        for pattern in &self.patterns {
            if base_cmd == pattern.prefix {
                changes = (pattern.analyzer)(&base_cmd, &args);
                fully_analyzed = true;
                break;
            }
        }

        if !fully_analyzed {
            // Check for output redirection (> or >>)
            if command.contains(" > ") {
                if let Some(target) = command.split(" > ").nth(1) {
                    let target = target.trim().split_whitespace().next().unwrap_or("").trim();
                    if !target.is_empty() {
                        changes.push(FsChange::Create {
                            path: PathBuf::from(target),
                            is_dir: false,
                        });
                        fully_analyzed = true;
                    }
                }
            }
        }

        if !fully_analyzed && !changes.is_empty() {
            warnings.push(
                "Partial analysis — some changes may not be detected.".to_string(),
            );
        }

        MutationPreview {
            command: command.to_string(),
            changes,
            fully_analyzed,
            warnings,
        }
    }

    /// Returns true if the command is known to mutate the filesystem.
    pub fn is_mutating(&self, command: &str) -> bool {
        let parts: Vec<String> = shell_split(command);
        if parts.is_empty() {
            return false;
        }

        let base_cmd = parts[0].rsplit('/').next().unwrap_or(&parts[0]);

        self.patterns.iter().any(|p| p.prefix == base_cmd)
            || command.contains(" > ")
            || command.contains(" >> ")
    }
}

impl Default for MutationAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

// --- Command-specific analyzers ---

fn analyze_rm(_cmd: &str, args: &[String]) -> Vec<FsChange> {
    let mut changes = Vec::new();
    let recursive = args.iter().any(|a| a.contains('r') && a.starts_with('-'));

    for arg in args {
        if arg.starts_with('-') {
            continue;
        }
        changes.push(FsChange::Delete {
            path: PathBuf::from(arg),
            is_dir: recursive,
            size: None, // Would need filesystem access to determine
        });
    }
    changes
}

fn analyze_mv(_cmd: &str, args: &[String]) -> Vec<FsChange> {
    let non_flag: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    if non_flag.len() >= 2 {
        let dest = non_flag.last().unwrap();
        for src in &non_flag[..non_flag.len() - 1] {
            return vec![FsChange::Move {
                from: PathBuf::from(src),
                to: PathBuf::from(dest.as_str()),
            }];
        }
    }
    vec![]
}

fn analyze_cp(_cmd: &str, args: &[String]) -> Vec<FsChange> {
    let non_flag: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    if non_flag.len() >= 2 {
        let dest = non_flag.last().unwrap();
        return vec![FsChange::Create {
            path: PathBuf::from(dest.as_str()),
            is_dir: args.iter().any(|a| a.contains('r') && a.starts_with('-')),
        }];
    }
    vec![]
}

fn analyze_mkdir(_cmd: &str, args: &[String]) -> Vec<FsChange> {
    args.iter()
        .filter(|a| !a.starts_with('-'))
        .map(|a| FsChange::Create {
            path: PathBuf::from(a),
            is_dir: true,
        })
        .collect()
}

fn analyze_touch(_cmd: &str, args: &[String]) -> Vec<FsChange> {
    args.iter()
        .filter(|a| !a.starts_with('-'))
        .map(|a| FsChange::Create {
            path: PathBuf::from(a),
            is_dir: false,
        })
        .collect()
}

fn analyze_chmod(_cmd: &str, args: &[String]) -> Vec<FsChange> {
    let non_flag: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    if non_flag.len() >= 2 {
        let mode = non_flag[0].to_string();
        return non_flag[1..]
            .iter()
            .map(|p| FsChange::Chmod {
                path: PathBuf::from(p.as_str()),
                old_mode: None,
                new_mode: mode.clone(),
            })
            .collect();
    }
    vec![]
}

fn analyze_chown(_cmd: &str, args: &[String]) -> Vec<FsChange> {
    let non_flag: Vec<&String> = args.iter().filter(|a| !a.starts_with('-')).collect();
    if non_flag.len() >= 2 {
        let owner = non_flag[0].to_string();
        return non_flag[1..]
            .iter()
            .map(|p| FsChange::Chown {
                path: PathBuf::from(p.as_str()),
                new_owner: owner.clone(),
            })
            .collect();
    }
    vec![]
}

/// Basic shell argument splitting.
fn shell_split(command: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escape_next = false;

    for ch in command.chars() {
        if escape_next {
            current.push(ch);
            escape_next = false;
            continue;
        }

        match ch {
            '\\' if !in_single_quote => escape_next = true,
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            ' ' | '\t' if !in_single_quote && !in_double_quote => {
                if !current.is_empty() {
                    result.push(current.clone());
                    current.clear();
                }
            }
            _ => current.push(ch),
        }
    }

    if !current.is_empty() {
        result.push(current);
    }

    result
}

/// Format a byte count as human-readable.
fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

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
    fn test_rm_analysis() {
        let analyzer = MutationAnalyzer::new();
        let preview = analyzer.analyze("rm -rf /tmp/old_files");

        assert_eq!(preview.changes.len(), 1);
        assert!(matches!(&preview.changes[0], FsChange::Delete { path, is_dir: true, .. } if path == Path::new("/tmp/old_files")));
        assert!(preview.changes[0].is_destructive());
    }

    #[test]
    fn test_mv_analysis() {
        let analyzer = MutationAnalyzer::new();
        let preview = analyzer.analyze("mv src/old.rs src/new.rs");

        assert_eq!(preview.changes.len(), 1);
        assert!(matches!(&preview.changes[0], FsChange::Move { from, to } if from == Path::new("src/old.rs") && to == Path::new("src/new.rs")));
    }

    #[test]
    fn test_mkdir_analysis() {
        let analyzer = MutationAnalyzer::new();
        let preview = analyzer.analyze("mkdir -p src/new_module tests/integration");

        assert_eq!(preview.changes.len(), 2);
        assert!(matches!(&preview.changes[0], FsChange::Create { is_dir: true, .. }));
    }

    #[test]
    fn test_mutation_summary() {
        let preview = MutationPreview {
            command: "test".to_string(),
            changes: vec![
                FsChange::Create { path: PathBuf::from("a"), is_dir: false },
                FsChange::Delete { path: PathBuf::from("b"), is_dir: false, size: Some(1024) },
                FsChange::Modify { path: PathBuf::from("c"), old_size: Some(100), new_size: Some(200) },
            ],
            fully_analyzed: true,
            warnings: vec![],
        };

        let summary = preview.summary();
        assert_eq!(summary.creates, 1);
        assert_eq!(summary.deletes, 1);
        assert_eq!(summary.modifies, 1);
        assert!(summary.has_destructive);
        assert_eq!(summary.one_line(), "+1 created, -1 deleted, ~1 modified");
    }

    #[test]
    fn test_non_mutating() {
        let analyzer = MutationAnalyzer::new();
        assert!(!analyzer.is_mutating("ls -la"));
        assert!(!analyzer.is_mutating("cat file.txt"));
        assert!(analyzer.is_mutating("rm file.txt"));
        assert!(analyzer.is_mutating("echo hello > file.txt"));
    }

    #[test]
    fn test_shell_split() {
        assert_eq!(shell_split("ls -la"), vec!["ls", "-la"]);
        assert_eq!(shell_split("echo 'hello world'"), vec!["echo", "hello world"]);
        assert_eq!(
            shell_split(r#"echo "hello world""#),
            vec!["echo", "hello world"]
        );
    }
}
