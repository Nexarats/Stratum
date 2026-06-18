//! Persistent long-term memory for the Stratum AI Agent.
//!
//! Stores user requests, executed commands, and extracted environment preferences
//! in `~/.stratum/memory.json`.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::{Context, Result};
use super::provider::AiProvider;
use super::chat::ChatEngine;

/// Represents a single recorded developer interaction.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InteractionRecord {
    pub timestamp: String,
    pub directory: String,
    pub user_request: String,
    pub executed_commands: Vec<String>,
}

/// Persistent memory storage.
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LongTermMemory {
    /// Extracted environmental preferences and developer settings/facts.
    pub learnings: Vec<String>,
    /// History of past interactions.
    pub interactions: Vec<InteractionRecord>,
}

impl LongTermMemory {
    /// Load memory from disk, or fallback to an empty default.
    pub fn load() -> Self {
        let path = Self::memory_path();
        if path.exists() {
            if let Ok(content) = std::fs::read_to_string(&path) {
                if let Ok(memory) = serde_json::from_str::<LongTermMemory>(&content) {
                    return memory;
                }
            }
        }
        Self::default()
    }

    /// Save current memory back to disk.
    pub fn save(&self) -> Result<()> {
        let path = Self::memory_path();
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create memory directory")?;
        }
        let content = serde_json::to_string_pretty(self)
            .context("Failed to serialize memory")?;
        std::fs::write(path, content)
            .context("Failed to write memory file")?;
        Ok(())
    }

    /// Add an interaction record to memory.
    pub fn add_interaction(&mut self, directory: String, user_request: String, executed_commands: Vec<String>) {
        let timestamp = if let Ok(dur) = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH) {
            dur.as_secs().to_string()
        } else {
            "unknown".to_string()
        };

        self.interactions.push(InteractionRecord {
            timestamp,
            directory,
            user_request,
            executed_commands,
        });

        // Limit interactions to keep files small (keep last 30)
        if self.interactions.len() > 30 {
            self.interactions.remove(0);
        }

        let _ = self.save();
    }

    /// Call AI provider asynchronously to extract learnings from the current interaction context.
    pub async fn extract_learnings(
        &mut self,
        provider: &AiProvider,
        user_request: &str,
        executed_commands: &[String],
        output: &str,
    ) -> Result<()> {
        let system_prompt = "You are Stratum Memory Processor. Extract key developer facts, configurations, paths, project types, or preferences from the developer's request, actions, and output to persist in long-term memory. Return a JSON list of strings, for example: [\"Project in E:/Vscodeprojects/nexarats/NOS is a Rust workspace\", \"User has git ssh access configured\"]. Return ONLY a JSON list of strings, and nothing else.";
        
        let prompt = format!(
            "User Request: {}\nExecuted Commands: {:?}\nOutput: {}\n\nExtract learnings:",
            user_request,
            executed_commands,
            output
        );

        if let Ok(response) = ChatEngine::query(provider, system_prompt, &prompt).await {
            let content = response.content.trim();
            let json_str = if let Some(start) = content.find('[') {
                if let Some(end) = content.rfind(']') {
                    &content[start..=end]
                } else {
                    content
                }
            } else {
                content
            };

            if let Ok(new_learns) = serde_json::from_str::<Vec<String>>(json_str) {
                for learn in new_learns {
                    let clean_learn = learn.trim().to_string();
                    if !clean_learn.is_empty() && !self.learnings.contains(&clean_learn) {
                        self.learnings.push(clean_learn);
                    }
                }
                // Limit learnings list (keep last 50)
                if self.learnings.len() > 50 {
                    self.learnings.remove(0);
                }
                let _ = self.save();
            }
        }
        Ok(())
    }

    /// Determine file path to memory storage.
    fn memory_path() -> PathBuf {
        if let Some(home) = dirs::home_dir() {
            home.join(".stratum").join("memory.json")
        } else if let Some(config) = dirs::config_dir() {
            config.join("stratum").join("memory.json")
        } else {
            PathBuf::from("stratum-memory.json")
        }
    }
}
