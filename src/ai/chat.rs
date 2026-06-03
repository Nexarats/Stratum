//! Chat Engine — sends messages to AI providers and receives responses.
//!
//! Supports OpenAI-compatible chat/completions format (used by 27/29 providers)
//! and Anthropic's messages format.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use super::provider::{AiMessage, AiProvider, AiResponse, AiRole, TokenUsage};

/// The chat engine manages conversation state and sends requests to AI providers.
pub struct ChatEngine {
    /// Conversation history.
    pub messages: Vec<AiMessage>,
    /// System prompt.
    system_prompt: String,
    /// Maximum tokens to generate.
    pub max_tokens: u32,
    /// Temperature (0.0 - 2.0).
    pub temperature: f32,
}

/// OpenAI-compatible request body.
#[derive(Serialize)]
struct OpenAiRequest {
    model: String,
    messages: Vec<AiMessage>,
    max_tokens: u32,
    temperature: f32,
    stream: bool,
}

/// Anthropic-specific request body.
#[derive(Serialize)]
struct AnthropicRequest {
    model: String,
    messages: Vec<AiMessage>,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
}

/// OpenAI-compatible response.
#[derive(Deserialize)]
struct OpenAiResponse {
    choices: Vec<OpenAiChoice>,
    model: Option<String>,
    usage: Option<OpenAiUsage>,
}

#[derive(Deserialize)]
struct OpenAiChoice {
    message: OpenAiMessage,
    finish_reason: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiMessage {
    content: Option<String>,
}

#[derive(Deserialize)]
struct OpenAiUsage {
    prompt_tokens: Option<u32>,
    completion_tokens: Option<u32>,
    total_tokens: Option<u32>,
}

/// Anthropic response format.
#[derive(Deserialize)]
struct AnthropicResponse {
    content: Vec<AnthropicContent>,
    model: Option<String>,
    usage: Option<AnthropicUsage>,
    stop_reason: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicContent {
    text: Option<String>,
}

#[derive(Deserialize)]
struct AnthropicUsage {
    input_tokens: Option<u32>,
    output_tokens: Option<u32>,
}

impl ChatEngine {
    /// Create a new chat engine with the Stratum system prompt.
    pub fn new() -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: Self::default_system_prompt(),
            max_tokens: 2048,
            temperature: 0.3,
        }
    }

    /// Create with a custom system prompt.
    pub fn with_system_prompt(prompt: impl Into<String>) -> Self {
        Self {
            messages: Vec::new(),
            system_prompt: prompt.into(),
            max_tokens: 2048,
            temperature: 0.3,
        }
    }

    /// Default system prompt for Stratum terminal AI.
    fn default_system_prompt() -> String {
        String::from(
            "You are Stratum AI, a terminal assistant built into the Stratum terminal emulator. \
             You help users with shell commands, explain errors, suggest fixes, and translate \
             between command syntaxes. Keep responses concise and practical — users are in a terminal. \
             When suggesting commands, use code blocks. When explaining errors, be direct."
        )
    }

    /// Send a message and get a response from the given provider.
    pub async fn send(
        &mut self,
        provider: &AiProvider,
        user_message: &str,
    ) -> Result<AiResponse> {
        let api_key = provider.api_key.as_ref()
            .context("No API key configured for this provider")?;

        // Add user message to history
        self.messages.push(AiMessage::user(user_message));

        let response = if provider.config.name == "anthropic" {
            self.send_anthropic(provider, api_key).await?
        } else {
            self.send_openai_compatible(provider, api_key).await?
        };

        // Add assistant response to history
        self.messages.push(AiMessage::assistant(&response.content));

        Ok(response)
    }

    /// Send a one-shot query (no history).
    pub async fn query(
        provider: &AiProvider,
        system: &str,
        user_message: &str,
    ) -> Result<AiResponse> {
        let api_key = provider.api_key.as_ref()
            .context("No API key configured for this provider")?;

        let messages = vec![
            AiMessage::system(system),
            AiMessage::user(user_message),
        ];

        if provider.config.name == "anthropic" {
            Self::send_anthropic_messages(provider, api_key, &messages, Some(system.to_string())).await
        } else {
            Self::send_openai_messages(provider, api_key, &messages).await
        }
    }

