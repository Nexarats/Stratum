//! Universal Command Suggestion System.
//!
//! Provides a card-popup UI with contextual completions for **every** command.
//! Architecture:
//!
//! ```text
//! Input Tracker → SuggestionEngine → Card State → GPU Renderer
//!                       │
//!          ┌────────────┤────────────────┐
//!          │            │                │
//!    Static Layer  Dynamic Layer     AI Layer
//!   (inline_docs) (ContextProviders)  (future)
//!   flags/synopsis  fs/git/docker    predictions
//! ```
//!
//! Each provider implements `ContextProvider` and returns `CompletionItem`s.
//! The engine merges all sources, fuzzy-filters, deduplicates, and updates
//! the `SuggestionCardState` which the GPU renderer reads each frame.

pub mod types;
pub mod provider;
pub mod engine;
pub mod providers;

// Re-export main types for convenience
pub use types::{CompletionItem, CompletionKind, SuggestionCardState};
pub use provider::{ContextProvider, ProviderRegistry};
pub use engine::SuggestionEngine;
