//! NOS Shell context provider — built-in commands, namespaces, pipeline ops.

use crate::suggestions::provider::ContextProvider;
use crate::suggestions::types::CompletionItem;
use std::path::Path;

/// Provides completions for NOS Shell built-in commands and pipeline operations.
pub struct NosShellProvider;

impl ContextProvider for NosShellProvider {
    fn name(&self) -> &str { "NOS Shell" }

    fn handles(&self) -> &[&str] {
        &["files", "file", "system", "disk", "process", "filter", "sort",
          "count", "first", "last", "unique", "reverse", "map", "head", "tail"]
    }

    fn completions(&self, command: &str, args: &[&str], partial: &str, _cwd: &Path) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let partial_lower = partial.to_lowercase();

        match command {
            "files" | "file" => {
                let methods = [
                    ("list", "List files in directory", "files.list()"),
                    ("read", "Read file contents", "files.read(\"path\")"),
                    ("exists", "Check if path exists", "files.exists(\"path\")"),
                    ("info", "Get file metadata", "files.info(\"path\")"),
                ];
                for (name, desc, insert) in methods {
                    if partial.is_empty() || name.starts_with(&partial_lower) {
                        let mut item = CompletionItem::subcommand(name, desc);
                        item.insert_text = insert.to_string();
                        item.icon = '\u{1F4C1}'; // 📁
                        items.push(item);
                    }
                }
            }
            "system" => {
                let methods = [
                    ("info", "Show OS, user, architecture", "system.info()"),
                    ("health", "Shell version and health", "system.health()"),
                ];
                for (name, desc, insert) in methods {
                    if partial.is_empty() || name.starts_with(&partial_lower) {
                        let mut item = CompletionItem::subcommand(name, desc);
                        item.insert_text = insert.to_string();
                        item.icon = '\u{2699}'; // ⚙
                        items.push(item);
                    }
                }
            }
            "disk" => {
                if partial.is_empty() || "usage".starts_with(&partial_lower) {
                    let mut item = CompletionItem::subcommand("usage", "Show disk space per drive");
                    item.insert_text = "disk.usage()".to_string();
                    item.icon = '\u{1F4BE}'; // 💾
                    items.push(item);
                }
            }
            "process" => {
                if partial.is_empty() || "list".starts_with(&partial_lower) {
                    let mut item = CompletionItem::subcommand("list", "List running processes");
                    item.insert_text = "process.list()".to_string();
                    item.icon = '\u{1F534}'; // 🔴
                    items.push(item);
                }
            }
            // Pipeline operations — suggest next pipeline stage
            "filter" | "sort" | "count" | "first" | "last" | "unique" | "reverse" | "map" | "head" | "tail" => {
                if args.is_empty() {
                    // If used as a bare command (not in pipeline), suggest pipe syntax
                    let pipe_ops = [
                        ("filter", "Filter by field value"),
                        ("sort", "Sort by field"),
                        ("count", "Count items"),
                        ("first", "Get first N items"),
                        ("last", "Get last N items"),
                        ("unique", "Remove duplicates"),
                        ("reverse", "Reverse order"),
                        ("map", "Extract fields"),
                    ];
                    for (name, desc) in pipe_ops {
                        if name != command && (partial.is_empty() || name.starts_with(&partial_lower)) {
                            let mut item = CompletionItem::subcommand(name, desc);
                            item.insert_text = format!("| {}", name);
                            items.push(item);
                        }
                    }
                }
            }
            _ => {}
        }

        items
    }
}
