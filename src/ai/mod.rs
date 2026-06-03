//! AI module — provider registry, API key management, chat engine, and inline AI commands.
//!
//! Supports 29 providers from hermes-agent with unified API interface.
//! All providers use OpenAI-compatible chat/completions endpoint format.

pub mod provider;
pub mod registry;
pub mod chat;
pub mod credentials;
pub mod commands;

pub use provider::{AiProvider, AiProviderConfig, AiMessage, AiRole, AiResponse};
pub use registry::ProviderRegistry;
pub use chat::ChatEngine;
pub use credentials::CredentialStore;
