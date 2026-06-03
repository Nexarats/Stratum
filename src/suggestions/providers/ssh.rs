//! SSH context provider — hosts from ~/.ssh/config.

use crate::suggestions::provider::ContextProvider;
use crate::suggestions::types::CompletionItem;
use std::path::Path;

pub struct SshProvider;

impl ContextProvider for SshProvider {
    fn name(&self) -> &str { "SSH" }
    fn handles(&self) -> &[&str] { &["ssh", "scp", "sftp", "rsync"] }

    fn completions(&self, _command: &str, _args: &[&str], partial: &str, _cwd: &Path) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let partial_lower = partial.to_lowercase();

        // Parse ~/.ssh/config
        let home = std::env::var("HOME")
            .or_else(|_| std::env::var("USERPROFILE"))
            .unwrap_or_default();
        let config_path = Path::new(&home).join(".ssh").join("config");

        if let Ok(content) = std::fs::read_to_string(&config_path) {
            let mut current_host: Option<String> = None;
            let mut current_hostname: Option<String> = None;

            for line in content.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') { continue; }

                if let Some(host) = trimmed.strip_prefix("Host ").or_else(|| trimmed.strip_prefix("Host\t")) {
                    // Flush previous host
                    if let Some(ref h) = current_host {
                        if !h.contains('*') {
                            if partial.is_empty() || h.to_lowercase().contains(&partial_lower) {
                                items.push(CompletionItem::host(h, current_hostname.clone()));
                            }
                        }
                    }
                    current_host = Some(host.trim().to_string());
                    current_hostname = None;
                } else if let Some(hostname) = trimmed.strip_prefix("HostName ").or_else(|| trimmed.strip_prefix("Hostname ")) {
                    current_hostname = Some(hostname.trim().to_string());
                }
            }
            // Flush last host
            if let Some(ref h) = current_host {
                if !h.contains('*') {
                    if partial.is_empty() || h.to_lowercase().contains(&partial_lower) {
                        items.push(CompletionItem::host(h, current_hostname));
                    }
                }
            }
        }

        items.truncate(20);
        items
    }
}
