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
    /// `/ai-set-provider <name>` — switch active provider.
    SetProvider(String),
    /// `/ai-set-model <model>` — switch active model.
    SetModel(String),
    /// `/ai-models <provider>` — list models for a provider.
    Models(String),
    /// `/ai-test` — test the current AI connection.
    Test,
    /// `/clear-chat` — clear AI conversation history.
    ClearChat,
    /// `/shelp` — show the Stratum help guide.
    Shelp,
}

impl AiCommand {
    /// Try to parse an AI command from a command string.
    /// Returns None if it's not an AI command (pass to shell).
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        if trimmed.is_empty() {
            return None;
        }

        let normalized = if trimmed.starts_with('/') {
            trimmed.to_string()
        } else {
            let first_word = trimmed.split_whitespace().next().unwrap_or("");
            let commands = [
                "ask", "explain", "suggest", "translate", "ai-config", "ai", 
                "ai-providers", "ai-set-key", "ai-models", "ai-set-provider", 
                "ai-set-model", "ai-test", "clear-chat", "shelp"
            ];
            if commands.contains(&first_word) {
                format!("/{}", trimmed)
            } else {
                return None;
            }
        };

        let parts: Vec<&str> = normalized.splitn(3, ' ').collect();
        let cmd = parts[0].to_lowercase();

