//! Provider Registry — all 29 AI providers from hermes-agent.
//!
//! Each provider is pre-configured with base URL, chat path, default model,
//! aliases, and available models. The registry provides lookup by name/alias.

use super::provider::{AiProvider, AiProviderConfig};

/// Registry of all supported AI providers.
pub struct ProviderRegistry {
    providers: Vec<AiProvider>,
}

impl ProviderRegistry {
    /// Create a registry with all 29 providers pre-configured.
    pub fn new() -> Self {
        let configs = vec![
            // 1. Alibaba (DashScope)
            AiProviderConfig {
                name: "alibaba".into(),
                display_name: "Alibaba (DashScope)".into(),
                aliases: vec!["dashscope".into(), "alibaba-cloud".into(), "qwen-dashscope".into()],
                base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "qwen-turbo".into(),
                models: vec!["qwen-turbo".into(), "qwen-plus".into(), "qwen-max".into()],
                api_key_env: "DASHSCOPE_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 2. Alibaba Coding Plan
            AiProviderConfig {
                name: "alibaba-coding-plan".into(),
                display_name: "Alibaba Coding Plan".into(),
                aliases: vec!["alibaba_coding".into(), "alibaba-coding".into(), "dashscope-coding".into()],
                base_url: "https://dashscope.aliyuncs.com/compatible-mode/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "qwen-coder-turbo".into(),
                models: vec!["qwen-coder-turbo".into(), "qwen-coder-plus".into()],
                api_key_env: "DASHSCOPE_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 3. Anthropic
            AiProviderConfig {
                name: "anthropic".into(),
                display_name: "Anthropic (Claude)".into(),
                aliases: vec!["claude".into(), "claude-oauth".into(), "claude-code".into()],
                base_url: "https://api.anthropic.com/v1".into(),
                chat_path: "messages".into(),
                default_model: "claude-haiku-4-5-20251001".into(),
                models: vec![
                    "claude-haiku-4-5-20251001".into(),
                    "claude-sonnet-4-20250514".into(),
                    "claude-opus-4-20250514".into(),
                    "claude-3-5-haiku-20241022".into(),
                    "claude-3-5-sonnet-20241022".into(),
                ],
                api_key_env: "ANTHROPIC_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![
                    ("anthropic-version".into(), "2023-06-01".into()),
                ],
                use_bearer_auth: false, // uses x-api-key header
            },
            // 4. Arcee AI
            AiProviderConfig {
                name: "arcee".into(),
                display_name: "Arcee AI".into(),
                aliases: vec!["arcee-ai".into(), "arceeai".into()],
                base_url: "https://conductor.arcee.ai/v2".into(),
                chat_path: "chat/completions".into(),
                default_model: "auto".into(),
                models: vec!["auto".into()],
                api_key_env: "ARCEE_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 5. Azure AI Foundry
            AiProviderConfig {
                name: "azure-foundry".into(),
                display_name: "Azure AI Foundry".into(),
                aliases: vec!["azure".into(), "azure-ai-foundry".into(), "azure-ai".into()],
                base_url: "https://models.inference.ai.azure.com".into(),
                chat_path: "chat/completions".into(),
                default_model: "gpt-4o".into(),
                models: vec!["gpt-4o".into(), "gpt-4o-mini".into()],
                api_key_env: "AZURE_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 6. AWS Bedrock
            AiProviderConfig {
                name: "bedrock".into(),
                display_name: "AWS Bedrock".into(),
                aliases: vec!["aws".into(), "aws-bedrock".into(), "amazon-bedrock".into(), "amazon".into()],
                base_url: "https://bedrock-runtime.us-east-1.amazonaws.com".into(),
                chat_path: "model/invoke".into(),
                default_model: "anthropic.claude-3-haiku-20240307-v1:0".into(),
                models: vec![
                    "anthropic.claude-3-haiku-20240307-v1:0".into(),
                    "anthropic.claude-3-sonnet-20240229-v1:0".into(),
                ],
                api_key_env: "AWS_ACCESS_KEY_ID".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: false,
            },
            // 7. GitHub Copilot
            AiProviderConfig {
                name: "copilot".into(),
                display_name: "GitHub Copilot".into(),
                aliases: vec!["github-copilot".into(), "github-models".into(), "github-model".into(), "github".into()],
                base_url: "https://api.githubcopilot.com".into(),
                chat_path: "chat/completions".into(),
                default_model: "gpt-4o".into(),
                models: vec!["gpt-4o".into(), "gpt-4o-mini".into(), "claude-3.5-sonnet".into()],
                api_key_env: "GITHUB_TOKEN".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 8. Copilot ACP
            AiProviderConfig {
                name: "copilot-acp".into(),
                display_name: "GitHub Copilot ACP".into(),
                aliases: vec!["github-copilot-acp".into(), "copilot-acp-agent".into()],
                base_url: "https://api.githubcopilot.com".into(),
                chat_path: "chat/completions".into(),
                default_model: "gpt-4o".into(),
                models: vec!["gpt-4o".into()],
                api_key_env: "GITHUB_TOKEN".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 9. Custom (OpenAI-compatible)
            AiProviderConfig {
                name: "custom".into(),
                display_name: "Custom (OpenAI-compatible)".into(),
                aliases: vec![],
                base_url: "http://localhost:11434/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "default".into(),
                models: vec![],
                api_key_env: "OPENAI_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 10. DeepSeek
            AiProviderConfig {
                name: "deepseek".into(),
                display_name: "DeepSeek".into(),
                aliases: vec!["deepseek-chat".into()],
                base_url: "https://api.deepseek.com/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "deepseek-chat".into(),
                models: vec!["deepseek-chat".into(), "deepseek-reasoner".into()],
                api_key_env: "DEEPSEEK_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 11. Google Gemini
            AiProviderConfig {
                name: "gemini".into(),
                display_name: "Google Gemini".into(),
                aliases: vec!["google".into(), "google-gemini".into(), "google-ai-studio".into()],
                base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
                chat_path: "chat/completions".into(),
                default_model: "gemini-2.0-flash".into(),
                models: vec![
                    "gemini-2.0-flash".into(),
                    "gemini-2.0-flash-lite".into(),
                    "gemini-1.5-pro".into(),
                    "gemini-1.5-flash".into(),
                ],
                api_key_env: "GEMINI_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 12. Gemini CLI (OAuth)
            AiProviderConfig {
                name: "google-gemini-cli".into(),
                display_name: "Gemini CLI (OAuth)".into(),
                aliases: vec!["gemini-cli".into(), "gemini-oauth".into()],
                base_url: "https://generativelanguage.googleapis.com/v1beta/openai".into(),
                chat_path: "chat/completions".into(),
                default_model: "gemini-2.0-flash".into(),
                models: vec!["gemini-2.0-flash".into()],
                api_key_env: "GEMINI_API_KEY".into(),
                requires_oauth: true,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 13. GMI Cloud
            AiProviderConfig {
                name: "gmi".into(),
                display_name: "GMI Cloud".into(),
                aliases: vec!["gmi-cloud".into(), "gmicloud".into()],
                base_url: "https://api.gmi-serving.com/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "google/gemini-3.1-flash-lite-preview".into(),
                models: vec![
                    "google/gemini-3.1-flash-lite-preview".into(),
                    "deepseek-ai/DeepSeek-V3.2".into(),
                    "moonshotai/Kimi-K2.5".into(),
                ],
                api_key_env: "GMI_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 14. Hugging Face
            AiProviderConfig {
                name: "huggingface".into(),
                display_name: "Hugging Face".into(),
                aliases: vec!["hf".into(), "hugging-face".into(), "huggingface-hub".into()],
                base_url: "https://api-inference.huggingface.co/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "Qwen/Qwen3-235B-A22B".into(),
                models: vec![
                    "Qwen/Qwen3-235B-A22B".into(),
                    "deepseek-ai/DeepSeek-V3-0324".into(),
                ],
                api_key_env: "HF_TOKEN".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 15. KiloCode
            AiProviderConfig {
                name: "kilocode".into(),
                display_name: "KiloCode".into(),
                aliases: vec!["kilo-code".into(), "kilo".into(), "kilo-gateway".into()],
                base_url: "https://gateway.kilocode.ai/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "google/gemini-3-flash-preview".into(),
                models: vec!["google/gemini-3-flash-preview".into()],
                api_key_env: "KILOCODE_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 16. Kimi (Moonshot)
            AiProviderConfig {
                name: "kimi-coding".into(),
                display_name: "Kimi (Moonshot)".into(),
                aliases: vec!["kimi".into(), "moonshot".into(), "kimi-for-coding".into()],
                base_url: "https://api.moonshot.cn/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "kimi-k2-turbo-preview".into(),
                models: vec!["kimi-k2-turbo-preview".into(), "moonshot-v1-8k".into()],
                api_key_env: "MOONSHOT_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 17. MiniMax
            AiProviderConfig {
                name: "minimax".into(),
                display_name: "MiniMax".into(),
                aliases: vec!["mini-max".into()],
                base_url: "https://api.minimax.chat/v1".into(),
                chat_path: "text/chatcompletion_v2".into(),
                default_model: "MiniMax-M2.7".into(),
                models: vec!["MiniMax-M2.7".into(), "MiniMax-Text-01".into()],
                api_key_env: "MINIMAX_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 18. Nous Research
            AiProviderConfig {
                name: "nous".into(),
                display_name: "Nous Research".into(),
                aliases: vec!["nous-portal".into(), "nousresearch".into()],
                base_url: "https://inference-api.nousresearch.com/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "hermes-3-405b".into(),
                models: vec!["hermes-3-405b".into(), "hermes-3-70b".into()],
                api_key_env: "NOUS_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 19. Novita AI
            AiProviderConfig {
                name: "novita".into(),
                display_name: "Novita AI".into(),
                aliases: vec!["novita-ai".into(), "novitaai".into()],
                base_url: "https://api.novita.ai/v3/openai".into(),
                chat_path: "chat/completions".into(),
                default_model: "deepseek/deepseek-v3-0324".into(),
                models: vec![
                    "deepseek/deepseek-v3-0324".into(),
                    "deepseek/deepseek-r1-0528".into(),
                    "moonshotai/kimi-k2.5".into(),
                    "qwen/qwen3-235b-a22b-fp8".into(),
                ],
                api_key_env: "NOVITA_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 20. NVIDIA NIM
            AiProviderConfig {
                name: "nvidia".into(),
                display_name: "NVIDIA NIM".into(),
                aliases: vec!["nvidia-nim".into()],
                base_url: "https://integrate.api.nvidia.com/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "nvidia/llama-3.1-nemotron-70b-instruct".into(),
                models: vec![
                    "nvidia/llama-3.1-nemotron-70b-instruct".into(),
                    "nvidia/llama-3.3-70b-instruct".into(),
                ],
                api_key_env: "NVIDIA_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 21. Ollama Cloud
            AiProviderConfig {
                name: "ollama-cloud".into(),
                display_name: "Ollama Cloud".into(),
                aliases: vec!["ollama_cloud".into()],
                base_url: "https://ollama.cloud/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "nemotron-3-nano:30b".into(),
                models: vec!["nemotron-3-nano:30b".into(), "llama3.1:70b".into()],
                api_key_env: "OLLAMA_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 22. OpenAI Codex
            AiProviderConfig {
                name: "openai-codex".into(),
                display_name: "OpenAI Codex".into(),
                aliases: vec!["codex".into(), "openai_codex".into()],
                base_url: "https://api.openai.com/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "gpt-4o".into(),
                models: vec!["gpt-4o".into(), "gpt-4o-mini".into(), "o1".into(), "o3-mini".into()],
                api_key_env: "OPENAI_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 23. OpenCode Zen
            AiProviderConfig {
                name: "opencode-zen".into(),
                display_name: "OpenCode Zen".into(),
                aliases: vec!["opencode".into(), "opencode_zen".into(), "zen".into()],
                base_url: "https://api.opencode.ai/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "gemini-3-flash".into(),
                models: vec!["gemini-3-flash".into()],
                api_key_env: "OPENCODE_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 24. OpenRouter
            AiProviderConfig {
                name: "openrouter".into(),
                display_name: "OpenRouter".into(),
                aliases: vec!["or".into()],
                base_url: "https://openrouter.ai/api/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "anthropic/claude-sonnet-4".into(),
                models: vec![
                    "anthropic/claude-sonnet-4".into(),
                    "openai/gpt-4o".into(),
                    "deepseek/deepseek-chat".into(),
                    "google/gemini-2.0-flash-001".into(),
                    "qwen/qwen3-plus".into(),
                ],
                api_key_env: "OPENROUTER_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![
                    ("HTTP-Referer".into(), "https://nexarats.com/stratum".into()),
                    ("X-Title".into(), "Stratum Terminal".into()),
                ],
                use_bearer_auth: true,
            },
            // 25. Qwen (OAuth)
            AiProviderConfig {
                name: "qwen-oauth".into(),
                display_name: "Qwen (OAuth)".into(),
                aliases: vec!["qwen".into(), "qwen-portal".into(), "qwen-cli".into()],
                base_url: "https://chat.qwenlm.ai/api/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "qwen3-plus".into(),
                models: vec!["qwen3-plus".into(), "qwen3-turbo".into()],
                api_key_env: "QWEN_API_KEY".into(),
                requires_oauth: true,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 26. StepFun
            AiProviderConfig {
                name: "stepfun".into(),
                display_name: "StepFun".into(),
                aliases: vec!["step".into(), "stepfun-coding-plan".into()],
                base_url: "https://api.stepfun.com/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "step-3.5-flash".into(),
                models: vec!["step-3.5-flash".into(), "step-2-16k".into()],
                api_key_env: "STEPFUN_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 27. xAI (Grok)
            AiProviderConfig {
                name: "xai".into(),
                display_name: "xAI (Grok)".into(),
                aliases: vec!["grok".into(), "x-ai".into(), "x.ai".into()],
                base_url: "https://api.x.ai/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "grok-2".into(),
                models: vec!["grok-2".into(), "grok-2-mini".into()],
                api_key_env: "XAI_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 28. Xiaomi (MiMo)
            AiProviderConfig {
                name: "xiaomi".into(),
                display_name: "Xiaomi (MiMo)".into(),
                aliases: vec!["mimo".into(), "xiaomi-mimo".into()],
                base_url: "https://api.xiaomi.com/v1".into(),
                chat_path: "chat/completions".into(),
                default_model: "mimo-v1".into(),
                models: vec!["mimo-v1".into()],
                api_key_env: "XIAOMI_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
            // 29. ZAI (Zhipu/GLM)
            AiProviderConfig {
                name: "zai".into(),
                display_name: "ZAI (Zhipu/GLM)".into(),
                aliases: vec!["glm".into(), "z-ai".into(), "z.ai".into(), "zhipu".into()],
                base_url: "https://open.bigmodel.cn/api/paas/v4".into(),
                chat_path: "chat/completions".into(),
                default_model: "glm-4.5-flash".into(),
                models: vec!["glm-4.5-flash".into(), "glm-4-flash".into(), "glm-4".into(), "glm-4-9b".into()],
                api_key_env: "ZAI_API_KEY".into(),
                requires_oauth: false,
                custom_headers: vec![],
                use_bearer_auth: true,
            },
        ];

        let providers = configs.into_iter().map(AiProvider::from_config).collect();
        Self { providers }
    }

    /// Look up a provider by name or alias.
    pub fn get(&self, name: &str) -> Option<&AiProvider> {
        self.providers.iter().find(|p| p.matches_name(name))
    }

    /// Get a mutable reference to a provider by name or alias.
    pub fn get_mut(&mut self, name: &str) -> Option<&mut AiProvider> {
        self.providers.iter_mut().find(|p| p.matches_name(name))
    }

    /// List all provider names (canonical).
    pub fn list_names(&self) -> Vec<&str> {
        self.providers.iter().map(|p| p.config.name.as_str()).collect()
    }

    /// List all configured providers (those with API keys).
    pub fn configured_providers(&self) -> Vec<&AiProvider> {
        self.providers.iter().filter(|p| p.is_configured()).collect()
    }

    /// List all providers.
    pub fn all_providers(&self) -> &[AiProvider] {
        &self.providers
    }

    /// Get the first configured provider (fallback for "auto" mode).
    pub fn first_configured(&self) -> Option<&AiProvider> {
        // Prefer well-known providers in order of reliability
        let preferred = ["gemini", "anthropic", "openrouter", "deepseek", "openai-codex", "xai"];
        for name in &preferred {
            if let Some(p) = self.get(name) {
                if p.is_configured() {
                    return Some(p);
                }
            }
        }
        // Fallback to any configured provider
        self.providers.iter().find(|p| p.is_configured())
    }

    /// Set an API key for a provider.
    pub fn set_api_key(&mut self, provider_name: &str, key: String) -> bool {
        if let Some(provider) = self.get_mut(provider_name) {
            provider.set_api_key(key);
            true
        } else {
            false
        }
    }
}

impl Default for ProviderRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_all_providers() {
        let reg = ProviderRegistry::new();
        assert_eq!(reg.all_providers().len(), 29);
    }

    #[test]
    fn test_lookup_by_name() {
        let reg = ProviderRegistry::new();
        assert!(reg.get("anthropic").is_some());
        assert!(reg.get("gemini").is_some());
        assert!(reg.get("deepseek").is_some());
        assert!(reg.get("nonexistent").is_none());
    }

    #[test]
    fn test_lookup_by_alias() {
        let reg = ProviderRegistry::new();
        assert!(reg.get("claude").is_some());
        assert_eq!(reg.get("claude").unwrap().config.name, "anthropic");
        assert!(reg.get("google").is_some());
        assert_eq!(reg.get("google").unwrap().config.name, "gemini");
        assert!(reg.get("grok").is_some());
        assert_eq!(reg.get("grok").unwrap().config.name, "xai");
    }

    #[test]
    fn test_provider_endpoints() {
        let reg = ProviderRegistry::new();
        let anthropic = reg.get("anthropic").unwrap();
        assert_eq!(anthropic.chat_endpoint(), "https://api.anthropic.com/v1/messages");

        let gemini = reg.get("gemini").unwrap();
        assert!(gemini.chat_endpoint().contains("chat/completions"));
    }
}
