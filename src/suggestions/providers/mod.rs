//! Context provider implementations.
//!
//! Each provider supplies dynamic, context-aware completions
//! for specific commands.

pub mod filesystem;
pub mod git;
pub mod docker;
pub mod npm;
pub mod cargo;
pub mod process;
pub mod ssh;
pub mod make;
pub mod env;
pub mod history;
pub mod nos_shell;
