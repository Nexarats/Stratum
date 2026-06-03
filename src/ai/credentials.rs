//! Credential Store — secure API key storage and retrieval.
//!
//! Keys are stored in `~/.stratum/credentials.toml` with restricted permissions.
//! Falls back to environment variables if no stored key exists.

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

/// Stored credentials for AI providers.
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct CredentialStore {
    /// Provider name → API key mapping.
    #[serde(default)]
    pub keys: HashMap<String, String>,
    /// Last active provider name.
    #[serde(default)]
    pub active_provider: Option<String>,
    /// Last active model for the active provider.
    #[serde(default)]
    pub active_model: Option<String>,
}

impl CredentialStore {
    /// Load credentials from disk, or create an empty store.
    pub fn load() -> Self {
        let path = Self::credentials_path();
        if path.exists() {
            match std::fs::read_to_string(&path) {
                Ok(content) => {
                    match toml::from_str::<CredentialStore>(&content) {
                        Ok(store) => return store,
                        Err(e) => {
                            tracing::warn!("Failed to parse credentials: {}", e);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("Failed to read credentials: {}", e);
                }
            }
        }
        Self::default()
    }

    /// Save credentials to disk.
    pub fn save(&self) -> Result<()> {
        let path = Self::credentials_path();

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .context("Failed to create credentials directory")?;
        }

        let content = toml::to_string_pretty(self)
            .context("Failed to serialize credentials")?;

        std::fs::write(&path, content)
            .context("Failed to write credentials file")?;

        // Set restrictive permissions on Unix
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = std::fs::Permissions::from_mode(0o600);
            std::fs::set_permissions(&path, perms)
                .context("Failed to set credentials permissions")?;
        }

        tracing::info!("Saved credentials to {}", path.display());
        Ok(())
    }

    /// Set an API key for a provider.
    pub fn set_key(&mut self, provider: &str, key: String) {
        self.keys.insert(provider.to_string(), key);
    }

    /// Get an API key for a provider. Checks store first, then env var.
    pub fn get_key(&self, provider: &str, env_var: &str) -> Option<String> {
        // 1. Check stored credentials
        if let Some(key) = self.keys.get(provider) {
            if !key.is_empty() {
                return Some(key.clone());
            }
        }
        // 2. Fall back to environment variable
        std::env::var(env_var).ok()
    }

    /// Remove an API key for a provider.
    pub fn remove_key(&mut self, provider: &str) -> bool {
        self.keys.remove(provider).is_some()
    }

    /// Check if any providers have keys configured.
    pub fn has_any_keys(&self) -> bool {
        !self.keys.is_empty() || self.has_env_keys()
    }

    /// Check if any environment variables for known providers are set.
    fn has_env_keys(&self) -> bool {
        let env_vars = [
            "ANTHROPIC_API_KEY", "GEMINI_API_KEY", "OPENROUTER_API_KEY",
            "DEEPSEEK_API_KEY", "OPENAI_API_KEY", "XAI_API_KEY",
            "GITHUB_TOKEN", "HF_TOKEN", "DASHSCOPE_API_KEY",
            "NVIDIA_API_KEY", "MOONSHOT_API_KEY", "MINIMAX_API_KEY",
        ];
        env_vars.iter().any(|var| std::env::var(var).is_ok())
    }

    /// Set the active provider and model.
    pub fn set_active(&mut self, provider: String, model: Option<String>) {
        self.active_provider = Some(provider);
        self.active_model = model;
    }

    /// Path to the credentials file.
    fn credentials_path() -> PathBuf {
        if let Some(home) = dirs::home_dir() {
            home.join(".stratum").join("credentials.toml")
        } else if let Some(config) = dirs::config_dir() {
            config.join("stratum").join("credentials.toml")
        } else {
            PathBuf::from("stratum-credentials.toml")
        }
    }
}
