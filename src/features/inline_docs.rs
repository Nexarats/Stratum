//! Inline Documentation Overlay — shows docs while the user types.
//!
//! As the user types a command, Stratum shows a non-intrusive overlay
//! with the command's synopsis, available flags, descriptions, and
//! the user's own command history for that base command.

use std::collections::HashMap;

/// A command's documentation entry.
#[derive(Debug, Clone)]
pub struct CommandDoc {
    /// Command name.
    pub name: String,
    /// One-line synopsis.
    pub synopsis: String,
    /// Short description.
    pub description: String,
    /// Available flags/options.
    pub flags: Vec<FlagDoc>,
    /// Usage examples.
    pub examples: Vec<String>,
}

/// Documentation for a single flag.
#[derive(Debug, Clone)]
pub struct FlagDoc {
    /// Short flag (e.g., "-l").
    pub short: Option<String>,
    /// Long flag (e.g., "--long").
    pub long: Option<String>,
    /// Description.
    pub description: String,
    /// Whether it takes an argument.
    pub takes_arg: bool,
}

/// The overlay content to display while the user is typing.
#[derive(Debug, Clone)]
pub struct OverlayContent {
    /// Command name being documented.
    pub command: String,
    /// Synopsis line.
    pub synopsis: Option<String>,
    /// Relevant flags for the current input context.
    pub relevant_flags: Vec<FlagDoc>,
    /// User's history entries for this command (most recent first).
    pub history_entries: Vec<String>,
    /// Completion suggestions.
    pub completions: Vec<String>,
}

/// Provides inline documentation as the user types.
pub struct DocOverlay {
    /// Built-in command documentation.
    docs: HashMap<String, CommandDoc>,
    /// User's command history (base_command → full commands, newest first).
    history: HashMap<String, Vec<String>>,
    /// Maximum history entries per command.
    max_history: usize,
}

impl DocOverlay {
    /// Create a new doc overlay with built-in command docs.
    pub fn new() -> Self {
        let mut docs = HashMap::new();

        // Register built-in docs for common commands
        Self::register_builtins(&mut docs);

        Self {
            docs,
            history: HashMap::new(),
            max_history: 50,
        }
    }

    /// Get overlay content for the current input line.
    pub fn get_overlay(&self, input: &str) -> Option<OverlayContent> {
        let input = input.trim();
        if input.is_empty() {
            return None;
        }

        let parts: Vec<&str> = input.split_whitespace().collect();
        let base_cmd = parts[0].rsplit('/').next().unwrap_or(parts[0]);

        let doc = self.docs.get(base_cmd)?;

        // Find flags relevant to what's being typed
        let current_arg = parts.last().unwrap_or(&"");
        let relevant_flags = if current_arg.starts_with('-') {
            doc.flags
                .iter()
                .filter(|f| {
                    f.short
                        .as_ref()
                        .is_some_and(|s| s.starts_with(current_arg))
                        || f.long
                            .as_ref()
                            .is_some_and(|l| l.starts_with(current_arg))
                })
                .cloned()
                .collect()
        } else if parts.len() <= 1 {
            // Show top flags when no args typed yet
            doc.flags.iter().take(5).cloned().collect()
        } else {
            vec![]
        };

        // Get user history for this command
        let history_entries = self
            .history
            .get(base_cmd)
            .map(|h| h.iter().take(3).cloned().collect())
            .unwrap_or_default();

        // Generate completions for current arg
        let completions = if current_arg.starts_with('-') {
            doc.flags
                .iter()
                .filter_map(|f| {
                    if let Some(ref long) = f.long {
                        if long.starts_with(current_arg) && long != current_arg {
                            return Some(long.clone());
                        }
                    }
                    if let Some(ref short) = f.short {
                        if short.starts_with(current_arg) && short != current_arg {
                            return Some(short.clone());
                        }
                    }
                    None
                })
                .take(5)
                .collect()
        } else {
            vec![]
        };

        Some(OverlayContent {
            command: base_cmd.to_string(),
            synopsis: Some(doc.synopsis.clone()),
            relevant_flags,
            history_entries,
            completions,
        })
    }