        match cmd.as_str() {
            "/shelp" => Some(Self::Shelp),
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
            "/ai-set-provider" => {
                if parts.len() >= 2 {
                    Some(Self::SetProvider(parts[1].to_string()))
                } else {
                    None
                }
            }
            "/ai-set-model" => {
                if parts.len() >= 2 {
                    Some(Self::SetModel(parts[1].to_string()))
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
            AiCommand::SetProvider(name) => self.handle_set_provider(name),
            AiCommand::SetModel(model) => self.handle_set_model(model),
            AiCommand::Models(provider) => Ok(self.handle_models(provider)),
            AiCommand::Test => self.handle_test().await,
            AiCommand::ClearChat => {
                self.chat_engine.clear();
                Ok("Chat history cleared.".into())
            }
            AiCommand::Shelp => Ok(self.handle_shelp()),
        }
    }

    /// shelp — show Stratum Shell AI Command Guide.
    fn handle_shelp(&self) -> String {
        let mut out = String::new();
        out.push_str("╭─ Stratum Shell AI — Command Guide ──────────────────────────────────────╮\n");
        out.push_str("│                                                                         │\n");
        out.push_str("│   To interact with the AI or the Shell, you have two modes (toggle with │\n");
        out.push_str("│   the [Tab] key):                                                       │\n");
        out.push_str("│                                                                         │\n");
        out.push_str("│     [shell] Mode: Commands run on the host PTY (PowerShell/bash) directly.│\n");
        out.push_str("│     [ai] Mode:    Input is processed by the AI agent in natural language. │\n");
        out.push_str("│                                                                         │\n");
        out.push_str("│   Available Commands (can be run in both modes, with or without '/'):    │\n");
        out.push_str("│                                                                         │\n");
        out.push_str("│     shelp                     - Show this interactive help guide        │\n");
        out.push_str("│     ask <question>            - Ask the AI a question directly          │\n");
        out.push_str("│     explain <output/error>    - Explain a terminal output or error      │\n");
        out.push_str("│     suggest                   - Suggest commands in the current directory│\n");
        out.push_str("│     translate <command>       - Translate commands between OS platforms │\n");
        out.push_str("│     clear-chat                - Clear current conversation history      │\n");
        out.push_str("│                                                                         │\n");
        out.push_str("│   AI Configuration Commands:                                             │\n");
        out.push_str("│                                                                         │\n");
        out.push_str("│     ai-config (or 'ai')       - View currently active provider & status │\n");
        out.push_str("│     ai-providers              - List all 29 supported AI providers      │\n");
        out.push_str("│     ai-set-key <prov> <key>   - Save API key for a provider             │\n");
        out.push_str("│     ai-models <prov>          - List available models for a provider    │\n");
        out.push_str("│     ai-set-provider <name>    - Change the active provider              │\n");
        out.push_str("│     ai-set-model <model>      - Change the active model                 │\n");
        out.push_str("│     ai-test                   - Test connection to the active provider  │\n");
        out.push_str("│                                                                         │\n");
        out.push_str("╰─────────────────────────────────────────────────────────────────────────╯\n");
        out
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

    /// /ai-set-provider — switch active provider.
    fn handle_set_provider(&mut self, name: &str) -> Result<String> {
        // Verify the provider exists
        let provider = self.registry.get(name)
            .ok_or_else(|| anyhow::anyhow!("Unknown provider: '{}'. Use /ai-providers to see available providers.", name))?;
        
        self.credentials.active_provider = Some(name.to_string());
        self.credentials.active_model = Some(provider.config.default_model.clone());
        let _ = self.credentials.save();
        Ok(format!("✓ Active provider set to '{}' (default model: {})", name, provider.config.default_model))
    }

    /// /ai-set-model — switch active model.
    fn handle_set_model(&mut self, model: &str) -> Result<String> {
        if let Some(ref provider_name) = self.credentials.active_provider {
            if let Some(provider) = self.registry.get(provider_name) {
                if !provider.config.models.contains(&model.to_string()) && model != provider.config.default_model {
                    anyhow::bail!(
                        "Model '{}' is not supported by active provider '{}'. Supported models: {:?}",
                        model,
                        provider.config.display_name,
                        provider.config.models
                    );
                }
            }
        }
        self.credentials.active_model = Some(model.to_string());
        let _ = self.credentials.save();
        Ok(format!("✓ Active model set to '{}'", model))
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
    pub fn get_active_provider(&self) -> Result<AiProvider> {
        // Check if user has set a specific provider
        if let Some(ref name) = self.credentials.active_provider {
            if let Some(p) = self.registry.get(name) {
                if p.is_configured() {
                    let mut provider = p.clone();
                    if let Some(ref model) = self.credentials.active_model {
                        if provider.config.models.contains(model) || model == &provider.config.default_model {
                            provider.active_model = model.clone();
                        }
                    }
                    return Ok(provider);
                }
            }
        }

        // Auto-detect first configured provider
        let mut provider = self.registry.first_configured()
            .cloned()
            .context("No AI provider configured. Use /ai-set-key <provider> <key> to set up.")?;

        if let Some(ref model) = self.credentials.active_model {
            if provider.config.models.contains(model) || model == &provider.config.default_model {
                provider.active_model = model.clone();
            }
        }
        Ok(provider)
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
    fn test_parse_slash_free_commands() {
        let cmd = AiCommand::parse("ask how do I list files?");
        assert!(matches!(cmd, Some(AiCommand::Ask(_))));
        if let Some(AiCommand::Ask(q)) = cmd {
            assert_eq!(q, "how do I list files?");
        }

        let cmd = AiCommand::parse("explain permission denied");
        assert!(matches!(cmd, Some(AiCommand::Explain(Some(_)))));
        if let Some(AiCommand::Explain(Some(ctx))) = cmd {
            assert_eq!(ctx, "permission denied");
        }

        assert!(matches!(AiCommand::parse("suggest"), Some(AiCommand::Suggest)));
        assert!(matches!(AiCommand::parse("shelp"), Some(AiCommand::Shelp)));
        assert!(matches!(AiCommand::parse("ai-config"), Some(AiCommand::Config)));
        assert!(matches!(AiCommand::parse("ai"), Some(AiCommand::Config)));
        assert!(matches!(AiCommand::parse("ai-providers"), Some(AiCommand::Providers)));
        assert!(matches!(AiCommand::parse("clear-chat"), Some(AiCommand::ClearChat)));
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
