//! Context Provider trait — dynamic completion sources.
//!
//! Each provider supplies contextual completions for specific commands.
//! For example, `GitProvider` supplies branches for `git checkout`,
//! `FilesystemProvider` supplies directories for `cd`, etc.

use super::types::CompletionItem;

/// Trait for providing dynamic completions for specific commands.
///
/// Implementations should be cheap to construct and should cache
/// expensive lookups (like git branch lists) with short TTLs.
pub trait ContextProvider: Send + Sync {
    /// Human-readable name for this provider (e.g., "Git", "Filesystem").
    fn name(&self) -> &str;

    /// Which base commands this provider handles.
    /// Return `&["cd", "ls", "cat"]` etc.
    fn handles(&self) -> &[&str];

    /// Check if this provider can handle the given command.
    fn can_handle(&self, command: &str) -> bool {
        self.handles().iter().any(|&h| command == h || command.ends_with(&format!("/{}", h)))
    }

    /// Get contextual completions for the current input.
    ///
    /// - `command`: The base command name (first word, e.g., "git").
    /// - `args`: All arguments typed so far (e.g., ["checkout", "fea"]).
    /// - `partial`: The partial text of the argument being typed (e.g., "fea").
    /// - `cwd`: Current working directory.
    fn completions(
        &self,
        command: &str,
        args: &[&str],
        partial: &str,
        cwd: &std::path::Path,
    ) -> Vec<CompletionItem>;
}

/// Registry of all context providers.
pub struct ProviderRegistry {
    providers: Vec<Box<dyn ContextProvider>>,
}

impl ProviderRegistry {
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Create a registry with all built-in providers.
    pub fn with_defaults() -> Self {
        let mut registry = Self::new();

        // Register all built-in providers
        registry.register(Box::new(super::providers::filesystem::FilesystemProvider));
        registry.register(Box::new(super::providers::git::GitProvider::new()));
        registry.register(Box::new(super::providers::docker::DockerProvider::new()));
        registry.register(Box::new(super::providers::npm::NpmProvider));
        registry.register(Box::new(super::providers::cargo::CargoProvider));
        registry.register(Box::new(super::providers::process::ProcessProvider));
        registry.register(Box::new(super::providers::ssh::SshProvider));
        registry.register(Box::new(super::providers::make::MakeProvider));
        registry.register(Box::new(super::providers::env::EnvProvider));
        registry.register(Box::new(super::providers::history::HistoryProvider::new()));
        registry.register(Box::new(super::providers::nos_shell::NosShellProvider));

        registry
    }

    /// Register a new provider.
    pub fn register(&mut self, provider: Box<dyn ContextProvider>) {
        self.providers.push(provider);
    }

    /// Get completions from all providers that handle the given command.
    pub fn get_completions(
        &self,
        command: &str,
        args: &[&str],
        partial: &str,
        cwd: &std::path::Path,
    ) -> Vec<CompletionItem> {
        let mut items = Vec::new();

        for provider in &self.providers {
            if provider.can_handle(command) {
                let mut provider_items = provider.completions(command, args, partial, cwd);
                items.append(&mut provider_items);
            }
        }

        // Also check for the HistoryProvider which handles all commands
        // (it's already registered with handles() returning &["*"])

        // Sort by priority (lower = first), then alphabetically
        items.sort_by(|a, b| {
            a.priority.cmp(&b.priority)
                .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
        });

        // Deduplicate by label
        items.dedup_by(|a, b| a.label == b.label);

        items
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

    struct TestProvider;
    impl ContextProvider for TestProvider {
        fn name(&self) -> &str { "test" }
        fn handles(&self) -> &[&str] { &["test-cmd"] }
        fn completions(
            &self, _command: &str, _args: &[&str], _partial: &str,
            _cwd: &std::path::Path,
        ) -> Vec<CompletionItem> {
            vec![CompletionItem::directory("test-dir")]
        }
    }

    #[test]
    fn test_registry_dispatch() {
        let mut registry = ProviderRegistry::new();
        registry.register(Box::new(TestProvider));

        let items = registry.get_completions("test-cmd", &[], "", std::path::Path::new("."));
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].label, "test-dir/");

        let items = registry.get_completions("other-cmd", &[], "", std::path::Path::new("."));
        assert_eq!(items.len(), 0);
    }
}