    /// Record a command in history for future doc overlay context.
    pub fn record_command(&mut self, command: &str) {
        let command = command.trim();
        if command.is_empty() {
            return;
        }

        let base_cmd = command
            .split_whitespace()
            .next()
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("");

        if base_cmd.is_empty() {
            return;
        }

        let history = self
            .history
            .entry(base_cmd.to_string())
            .or_insert_with(Vec::new);

        // Avoid duplicates — remove existing and re-add at front
        history.retain(|h| h != command);
        history.insert(0, command.to_string());

        if history.len() > self.max_history {
            history.truncate(self.max_history);
        }
    }

    /// Register a custom command doc.
    pub fn register_doc(&mut self, doc: CommandDoc) {
        self.docs.insert(doc.name.clone(), doc);
    }

    /// Register built-in documentation for common Linux commands.
    fn register_builtins(docs: &mut HashMap<String, CommandDoc>) {
        docs.insert("ls".to_string(), CommandDoc {
            name: "ls".to_string(),
            synopsis: "ls [OPTION]... [FILE]...".to_string(),
            description: "List directory contents".to_string(),
            flags: vec![
                FlagDoc { short: Some("-l".into()), long: Some("--long".into()), description: "Long listing format".into(), takes_arg: false },
                FlagDoc { short: Some("-a".into()), long: Some("--all".into()), description: "Show hidden files".into(), takes_arg: false },
                FlagDoc { short: Some("-h".into()), long: Some("--human-readable".into()), description: "Human-readable sizes".into(), takes_arg: false },
                FlagDoc { short: Some("-R".into()), long: Some("--recursive".into()), description: "List recursively".into(), takes_arg: false },
                FlagDoc { short: Some("-S".into()), long: None, description: "Sort by file size".into(), takes_arg: false },
                FlagDoc { short: Some("-t".into()), long: None, description: "Sort by modification time".into(), takes_arg: false },
            ],
            examples: vec!["ls -la".into(), "ls -lhS".into(), "ls -R src/".into()],
        });

        docs.insert("git".to_string(), CommandDoc {
            name: "git".to_string(),
            synopsis: "git <command> [<args>]".to_string(),
            description: "Distributed version control system".to_string(),
            flags: vec![
                FlagDoc { short: None, long: Some("status".into()), description: "Show working tree status".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("add".into()), description: "Stage files for commit".into(), takes_arg: true },
                FlagDoc { short: None, long: Some("commit".into()), description: "Record changes to repository".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("push".into()), description: "Push to remote repository".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("pull".into()), description: "Fetch and merge from remote".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("log".into()), description: "Show commit history".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("diff".into()), description: "Show changes between commits".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("branch".into()), description: "List, create, or delete branches".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("checkout".into()), description: "Switch branches or restore files".into(), takes_arg: true },
                FlagDoc { short: None, long: Some("stash".into()), description: "Stash uncommitted changes".into(), takes_arg: false },
            ],
            examples: vec!["git status".into(), "git add -A".into(), "git commit -m 'message'".into(), "git push origin main".into()],
        });

        docs.insert("grep".to_string(), CommandDoc {
            name: "grep".to_string(),
            synopsis: "grep [OPTION]... PATTERN [FILE]...".to_string(),
            description: "Search files for lines matching a pattern".to_string(),
            flags: vec![
                FlagDoc { short: Some("-r".into()), long: Some("--recursive".into()), description: "Search directories recursively".into(), takes_arg: false },
                FlagDoc { short: Some("-i".into()), long: Some("--ignore-case".into()), description: "Case-insensitive search".into(), takes_arg: false },
                FlagDoc { short: Some("-n".into()), long: Some("--line-number".into()), description: "Print line numbers".into(), takes_arg: false },
                FlagDoc { short: Some("-l".into()), long: Some("--files-with-matches".into()), description: "Print only filenames".into(), takes_arg: false },
                FlagDoc { short: Some("-c".into()), long: Some("--count".into()), description: "Print match count per file".into(), takes_arg: false },
                FlagDoc { short: Some("-v".into()), long: Some("--invert-match".into()), description: "Invert match (non-matching lines)".into(), takes_arg: false },
                FlagDoc { short: Some("-E".into()), long: Some("--extended-regexp".into()), description: "Extended regex".into(), takes_arg: false },
            ],
            examples: vec!["grep -rn 'TODO' src/".into(), "grep -i error /var/log/syslog".into()],
        });

        docs.insert("find".to_string(), CommandDoc {
            name: "find".to_string(),
            synopsis: "find [path...] [expression]".to_string(),
            description: "Search for files in a directory hierarchy".to_string(),
            flags: vec![
                FlagDoc { short: None, long: Some("-name".into()), description: "Match by filename pattern".into(), takes_arg: true },
                FlagDoc { short: None, long: Some("-type".into()), description: "File type (f=file, d=dir)".into(), takes_arg: true },
                FlagDoc { short: None, long: Some("-size".into()), description: "File size (+10M, -1k)".into(), takes_arg: true },
                FlagDoc { short: None, long: Some("-mtime".into()), description: "Days since modification".into(), takes_arg: true },
                FlagDoc { short: None, long: Some("-exec".into()), description: "Execute command on results".into(), takes_arg: true },
                FlagDoc { short: None, long: Some("-delete".into()), description: "Delete matching files".into(), takes_arg: false },
            ],
            examples: vec!["find . -name '*.rs'".into(), "find /tmp -type f -mtime +7 -delete".into()],
        });

        for (name, synopsis, description) in &[
            ("cd", "cd [DIR]", "Change the current directory"),
            ("pwd", "pwd", "Print working directory"),
            ("cat", "cat [FILE]...", "Concatenate and print files"),
            ("echo", "echo [STRING]...", "Display text"),
            ("cp", "cp [OPTION]... SOURCE... DEST", "Copy files and directories"),
            ("mv", "mv [OPTION]... SOURCE... DEST", "Move or rename files"),
            ("rm", "rm [OPTION]... FILE...", "Remove files or directories"),
            ("mkdir", "mkdir [OPTION]... DIR...", "Create directories"),
            ("chmod", "chmod [OPTION]... MODE FILE...", "Change file permissions"),
            ("chown", "chown [OPTION]... OWNER FILE...", "Change file owner"),
            ("head", "head [OPTION]... [FILE]...", "Output first part of files"),
            ("tail", "tail [OPTION]... [FILE]...", "Output last part of files"),
            ("wc", "wc [OPTION]... [FILE]...", "Print line, word, byte counts"),
            ("sort", "sort [OPTION]... [FILE]...", "Sort lines of text"),
            ("uniq", "uniq [OPTION]... [INPUT [OUTPUT]]", "Report or omit repeated lines"),
            ("curl", "curl [options] [URL...]", "Transfer data from URLs"),
            ("wget", "wget [OPTION]... [URL]...", "Download files from the web"),
            ("tar", "tar [OPTION]... [FILE]...", "Archive utility"),
            ("ssh", "ssh [user@]host [command]", "Secure shell remote login"),
            ("docker", "docker [OPTIONS] COMMAND", "Container management platform"),
            ("cargo", "cargo [OPTIONS] [COMMAND]", "Rust package manager"),
            ("npm", "npm <command> [args]", "Node.js package manager"),
            ("python", "python [option] [script] [arg]...", "Python interpreter"),
            ("pip", "pip <command> [options]", "Python package manager"),
        ] {
            docs.insert(name.to_string(), CommandDoc {
                name: name.to_string(),
                synopsis: synopsis.to_string(),
                description: description.to_string(),
                flags: vec![],
                examples: vec![],
            });
        }

        // --- Windows / PowerShell commands ---
        docs.insert("dir".to_string(), CommandDoc {
            name: "dir".to_string(),
            synopsis: "dir [path] [/options]".to_string(),
            description: "List directory contents (Windows)".to_string(),
            flags: vec![
                FlagDoc { short: None, long: Some("/a".into()), description: "Show all files including hidden".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("/s".into()), description: "List recursively".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("/b".into()), description: "Bare format (names only)".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("/o".into()), description: "Sort (N=name, S=size, D=date)".into(), takes_arg: true },
            ],
            examples: vec!["dir /a /s".into(), "dir *.txt".into()],
        });

        docs.insert("Get-ChildItem".to_string(), CommandDoc {
            name: "Get-ChildItem".to_string(),
            synopsis: "Get-ChildItem [-Path] <string> [-Filter] [-Recurse]".to_string(),
            description: "List items in a location (PowerShell)".to_string(),
            flags: vec![
                FlagDoc { short: None, long: Some("-Recurse".into()), description: "List recursively".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("-Filter".into()), description: "Filter by name pattern".into(), takes_arg: true },
                FlagDoc { short: None, long: Some("-Force".into()), description: "Include hidden items".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("-Name".into()), description: "Output names only".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("-File".into()), description: "Files only".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("-Directory".into()), description: "Directories only".into(), takes_arg: false },
            ],
            examples: vec!["Get-ChildItem -Recurse -Filter *.rs".into(), "gci -Force".into()],
        });

        docs.insert("Get-Process".to_string(), CommandDoc {
            name: "Get-Process".to_string(),
            synopsis: "Get-Process [[-Name] <string[]>] [-Id <int[]>]".to_string(),
            description: "List running processes".to_string(),
            flags: vec![
                FlagDoc { short: None, long: Some("-Name".into()), description: "Filter by process name".into(), takes_arg: true },
                FlagDoc { short: None, long: Some("-Id".into()), description: "Filter by process ID".into(), takes_arg: true },
            ],
            examples: vec!["Get-Process explorer".into(), "gps | Sort-Object CPU".into()],
        });

        docs.insert("Get-Content".to_string(), CommandDoc {
            name: "Get-Content".to_string(),
            synopsis: "Get-Content [-Path] <string[]> [-TotalCount <int>] [-Tail <int>]".to_string(),
            description: "Read file contents".to_string(),
            flags: vec![
                FlagDoc { short: None, long: Some("-TotalCount".into()), description: "Read first N lines".into(), takes_arg: true },
                FlagDoc { short: None, long: Some("-Tail".into()), description: "Read last N lines".into(), takes_arg: true },
                FlagDoc { short: None, long: Some("-Raw".into()), description: "Read as single string".into(), takes_arg: false },
            ],
            examples: vec!["Get-Content log.txt -Tail 20".into(), "gc file.txt | Select-String pattern".into()],
        });

        docs.insert("Set-Location".to_string(), CommandDoc {
            name: "Set-Location".to_string(),
            synopsis: "Set-Location [-Path] <string>".to_string(),
            description: "Change current directory (cd)".to_string(),
            flags: vec![
                FlagDoc { short: None, long: Some("-Path".into()), description: "Target directory".into(), takes_arg: true },
            ],
            examples: vec!["Set-Location C:\\Users".into(), "cd ..".into()],
        });

        docs.insert("Select-String".to_string(), CommandDoc {
            name: "Select-String".to_string(),
            synopsis: "Select-String -Pattern <string> [-Path <string[]>]".to_string(),
            description: "Find text in strings and files (grep)".to_string(),
            flags: vec![
                FlagDoc { short: None, long: Some("-Pattern".into()), description: "Regex pattern to search".into(), takes_arg: true },
                FlagDoc { short: None, long: Some("-Path".into()), description: "Files to search".into(), takes_arg: true },
                FlagDoc { short: None, long: Some("-CaseSensitive".into()), description: "Case-sensitive match".into(), takes_arg: false },
                FlagDoc { short: None, long: Some("-Recurse".into()), description: "Search recursively".into(), takes_arg: false },
            ],
            examples: vec!["Select-String -Pattern 'error' -Path *.log".into()],
        });

        // Windows CMD + PowerShell aliases
        for (name, synopsis, description) in &[
            ("cls", "cls", "Clear the terminal screen"),
            ("type", "type <FILE>", "Display file contents"),
            ("copy", "copy <SOURCE> <DEST>", "Copy files"),
            ("move", "move <SOURCE> <DEST>", "Move files"),
            ("del", "del <FILE>", "Delete files"),
            ("md", "md <DIR>", "Create directory"),
            ("rd", "rd <DIR>", "Remove directory"),
            ("ren", "ren <OLD> <NEW>", "Rename a file"),
            ("tree", "tree [path] [/F] [/A]", "Display directory tree"),
            ("ipconfig", "ipconfig [/all] [/release] [/renew]", "Display network config"),
            ("tasklist", "tasklist [/FI filter]", "List running processes"),
            ("taskkill", "taskkill /PID <pid> [/F]", "Kill a process"),
            ("netstat", "netstat [-a] [-n] [-o]", "Network connections"),
            ("where", "where <COMMAND>", "Locate a command (like which)"),
            ("systeminfo", "systeminfo", "Display system information"),
            ("sc", "sc <command> <service>", "Service control manager"),
            ("sfc", "sfc /scannow", "System file checker"),
            ("gci", "gci [path]", "Alias for Get-ChildItem"),
            ("gps", "gps [name]", "Alias for Get-Process"),
            ("gc", "gc [path]", "Alias for Get-Content"),
            ("sl", "sl <path>", "Alias for Set-Location"),
            ("sls", "sls -Pattern <p>", "Alias for Select-String"),
            ("iwr", "iwr <url>", "Alias for Invoke-WebRequest"),
            ("ni", "ni <path>", "Alias for New-Item"),
            ("ri", "ri <path>", "Alias for Remove-Item"),
            ("rustup", "rustup [command]", "Rust toolchain manager"),
            ("code", "code [path]", "Open VS Code"),
            ("node", "node [script]", "Node.js runtime"),
        ] {
            docs.insert(name.to_string(), CommandDoc {
                name: name.to_string(),
                synopsis: synopsis.to_string(),
                description: description.to_string(),
                flags: vec![],
                examples: vec![],
            });
        }
    }
}