    /// Send using OpenAI-compatible format (used by 27/29 providers).
    async fn send_openai_compatible(
        &self,
        provider: &AiProvider,
        api_key: &str,
    ) -> Result<AiResponse> {
        let mut all_messages = vec![AiMessage::system(&self.system_prompt)];
        all_messages.extend(self.messages.clone());

        Self::send_openai_messages(provider, api_key, &all_messages).await
    }

    /// Static helper for OpenAI-compatible requests.
    async fn send_openai_messages(
        provider: &AiProvider,
        api_key: &str,
        messages: &[AiMessage],
    ) -> Result<AiResponse> {
        let body = OpenAiRequest {
            model: provider.active_model.clone(),
            messages: messages.to_vec(),
            max_tokens: 2048,
            temperature: 0.3,
            stream: false,
        };

        let client = reqwest::Client::new();
        let mut req = client.post(provider.chat_endpoint())
            .json(&body);

        // Auth
        if provider.config.use_bearer_auth {
            req = req.bearer_auth(api_key);
        } else {
            req = req.header("x-api-key", api_key);
        }

        // Custom headers
        for (key, value) in &provider.config.custom_headers {
            req = req.header(key, value);
        }

        let resp = req.send().await
            .context("Failed to send request to AI provider")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("AI provider returned {}: {}", status, body);
        }

        let data: OpenAiResponse = resp.json().await
            .context("Failed to parse AI response")?;

        let content = data.choices.first()
            .and_then(|c| c.message.content.clone())
            .unwrap_or_default();

        let usage = data.usage.map(|u| TokenUsage {
            prompt_tokens: u.prompt_tokens.unwrap_or(0),
            completion_tokens: u.completion_tokens.unwrap_or(0),
            total_tokens: u.total_tokens.unwrap_or(0),
        });

        let finish_reason = data.choices.first()
            .and_then(|c| c.finish_reason.clone());

        Ok(AiResponse {
            content,
            model: data.model.unwrap_or_else(|| provider.active_model.clone()),
            usage,
            finish_reason,
        })
    }

    /// Send using Anthropic messages format.
    async fn send_anthropic(
        &self,
        provider: &AiProvider,
        api_key: &str,
    ) -> Result<AiResponse> {
        // Anthropic: system is a top-level field, not in messages
        let non_system: Vec<AiMessage> = self.messages.iter()
            .filter(|m| m.role != AiRole::System)
            .cloned()
            .collect();

        Self::send_anthropic_messages(
            provider,
            api_key,
            &non_system,
            Some(self.system_prompt.clone()),
        ).await
    }

    /// Static helper for Anthropic requests.
    async fn send_anthropic_messages(
        provider: &AiProvider,
        api_key: &str,
        messages: &[AiMessage],
        system: Option<String>,
    ) -> Result<AiResponse> {
        let non_system: Vec<AiMessage> = messages.iter()
            .filter(|m| m.role != AiRole::System)
            .cloned()
            .collect();

        let body = AnthropicRequest {
            model: provider.active_model.clone(),
            messages: non_system,
            max_tokens: 2048,
            temperature: 0.3,
            system,
        };

        let client = reqwest::Client::new();
        let mut req = client.post(provider.chat_endpoint())
            .json(&body)
            .header("x-api-key", api_key);

        for (key, value) in &provider.config.custom_headers {
            req = req.header(key, value);
        }

        let resp = req.send().await
            .context("Failed to send request to Anthropic")?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            anyhow::bail!("Anthropic returned {}: {}", status, body);
        }

        let data: AnthropicResponse = resp.json().await
            .context("Failed to parse Anthropic response")?;

        let content = data.content.first()
            .and_then(|c| c.text.clone())
            .unwrap_or_default();

        let usage = data.usage.map(|u| TokenUsage {
            prompt_tokens: u.input_tokens.unwrap_or(0),
            completion_tokens: u.output_tokens.unwrap_or(0),
            total_tokens: u.input_tokens.unwrap_or(0) + u.output_tokens.unwrap_or(0),
        });

        Ok(AiResponse {
            content,
            model: data.model.unwrap_or_else(|| provider.active_model.clone()),
            usage,
            finish_reason: data.stop_reason,
        })
    }

    /// Clear conversation history.
    pub fn clear(&mut self) {
        self.messages.clear();
    }
}

impl Default for ChatEngine {
    fn default() -> Self {
        Self::new()
    }
}
