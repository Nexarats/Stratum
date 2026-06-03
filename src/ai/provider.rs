//! AI Provider types — unified interface for all 29 AI providers.
//!
//! Every provider speaks OpenAI-compatible chat/completions format.
//! The provider config stores: name, aliases, base URL, default model,
//! API key env var, and available models.

use serde::{Deserialize, Serialize};

/// Role in a chat conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiRole {
    System,
    User,
    Assistant,
}

/// A single message in a chat conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiMessage {
    pub role: AiRole,
    pub content: String,
}

impl AiMessage {
    pub fn system(content: impl Into<String>) -> Self {
        Self { role: AiRole::System, content: content.into() }
    }
    pub fn user(content: impl Into<String>) -> Self {
        Self { role: AiRole::User, content: content.into() }
    }
    pub fn assistant(content: impl Into<String>) -> Self {
        Self { role: AiRole::Assistant, content: content.into() }
    }
}

/// Response from an AI provider.
#[derive(Debug, Clone)]
pub struct AiResponse {
    /// The generated text content.
    pub content: String,
    /// Model used for generation.
    pub model: String,
    /// Token counts (if available).
    pub usage: Option<TokenUsage>,
    /// Whether the response was truncated.
    pub finish_reason: Option<String>,
}

/// Token usage statistics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Configuration for an AI provider.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiProviderConfig {
    /// Canonical provider name (e.g., "anthropic", "gemini").
    pub name: String,
    /// Human-readable display name.
    pub display_name: String,
    /// Alternative names that resolve to this provider.
    pub aliases: Vec<String>,
    /// Base URL for the API endpoint.
    pub base_url: String,
    /// Path appended to base URL for chat completions.
    pub chat_path: String,
    /// Default model to use.
    pub default_model: String,
    /// Available model IDs.
    pub models: Vec<String>,
    /// Environment variable name for the API key.
    pub api_key_env: String,
    /// Whether this provider requires a custom auth flow (OAuth, etc.).
    pub requires_oauth: bool,
    /// Custom headers to send with requests (e.g., x-api-key for Anthropic).
    pub custom_headers: Vec<(String, String)>,
    /// Whether to use Bearer token auth (true) or custom header (false).
    pub use_bearer_auth: bool,
}

/// A resolved, ready-to-use AI provider instance.
#[derive(Debug, Clone)]
pub struct AiProvider {
    pub config: AiProviderConfig,
    /// The resolved API key (from env, credential store, or config).
    pub api_key: Option<String>,
    /// Currently selected model.
    pub active_model: String,
}

impl AiProvider {
    /// Create a provider from config, attempting to resolve the API key.
    pub fn from_config(config: AiProviderConfig) -> Self {
        let api_key = std::env::var(&config.api_key_env).ok();
        let active_model = config.default_model.clone();
        Self { config, api_key, active_model }
    }

    /// Check if this provider is configured (has an API key).
    pub fn is_configured(&self) -> bool {
        self.api_key.as_ref().map_or(false, |k| !k.is_empty())
    }

    /// Get the full endpoint URL for chat completions.
    pub fn chat_endpoint(&self) -> String {
        let base = self.config.base_url.trim_end_matches('/');
        let path = self.config.chat_path.trim_start_matches('/');
        format!("{}/{}", base, path)
    }

    /// Set the API key (from credential store or user input).
    pub fn set_api_key(&mut self, key: String) {
        self.api_key = Some(key);
    }

    /// Set the active model.
    pub fn set_model(&mut self, model: String) {
        self.active_model = model;
    }

    /// Check if a name matches this provider (canonical or alias).
    pub fn matches_name(&self, name: &str) -> bool {
        let lower = name.to_lowercase();
        if self.config.name == lower {
            return true;
        }
        self.config.aliases.iter().any(|a| a == &lower)
    }
}