impl Default for DocOverlay {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_overlay() {
        let overlay = DocOverlay::new();
        let content = overlay.get_overlay("ls").unwrap();
        assert_eq!(content.command, "ls");
        assert!(content.synopsis.is_some());
        assert!(!content.relevant_flags.is_empty());
    }

    #[test]
    fn test_flag_filtering() {
        let overlay = DocOverlay::new();
        let content = overlay.get_overlay("ls --rec").unwrap();
        assert!(content.relevant_flags.iter().any(|f| f.long.as_deref() == Some("--recursive")));
    }

    #[test]
    fn test_unknown_command() {
        let overlay = DocOverlay::new();
        assert!(overlay.get_overlay("myunknowncmd").is_none());
    }

    #[test]
    fn test_history() {
        let mut overlay = DocOverlay::new();
        overlay.record_command("ls -la /home");
        overlay.record_command("ls -R src/");

        let content = overlay.get_overlay("ls").unwrap();
        assert_eq!(content.history_entries.len(), 2);
        assert_eq!(content.history_entries[0], "ls -R src/");
    }

    #[test]
    fn test_completions() {
        let overlay = DocOverlay::new();
        let content = overlay.get_overlay("grep --rec").unwrap();
        assert!(content.completions.iter().any(|c| c == "--recursive"));
    }
}
