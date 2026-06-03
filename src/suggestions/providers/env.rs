//! Environment variable context provider.

use crate::suggestions::provider::ContextProvider;
use crate::suggestions::types::CompletionItem;
use std::path::Path;

pub struct EnvProvider;

impl ContextProvider for EnvProvider {
    fn name(&self) -> &str { "Environment" }
    fn handles(&self) -> &[&str] { &["export", "env", "set", "printenv", "unset"] }

    fn completions(&self, command: &str, _args: &[&str], partial: &str, _cwd: &Path) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let partial_lower = partial.to_lowercase();
        // Strip leading $ if present
        let search = if partial_lower.starts_with('$') { &partial_lower[1..] } else { &partial_lower };

        for (key, value) in std::env::vars() {
            if search.is_empty() || key.to_lowercase().starts_with(search) || key.to_lowercase().contains(search) {
                let preview = if value.len() > 40 {
                    format!("{}...", &value[..37])
                } else {
                    value
                };
                let mut item = CompletionItem::env_var(&key, Some(preview));
                // For 'export' or 'set', insert KEY=
                if command == "export" || command == "set" {
                    item.insert_text = format!("{}=", key);
                }
                items.push(item);
            }
        }

        items.sort_by(|a, b| a.label.cmp(&b.label));
        items.truncate(25);
        items
    }
}
