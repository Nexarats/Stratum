//! NPM context provider — scripts from package.json, installed packages.

use crate::suggestions::provider::ContextProvider;
use crate::suggestions::types::CompletionItem;
use std::path::Path;

pub struct NpmProvider;

impl NpmProvider {
    /// Read scripts from package.json in the given directory.
    fn get_scripts(cwd: &Path) -> Vec<(String, String)> {
        let pkg_path = cwd.join("package.json");
        if !pkg_path.exists() {
            return Vec::new();
        }
        let content = match std::fs::read_to_string(&pkg_path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        // Simple JSON parser for "scripts" section
        Self::parse_scripts_from_json(&content)
    }

    fn parse_scripts_from_json(json: &str) -> Vec<(String, String)> {
        // Find "scripts" key and extract key-value pairs
        let mut scripts = Vec::new();
        if let Some(scripts_start) = json.find("\"scripts\"") {
            let rest = &json[scripts_start..];
            if let Some(brace_start) = rest.find('{') {
                let inner = &rest[brace_start + 1..];
                let mut depth = 1;
                let mut end = 0;
                for (i, ch) in inner.char_indices() {
                    match ch {
                        '{' => depth += 1,
                        '}' => {
                            depth -= 1;
                            if depth == 0 { end = i; break; }
                        }
                        _ => {}
                    }
                }
                let scripts_block = &inner[..end];
                // Parse "key": "value" pairs
                let mut chars = scripts_block.chars().peekable();
                loop {
                    // Skip to next "
                    while chars.peek().is_some() && *chars.peek().unwrap() != '"' {
                        chars.next();
                    }
                    if chars.peek().is_none() { break; }
                    chars.next(); // consume opening "
                    let mut key = String::new();
                    while let Some(&ch) = chars.peek() {
                        if ch == '"' { chars.next(); break; }
                        key.push(ch); chars.next();
                    }
                    // Skip to next "
                    while chars.peek().is_some() && *chars.peek().unwrap() != '"' {
                        chars.next();
                    }
                    if chars.peek().is_none() { break; }
                    chars.next(); // consume opening "
                    let mut val = String::new();
                    while let Some(&ch) = chars.peek() {
                        if ch == '"' { chars.next(); break; }
                        if ch == '\\' { chars.next(); if let Some(&esc) = chars.peek() { val.push(esc); chars.next(); continue; } }
                        val.push(ch); chars.next();
                    }
                    if !key.is_empty() {
                        scripts.push((key, val));
                    }
                }
            }
        }
        scripts
    }
}

impl ContextProvider for NpmProvider {
    fn name(&self) -> &str { "NPM" }
    fn handles(&self) -> &[&str] { &["npm", "yarn", "pnpm", "bun"] }

    fn completions(&self, _command: &str, args: &[&str], partial: &str, cwd: &Path) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let partial_lower = partial.to_lowercase();
        let subcommand = args.first().copied().unwrap_or("");

        match subcommand {
            "" => {
                let subs = [
                    ("install", "Install packages"), ("run", "Run a script"),
                    ("test", "Run tests"), ("start", "Start the app"),
                    ("build", "Build the project"), ("init", "Create package.json"),
                    ("uninstall", "Remove a package"), ("update", "Update packages"),
                    ("list", "List installed packages"), ("audit", "Security audit"),
                ];
                for (name, desc) in subs {
                    if partial.is_empty() || name.starts_with(&partial_lower) {
                        items.push(CompletionItem::subcommand(name, desc));
                    }
                }
            }
            "run" | "run-script" => {
                for (name, cmd) in Self::get_scripts(cwd) {
                    if partial.is_empty() || name.to_lowercase().contains(&partial_lower) {
                        items.push(CompletionItem::script(&name, cmd));
                    }
                }
            }
            "install" | "i" | "add" => {
                // Show flags for install
                let flags = [
                    ("--save-dev", "Save as dev dependency"),
                    ("--save-exact", "Save exact version"),
                    ("--global", "Install globally"),
                    ("--production", "Skip dev dependencies"),
                ];
                for (flag, desc) in flags {
                    if partial.is_empty() || flag.starts_with(&partial_lower) {
                        items.push(CompletionItem::flag(flag, desc));
                    }
                }
            }
            _ => {}
        }
        items.truncate(20);
        items
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_scripts() {
        let json = r#"{"name":"test","scripts":{"dev":"vite","build":"tsc && vite build","test":"vitest"}}"#;
        let scripts = NpmProvider::parse_scripts_from_json(json);
        assert_eq!(scripts.len(), 3);
        assert!(scripts.iter().any(|(k, _)| k == "dev"));
        assert!(scripts.iter().any(|(k, _)| k == "build"));
    }
}
