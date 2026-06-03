//! History context provider — suggests from past commands.

use crate::suggestions::provider::ContextProvider;
use crate::suggestions::types::CompletionItem;
use std::path::Path;

/// Provides history-based suggestions for ANY command.
/// This provider has a special wildcard handle — it matches all commands.
pub struct HistoryProvider {
    /// All past commands (newest first).
    history: Vec<String>,
}

impl HistoryProvider {
    pub fn new() -> Self {
        Self { history: Vec::new() }
    }

    /// Record a command into history.
    pub fn record(&mut self, command: impl Into<String>) {
        let cmd = command.into();
        if cmd.trim().is_empty() { return; }
        // Remove duplicates (move to front)
        self.history.retain(|h| h != &cmd);
        self.history.insert(0, cmd);
        // Cap at 500 entries
        self.history.truncate(500);
    }

    /// Get all history entries.
    pub fn entries(&self) -> &[String] {
        &self.history
    }
}

impl ContextProvider for HistoryProvider {
    fn name(&self) -> &str { "History" }

    /// Wildcard: handles ALL commands via fuzzy history matching.
    fn handles(&self) -> &[&str] { &[] }

    /// Override can_handle to always return true — history works for everything.
    fn can_handle(&self, _command: &str) -> bool {
        true
    }

    fn completions(&self, command: &str, _args: &[&str], partial: &str, _cwd: &Path) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let full_input = if partial.is_empty() {
            command.to_lowercase()
        } else {
            format!("{} {}", command, partial).to_lowercase()
        };

        for entry in &self.history {
            let entry_lower = entry.to_lowercase();
            // Match if entry starts with the command or contains the full input
            if entry_lower.starts_with(&command.to_lowercase()) && entry_lower.contains(&full_input) {
                // Don't suggest the exact same thing being typed
                if entry_lower == full_input {
                    continue;
                }
                items.push(CompletionItem::history(entry));
            }
        }

        items.truncate(5); // Don't overwhelm with history
        items
    }
}

impl Default for HistoryProvider {
    fn default() -> Self { Self::new() }
}
