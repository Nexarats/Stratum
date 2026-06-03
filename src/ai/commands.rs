//! Inline AI commands — /ask, /explain, /suggest, /translate.
//!
//! These commands are intercepted from terminal input before being sent
//! to the PTY. They trigger AI queries and render responses as overlays.

use super::provider::{AiProvider, AiResponse};
use super::chat::ChatEngine;
use super::registry::ProviderRegistry;
use super::credentials::CredentialStore;
use anyhow::{Context, Result};

/// Parsed AI command from user input.
#[derive(Debug, Clone)]
pub enum AiCommand {
    /// `/ask "question"` — ask the AI a question.
    Ask(String),
    /// `/explain` — explain the last error or output.
    Explain(Option<String>),
    /// `/suggest` — suggest the next command based on context.
    Suggest,
    /// `/translate <command>` — translate command between platforms.
    Translate(String),
    /// `/ai-config` — show current AI configuration.
    Config,
    /// `/ai-providers` — list all available providers.
    Providers,
    /// `/ai-set-key <provider> <key>` — set an API key.
    SetKey { provider: String, key: String },
    /// `/ai-models <provider>` — list models for a provider.
    Models(String),
    /// `/ai-test` — test the current AI connection.
    Test,
    /// `/clear-chat` — clear AI conversation history.
    ClearChat,
}

impl AiCommand {
    /// Try to parse an AI command from a command string.
    /// Returns None if it's not an AI command (pass to shell).
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        if !trimmed.starts_with('/') {
            return None;
        }

        let parts: Vec<&str> = trimmed.splitn(3, ' ').collect();
        let cmd = parts[0].to_lowercase();

        match cmd.as_str() {
            "/ask" => {
                let query = parts.get(1..).map(|p| p.join(" ")).unwrap_or_default();
                if query.is_empty() {
                    None // Not enough args
                } else {
                    Some(Self::Ask(query))
                }
            }
            "/explain" => {
                let context = if parts.len() > 1 {
                    Some(parts[1..].join(" "))
                } else {
                    None
                };
                Some(Self::Explain(context))
            }
            "/suggest" => Some(Self::Suggest),
            "/translate" => {
                let cmd_text = parts.get(1..).map(|p| p.join(" ")).unwrap_or_default();
                if cmd_text.is_empty() {
                    None
                } else {
                    Some(Self::Translate(cmd_text))
                }
            }
            "/ai-config" | "/ai" => Some(Self::Config),
            "/ai-providers" => Some(Self::Providers),
            "/ai-set-key" => {
                if parts.len() >= 3 {
                    Some(Self::SetKey {
                        provider: parts[1].to_string(),
                        key: parts[2].to_string(),
                    })
                } else {
                    None
                }
            }
            "/ai-models" => {
                if parts.len() >= 2 {
                    Some(Self::Models(parts[1].to_string()))
                } else {
                    None
                }
            }
            "/ai-test" => Some(Self::Test),
            "/clear-chat" => Some(Self::ClearChat),
            _ => None,
        }
    }
}

/// AI command executor — processes AI commands and returns formatted output.
pub struct AiCommandExecutor {
    pub registry: ProviderRegistry,
    pub credentials: CredentialStore,
    pub chat_engine: ChatEngine,
}

impl AiCommandExecutor {
    /// Create a new executor with loaded credentials.
    pub fn new() -> Self {
        let credentials = CredentialStore::load();
        let mut registry = ProviderRegistry::new();

        // Apply stored credentials to providers
        for (provider_name, key) in &credentials.keys {
            registry.set_api_key(provider_name, key.clone());
        }

        Self {
            registry,
            credentials,
            chat_engine: ChatEngine::new(),
        }
    }

    /// Execute an AI command and return the output text.
    pub async fn execute(&mut self, command: &AiCommand) -> Result<String> {
        match command {
            AiCommand::Ask(query) => self.handle_ask(query).await,
            AiCommand::Explain(context) => self.handle_explain(context.as_deref()).await,
            AiCommand::Suggest => self.handle_suggest().await,
            AiCommand::Translate(cmd) => self.handle_translate(cmd).await,
            AiCommand::Config => Ok(self.handle_config()),
            AiCommand::Providers => Ok(self.handle_providers()),
            AiCommand::SetKey { provider, key } => self.handle_set_key(provider, key),
            AiCommand::Models(provider) => Ok(self.handle_models(provider)),
            AiCommand::Test => self.handle_test().await,
            AiCommand::ClearChat => {
                self.chat_engine.clear();
                Ok("Chat history cleared.".into())
            }
        }
    }

