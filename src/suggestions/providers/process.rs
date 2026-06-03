//! Process context provider — running processes for kill/pkill.

use crate::suggestions::provider::ContextProvider;
use crate::suggestions::types::CompletionItem;
use std::path::Path;
use std::process::Command;

pub struct ProcessProvider;

impl ContextProvider for ProcessProvider {
    fn name(&self) -> &str { "Process" }
    fn handles(&self) -> &[&str] { &["kill", "pkill", "killall"] }

    fn completions(&self, _command: &str, _args: &[&str], partial: &str, _cwd: &Path) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let partial_lower = partial.to_lowercase();

        #[cfg(target_os = "windows")]
        {
            let output = Command::new("tasklist").args(["/FO", "CSV", "/NH"]).output();
            if let Ok(out) = output {
                if out.status.success() {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    for line in stdout.lines() {
                        let parts: Vec<&str> = line.split(',').collect();
                        if parts.len() >= 2 {
                            let name = parts[0].trim_matches('"');
                            let pid = parts[1].trim_matches('"');
                            if partial.is_empty() || name.to_lowercase().contains(&partial_lower)
                                || pid.contains(&partial_lower)
                            {
                                items.push(CompletionItem::process(name, pid));
                            }
                        }
                    }
                }
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            let output = Command::new("ps").args(["aux", "--no-headers"]).output()
                .or_else(|_| Command::new("ps").arg("aux").output());
            if let Ok(out) = output {
                if out.status.success() {
                    let stdout = String::from_utf8_lossy(&out.stdout);
                    for line in stdout.lines() {
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if parts.len() >= 11 {
                            let pid = parts[1];
                            let name = parts[10..].join(" ");
                            if partial.is_empty() || name.to_lowercase().contains(&partial_lower)
                                || pid.contains(&partial_lower)
                            {
                                items.push(CompletionItem::process(&name, pid));
                            }
                        }
                    }
                }
            }
        }

        // Deduplicate by name, keep first occurrence
        let mut seen = std::collections::HashSet::new();
        items.retain(|item| seen.insert(item.label.clone()));
        items.truncate(20);
        items
    }
}
