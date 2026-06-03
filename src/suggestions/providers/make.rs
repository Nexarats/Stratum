//! Make context provider — targets from Makefile.

use crate::suggestions::provider::ContextProvider;
use crate::suggestions::types::CompletionItem;
use std::path::Path;

pub struct MakeProvider;

impl ContextProvider for MakeProvider {
    fn name(&self) -> &str { "Make" }
    fn handles(&self) -> &[&str] { &["make", "gmake"] }

    fn completions(&self, _command: &str, _args: &[&str], partial: &str, cwd: &Path) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let partial_lower = partial.to_lowercase();

        // Try Makefile, makefile, GNUmakefile
        let makefile_names = ["Makefile", "makefile", "GNUmakefile"];
        let mut content = None;
        for name in makefile_names {
            let path = cwd.join(name);
            if path.exists() {
                content = std::fs::read_to_string(&path).ok();
                break;
            }
        }

        if let Some(content) = content {
            for line in content.lines() {
                // Match lines like "target:" or "target: deps"
                // Skip lines starting with whitespace (recipe lines), comments, and variable assignments
                if line.starts_with('\t') || line.starts_with(' ') || line.starts_with('#') {
                    continue;
                }
                if let Some(colon_pos) = line.find(':') {
                    // Skip variable assignments (::=, :=)
                    if line[colon_pos..].starts_with("::=") || line[colon_pos..].starts_with(":=") {
                        continue;
                    }
                    let target = line[..colon_pos].trim();
                    // Skip .PHONY, .DEFAULT, etc.
                    if target.starts_with('.') || target.contains('%') || target.contains('$') {
                        continue;
                    }
                    // Handle multiple targets on same line
                    for t in target.split_whitespace() {
                        let t = t.trim();
                        if !t.is_empty() && (partial.is_empty() || t.to_lowercase().starts_with(&partial_lower)) {
                            items.push(CompletionItem::target(t, "Makefile"));
                        }
                    }
                }
            }
        }

        // Deduplicate
        let mut seen = std::collections::HashSet::new();
        items.retain(|item| seen.insert(item.label.clone()));
        items.truncate(20);
        items
    }
}
