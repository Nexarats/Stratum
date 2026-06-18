//! Suggestion Engine — orchestrates static, dynamic, and AI suggestion layers.
//!
//! Takes the current input line, splits it into command + args + partial,
//! then queries:
//!   1. Static layer (inline_docs.rs — flags, synopsis)
//!   2. Dynamic layer (ContextProvider registry — fs, git, docker, etc.)
//!   3. AI layer (future: AI-powered predictions)
//!
//! Merges, deduplicates, and sorts the results into a single list.

use super::provider::ProviderRegistry;
use super::types::{CompletionItem, CompletionKind, SuggestionCardState};
use crate::features::inline_docs::DocOverlay;
use std::path::PathBuf;

/// The main suggestion engine — call `update()` on each keystroke.
pub struct SuggestionEngine {
    /// Dynamic context providers.
    pub providers: ProviderRegistry,
    /// Current working directory (updated from pane).
    pub cwd: PathBuf,
    /// The suggestion card state (shared with renderer).
    pub card: SuggestionCardState,
    /// Debounce: minimum chars before showing suggestions.
    pub min_chars: usize,
}

impl SuggestionEngine {
    pub fn new() -> Self {
        Self {
            providers: ProviderRegistry::with_defaults(),
            cwd: std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
            card: SuggestionCardState::new(),
            min_chars: 1,
        }
    }

    /// Update the suggestion card based on current input.
    ///
    /// Called from the event loop whenever the input tracker reports dirty.
    pub fn update(&mut self, input: &str, doc_overlay: &DocOverlay) {
        let input_trimmed = input.trim();

        // Empty input → hide
        if input_trimmed.is_empty() || input_trimmed.len() < self.min_chars {
            self.card.hide();
            return;
        }

        // Parse input into command + args + partial
        let parts: Vec<&str> = input_trimmed.split_whitespace().collect();
        if parts.is_empty() {
            self.card.hide();
            return;
        }

        let command = parts[0];
        let args = if parts.len() > 1 { &parts[1..] } else { &[] };
        let partial = if parts.len() > 1 && !input.ends_with(' ') {
            // User is in the middle of typing an argument
            parts.last().copied().unwrap_or("")
        } else if input.ends_with(' ') {
            // User pressed space — show all suggestions for next arg
            ""
        } else {
            // User is typing the command itself — no args
            ""
        };

        let is_typing_command = parts.len() == 1 && !input.ends_with(' ') && command != "cd";

        let mut items = Vec::new();

        // ─── Layer 1: Static (inline docs flags) ───
        if let Some(overlay) = doc_overlay.get_overlay(input_trimmed) {
            // Add flags from inline docs
            for flag in &overlay.relevant_flags {
                let flag_text = flag.long.clone()
                    .or_else(|| flag.short.clone())
                    .unwrap_or_default();
                if !flag_text.is_empty() {
                    items.push(CompletionItem::flag(&flag_text, &flag.description));
                }
            }

            // Add flag completions
            for completion in &overlay.completions {
                if !items.iter().any(|i| i.label == *completion) {
                    items.push(CompletionItem::flag(completion, ""));
                }
            }
        }

        // ─── Layer 2: Dynamic (context providers) ───
        if is_typing_command {
            // User is typing the command name — suggest matching commands
            let known_commands = self.get_known_commands();
            let partial_lower = command.to_lowercase();
            for (name, desc) in known_commands {
                if name.starts_with(&partial_lower) || name.contains(&partial_lower) {
                    items.push(CompletionItem::subcommand(name, desc));
                }
            }
        } else {
            // User is typing an argument — get context-aware completions
            let dynamic_items = self.providers.get_completions(
                command, args, partial, &self.cwd,
            );
            items.extend(dynamic_items);
        }

        // ─── Layer 3: AI (future) ───
        // TODO: Add AI-powered suggestions here

        // ─── Filter by partial ───
        if !partial.is_empty() && !is_typing_command {
            let partial_lower = partial.to_lowercase();
            items.retain(|item| {
                let label_lower = item.label.to_lowercase();
                label_lower.starts_with(&partial_lower)
                    || label_lower.contains(&partial_lower)
                    || fuzzy_match(&label_lower, &partial_lower)
            });
        }

        // ─── Sort ───
        items.sort_by(|a, b| {
            // Exact prefix matches first
            let a_prefix = a.label.to_lowercase().starts_with(
                &partial.to_lowercase()
            );
            let b_prefix = b.label.to_lowercase().starts_with(
                &partial.to_lowercase()
            );
            b_prefix.cmp(&a_prefix)
                .then_with(|| a.priority.cmp(&b.priority))
                .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
        });

        // ─── Deduplicate ───
        let mut seen = std::collections::HashSet::new();
        items.retain(|item| seen.insert(item.label.clone()));

        // ─── Limit ───
        items.truncate(30);

        // ─── Update card ───
        let filter = if is_typing_command { command.to_string() } else { partial.to_string() };
        if items.is_empty() {
            self.card.hide();
        } else {
            self.card.show(command.to_string(), filter, items);
        }
    }

    /// Set the current working directory.
    pub fn set_cwd(&mut self, cwd: PathBuf) {
        self.cwd = cwd;
    }

