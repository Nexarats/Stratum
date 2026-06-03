//! Cargo context provider — targets, features, subcommands.

use crate::suggestions::provider::ContextProvider;
use crate::suggestions::types::CompletionItem;
use std::path::Path;

pub struct CargoProvider;

impl ContextProvider for CargoProvider {
    fn name(&self) -> &str { "Cargo" }
    fn handles(&self) -> &[&str] { &["cargo"] }

    fn completions(&self, _command: &str, args: &[&str], partial: &str, cwd: &Path) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let partial_lower = partial.to_lowercase();
        let subcommand = args.first().copied().unwrap_or("");

        match subcommand {
            "" => {
                let subs = [
                    ("build", "Compile the current package"), ("run", "Run a binary"),
                    ("test", "Run tests"), ("check", "Check for errors"),
                    ("clippy", "Lint with Clippy"), ("fmt", "Format code"),
                    ("bench", "Run benchmarks"), ("doc", "Build documentation"),
                    ("clean", "Remove target directory"), ("update", "Update dependencies"),
                    ("add", "Add a dependency"), ("remove", "Remove a dependency"),
                    ("publish", "Publish to crates.io"), ("install", "Install a binary"),
                    ("init", "Create a new package"), ("new", "Create a new package"),
                ];
                for (name, desc) in subs {
                    if partial.is_empty() || name.starts_with(&partial_lower) {
                        items.push(CompletionItem::subcommand(name, desc));
                    }
                }
            }
            "build" | "run" | "test" | "check" | "bench" => {
                // Show common flags
                let flags = [
                    ("--release", "Build in release mode"),
                    ("--target", "Target triple"),
                    ("--features", "Enable features"),
                    ("--all-features", "Enable all features"),
                    ("--no-default-features", "Disable default features"),
                    ("--jobs", "Number of parallel jobs"),
                    ("--verbose", "Use verbose output"),
                ];
                for (flag, desc) in flags {
                    if partial.is_empty() || flag.starts_with(&partial_lower) {
                        items.push(CompletionItem::flag(flag, desc));
                    }
                }
                // Try to get workspace members / bin targets from Cargo.toml
                if let Some(targets) = Self::get_targets(cwd) {
                    for target in targets {
                        if partial.is_empty() || target.to_lowercase().contains(&partial_lower) {
                            items.push(CompletionItem::target(&target, "Cargo.toml"));
                        }
                    }
                }
            }
            _ => {}
        }
        items.truncate(20);
        items
    }
}

impl CargoProvider {
    fn get_targets(cwd: &Path) -> Option<Vec<String>> {
        let cargo_toml = cwd.join("Cargo.toml");
        if !cargo_toml.exists() { return None; }
        let content = std::fs::read_to_string(&cargo_toml).ok()?;
        let mut targets = Vec::new();
        // Simple parse: look for [[bin]] name = "..." or [package] name = "..."
        for line in content.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("name") && trimmed.contains('=') {
                if let Some(val) = trimmed.split('=').nth(1) {
                    let name = val.trim().trim_matches('"').trim_matches('\'');
                    if !name.is_empty() {
                        targets.push(name.to_string());
                    }
                }
            }
        }
        if targets.is_empty() { None } else { Some(targets) }
    }
}