    /// /ask — send a question to the AI.
    async fn handle_ask(&mut self, query: &str) -> Result<String> {
        let provider = self.get_active_provider()?;
        let response = self.chat_engine.send(&provider, query).await?;
        Ok(Self::format_response(&response))
    }

    /// /explain — explain the last error.
    async fn handle_explain(&self, context: Option<&str>) -> Result<String> {
        let provider = self.get_active_provider()?;
        let prompt = if let Some(ctx) = context {
            format!(
                "Explain this terminal error or output concisely. What went wrong and how to fix it:\n\n{}",
                ctx
            )
        } else {
            "Explain the most common terminal errors and how to fix them.".into()
        };

        let response = ChatEngine::query(
            &provider,
            "You are a terminal error expert. Explain errors concisely with actionable fixes.",
            &prompt,
        ).await?;

        Ok(Self::format_response(&response))
    }

    /// /suggest — suggest the next command.
    async fn handle_suggest(&self) -> Result<String> {
        let provider = self.get_active_provider()?;
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".into());

        let prompt = format!(
            "Based on the current directory '{}', suggest 3-5 useful shell commands \
             the user might want to run. Format as a numbered list with brief descriptions.",
            cwd
        );

        let response = ChatEngine::query(
            &provider,
            "You are a shell command expert. Suggest practical commands.",
            &prompt,
        ).await?;

        Ok(Self::format_response(&response))
    }

    /// /translate — translate command between platforms.
    async fn handle_translate(&self, cmd: &str) -> Result<String> {
        let provider = self.get_active_provider()?;
        let prompt = format!(
            "Translate this command to all major platforms. Show Windows (cmd/PowerShell), \
             macOS, and Linux equivalents:\n\n{}",
            cmd
        );

        let response = ChatEngine::query(
            &provider,
            "You are a cross-platform command translator. Show equivalent commands.",
            &prompt,
        ).await?;

        Ok(Self::format_response(&response))
    }

    /// /ai-config — show current configuration.
    fn handle_config(&self) -> String {
        let mut out = String::new();
        out.push_str("╭─ Stratum AI Configuration ─────────────────╮\n");

        if let Some(ref name) = self.credentials.active_provider {
            out.push_str(&format!("│ Active Provider: {:<27}│\n", name));
        } else {
            out.push_str("│ Active Provider: (auto-detect)              │\n");
        }

        if let Some(ref model) = self.credentials.active_model {
            out.push_str(&format!("│ Active Model:    {:<27}│\n", model));
        }

        let configured = self.registry.configured_providers();
        out.push_str(&format!("│ Configured:      {}/29 providers{:>12}│\n",
            configured.len(), ""));

        out.push_str("├─────────────────────────────────────────────┤\n");

        if configured.is_empty() {
            out.push_str("│ No providers configured.                    │\n");
            out.push_str("│ Use: /ai-set-key <provider> <key>           │\n");
        } else {
            for p in &configured {
                let status = "✓";
                out.push_str(&format!("│ {} {:<20} {:<19}│\n",
                    status, p.config.display_name, p.active_model));
            }
        }

        out.push_str("╰─────────────────────────────────────────────╯\n");
        out
    }

    /// /ai-providers — list all providers.
    fn handle_providers(&self) -> String {
        let mut out = String::new();
        out.push_str("╭─ Available AI Providers (29) ────────────────────────────╮\n");
        out.push_str("│ #  Provider              API Key Env          Status    │\n");
        out.push_str("├──────────────────────────────────────────────────────────┤\n");

        for (i, p) in self.registry.all_providers().iter().enumerate() {
            let status = if p.is_configured() { "✓ ready" } else { "✗ no key" };
            out.push_str(&format!("│ {:<2} {:<22} {:<20} {:<8} │\n",
                i + 1, p.config.display_name, p.config.api_key_env, status));
        }

        out.push_str("╰──────────────────────────────────────────────────────────╯\n");
        out
    }

