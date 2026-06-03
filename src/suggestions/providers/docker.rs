//! Docker context provider — containers, images, networks.

use crate::suggestions::provider::ContextProvider;
use crate::suggestions::types::CompletionItem;
use std::path::Path;
use std::process::Command;

pub struct DockerProvider {
    _cache: (),
}

impl DockerProvider {
    pub fn new() -> Self { Self { _cache: () } }

    fn get_containers(cwd: &Path, all: bool) -> Vec<(String, String)> {
        let mut args = vec!["ps", "--format", "{{.Names}}|{{.Status}}"];
        if all { args.push("-a"); }
        let output = Command::new("docker").args(&args).current_dir(cwd).output();
        match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter_map(|l| {
                        let parts: Vec<&str> = l.splitn(2, '|').collect();
                        if parts.len() == 2 {
                            Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
                        } else { None }
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }

    fn get_images(cwd: &Path) -> Vec<(String, String)> {
        let output = Command::new("docker")
            .args(["images", "--format", "{{.Repository}}:{{.Tag}}|{{.Size}}"])
            .current_dir(cwd).output();
        match output {
            Ok(out) if out.status.success() => {
                String::from_utf8_lossy(&out.stdout)
                    .lines()
                    .filter_map(|l| {
                        let parts: Vec<&str> = l.splitn(2, '|').collect();
                        if parts.len() == 2 && !parts[0].contains("<none>") {
                            Some((parts[0].trim().to_string(), parts[1].trim().to_string()))
                        } else { None }
                    })
                    .collect()
            }
            _ => Vec::new(),
        }
    }
}

impl ContextProvider for DockerProvider {
    fn name(&self) -> &str { "Docker" }
    fn handles(&self) -> &[&str] { &["docker", "docker-compose"] }

    fn completions(&self, _command: &str, args: &[&str], partial: &str, cwd: &Path) -> Vec<CompletionItem> {
        let mut items = Vec::new();
        let partial_lower = partial.to_lowercase();
        let subcommand = args.first().copied().unwrap_or("");

        match subcommand {
            "" => {
                let subs = [
                    ("run", "Create and run a container"), ("build", "Build an image"),
                    ("ps", "List containers"), ("exec", "Execute in container"),
                    ("stop", "Stop containers"), ("rm", "Remove containers"),
                    ("rmi", "Remove images"), ("pull", "Pull an image"),
                    ("push", "Push an image"), ("images", "List images"),
                    ("logs", "View container logs"), ("compose", "Docker Compose"),
                ];
                for (name, desc) in subs {
                    if partial.is_empty() || name.starts_with(&partial_lower) {
                        items.push(CompletionItem::subcommand(name, desc));
                    }
                }
            }
            "exec" | "stop" | "rm" | "logs" | "restart" | "start" | "attach" => {
                for (name, status) in Self::get_containers(cwd, subcommand == "start" || subcommand == "rm") {
                    if partial.is_empty() || name.to_lowercase().contains(&partial_lower) {
                        items.push(CompletionItem::container(&name, &status));
                    }
                }
            }
            "run" | "pull" => {
                for (name, size) in Self::get_images(cwd) {
                    if partial.is_empty() || name.to_lowercase().contains(&partial_lower) {
                        items.push(CompletionItem::image(&name, Some(size)));
                    }
                }
            }
            "rmi" => {
                for (name, size) in Self::get_images(cwd) {
                    if partial.is_empty() || name.to_lowercase().contains(&partial_lower) {
                        items.push(CompletionItem::image(&name, Some(size)));
                    }
                }
            }
            _ => {}
        }
        items.truncate(20);
        items
    }
}