    /// Get a list of known commands for command-name completion.
    fn get_known_commands(&self) -> Vec<(&str, &str)> {
        vec![
            // Core Unix/Linux
            ("ls", "List directory contents"),
            ("cd", "Change directory"),
            ("pwd", "Print working directory"),
            ("cat", "Concatenate and display files"),
            ("echo", "Display a line of text"),
            ("mkdir", "Create directories"),
            ("rm", "Remove files or directories"),
            ("cp", "Copy files and directories"),
            ("mv", "Move/rename files"),
            ("touch", "Create/update file timestamps"),
            ("chmod", "Change file permissions"),
            ("chown", "Change file owner"),
            ("find", "Search for files"),
            ("grep", "Search text patterns"),
            ("head", "Show first lines of file"),
            ("tail", "Show last lines of file"),
            ("less", "Page through file"),
            ("more", "Page through file"),
            ("wc", "Count lines/words/bytes"),
            ("sort", "Sort lines"),
            ("uniq", "Filter duplicate lines"),
            ("diff", "Compare files"),
            ("tar", "Archive files"),
            ("zip", "Compress files"),
            ("unzip", "Extract zip archives"),
            ("curl", "Transfer data with URLs"),
            ("wget", "Download files"),
            ("ssh", "Secure shell"),
            ("scp", "Secure copy"),
            ("rsync", "Remote sync"),
            ("ps", "List processes"),
            ("kill", "Send signal to process"),
            ("top", "System monitor"),
            ("htop", "Interactive process viewer"),
            ("df", "Disk free space"),
            ("du", "Disk usage"),
            ("free", "Memory usage"),
            ("man", "Manual pages"),
            ("which", "Locate a command"),
            ("whoami", "Current user"),
            ("uname", "System information"),
            ("clear", "Clear terminal"),
            ("history", "Command history"),
            ("env", "Environment variables"),
            ("export", "Set environment variable"),
            ("alias", "Create command alias"),
            ("ln", "Create links"),
            ("stat", "File status"),
            ("file", "Determine file type"),
            ("xargs", "Build commands from input"),
            ("awk", "Pattern scanning"),
            ("sed", "Stream editor"),
            ("tee", "Read from stdin, write to files"),
            // Dev tools
            ("git", "Version control"),
            ("docker", "Container management"),
            ("npm", "Node package manager"),
            ("yarn", "JavaScript package manager"),
            ("pnpm", "Fast Node package manager"),
            ("bun", "Fast JavaScript runtime"),
            ("cargo", "Rust package manager"),
            ("python", "Python interpreter"),
            ("python3", "Python 3 interpreter"),
            ("node", "Node.js runtime"),
            ("go", "Go tools"),
            ("make", "Build tool"),
            ("cmake", "Cross-platform build"),
            ("pip", "Python packages"),
            ("pip3", "Python 3 packages"),
            ("rustc", "Rust compiler"),
            ("rustup", "Rust toolchain manager"),
            ("gcc", "GNU C compiler"),
            ("g++", "GNU C++ compiler"),
            ("java", "Java runtime"),
            ("javac", "Java compiler"),
            ("mvn", "Maven build tool"),
            ("gradle", "Gradle build tool"),
            // System / Windows
            ("systemctl", "Manage services"),
            ("journalctl", "View systemd logs"),
            ("apt", "Debian package manager"),
            ("brew", "Homebrew package manager"),
            ("pacman", "Arch package manager"),
            ("yum", "RPM package manager"),
            ("dnf", "Fedora package manager"),
            ("snap", "Snap package manager"),
            ("flatpak", "Flatpak package manager"),
            // kubectl/k8s
            ("kubectl", "Kubernetes CLI"),
            ("helm", "Kubernetes package manager"),
            // NOS Shell
            ("files.list", "NOS: List files"),
            ("files.read", "NOS: Read file"),
            ("system.info", "NOS: System info"),
            ("process.list", "NOS: List processes"),
            ("disk.usage", "NOS: Disk usage"),
        ]
    }
}

impl Default for SuggestionEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple fuzzy matching — checks if all chars of pattern appear in text in order.
fn fuzzy_match(text: &str, pattern: &str) -> bool {
    let mut text_chars = text.chars();
    for pat_char in pattern.chars() {
        let found = loop {
            match text_chars.next() {
                Some(tc) if tc == pat_char => break true,
                Some(_) => continue,
                None => break false,
            }
        };
        if !found {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fuzzy_match() {
        assert!(fuzzy_match("filesystem", "fsm"));
        assert!(fuzzy_match("docker", "dkr"));
        assert!(fuzzy_match("hello", "hlo"));
        assert!(!fuzzy_match("hello", "xyz"));
        assert!(fuzzy_match("abc", "abc"));
        assert!(fuzzy_match("anything", ""));
    }

    #[test]
    fn test_engine_basic() {
        let engine = SuggestionEngine::new();
        assert!(!engine.card.visible);
    }

    #[test]
    fn test_engine_known_commands() {
        let engine = SuggestionEngine::new();
        let commands = engine.get_known_commands();
        assert!(commands.iter().any(|(n, _)| *n == "git"));
        assert!(commands.iter().any(|(n, _)| *n == "docker"));
        assert!(commands.iter().any(|(n, _)| *n == "cd"));
    }
}