    /// /ai-set-key — set API key for a provider.
    fn handle_set_key(&mut self, provider: &str, key: &str) -> Result<String> {
        if !self.registry.set_api_key(provider, key.to_string()) {
            anyhow::bail!("Unknown provider: '{}'. Use /ai-providers to see available providers.", provider);
        }

        self.credentials.set_key(provider, key.to_string());
        self.credentials.save()?;

        Ok(format!("✓ API key set for '{}' and saved to credentials.", provider))
    }

    /// /ai-models — list models for a provider.
    fn handle_models(&self, provider: &str) -> String {
        match self.registry.get(provider) {
            Some(p) => {
                let mut out = String::new();
                out.push_str(&format!("Models for {} ({}):\n", p.config.display_name, p.config.name));
                for (i, model) in p.config.models.iter().enumerate() {
                    let marker = if *model == p.active_model { "→" } else { " " };
                    out.push_str(&format!("  {} {}. {}\n", marker, i + 1, model));
                }
                out
            }
            None => format!("Unknown provider: '{}'. Use /ai-providers to list all.", provider),
        }
    }

    /// /ai-test — test current provider connection.
    async fn handle_test(&self) -> Result<String> {
        let provider = self.get_active_provider()?;
        let response = ChatEngine::query(
            &provider,
            "You are a test assistant.",
            "Reply with exactly: 'Stratum AI connection successful.' and nothing else.",
        ).await?;

        Ok(format!(
            "✓ {} ({}) responded: {}",
            provider.config.display_name,
            provider.active_model,
            response.content.trim()
        ))
    }

    /// Get the currently active provider.
    fn get_active_provider(&self) -> Result<AiProvider> {
        // Check if user has set a specific provider
        if let Some(ref name) = self.credentials.active_provider {
            if let Some(p) = self.registry.get(name) {
                if p.is_configured() {
                    return Ok(p.clone());
                }
            }
        }

        // Auto-detect first configured provider
        self.registry.first_configured()
            .cloned()
            .context("No AI provider configured. Use /ai-set-key <provider> <key> to set up.")
    }

    /// Format an AI response for terminal display.
    fn format_response(response: &AiResponse) -> String {
        let mut out = String::new();
        out.push_str(&response.content);
        if let Some(ref usage) = response.usage {
            out.push_str(&format!(
                "\n─── {} │ {} tokens ───",
                response.model, usage.total_tokens
            ));
        }
        out
    }
}

impl Default for AiCommandExecutor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_ask() {
        let cmd = AiCommand::parse("/ask how do I list files?");
        assert!(matches!(cmd, Some(AiCommand::Ask(_))));
        if let Some(AiCommand::Ask(q)) = cmd {
            assert_eq!(q, "how do I list files?");
        }
    }

    #[test]
    fn test_parse_explain() {
        assert!(matches!(AiCommand::parse("/explain"), Some(AiCommand::Explain(None))));
        let cmd = AiCommand::parse("/explain permission denied");
        assert!(matches!(cmd, Some(AiCommand::Explain(Some(_)))));
    }

    #[test]
    fn test_parse_config_commands() {
        assert!(matches!(AiCommand::parse("/ai-config"), Some(AiCommand::Config)));
        assert!(matches!(AiCommand::parse("/ai"), Some(AiCommand::Config)));
        assert!(matches!(AiCommand::parse("/ai-providers"), Some(AiCommand::Providers)));
        assert!(matches!(AiCommand::parse("/ai-test"), Some(AiCommand::Test)));
        assert!(matches!(AiCommand::parse("/clear-chat"), Some(AiCommand::ClearChat)));
    }

    #[test]
    fn test_parse_set_key() {
        let cmd = AiCommand::parse("/ai-set-key anthropic sk-ant-xxx");
        assert!(matches!(cmd, Some(AiCommand::SetKey { .. })));
        if let Some(AiCommand::SetKey { provider, key }) = cmd {
            assert_eq!(provider, "anthropic");
            assert_eq!(key, "sk-ant-xxx");
        }
    }

    #[test]
    fn test_non_ai_commands() {
        assert!(AiCommand::parse("ls -la").is_none());
        assert!(AiCommand::parse("cd src").is_none());
        assert!(AiCommand::parse("git status").is_none());
    }
}
