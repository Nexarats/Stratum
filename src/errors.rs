//! Production error types for the Stratum terminal.

use thiserror::Error;

/// Top-level errors for the Stratum application.
#[derive(Error, Debug)]
pub enum StratumError {
    #[error("PTY error: {0}")]
    Pty(#[from] PtyError),

    #[error("Renderer error: {0}")]
    Renderer(#[from] RendererError),

    #[error("Configuration error: {0}")]
    Config(#[from] ConfigError),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Errors from the PTY subsystem.
#[derive(Error, Debug)]
pub enum PtyError {
    #[error("Failed to open pseudo-terminal: {0}")]
    Open(String),

    #[error("Failed to spawn shell process '{0}': {1}")]
    SpawnFailed(String, String),

    #[error("Failed to write to PTY: {0}")]
    Write(String),

    #[error("Failed to resize PTY to {0}x{1}: {2}")]
    Resize(u16, u16, String),

    #[error("PTY reader disconnected unexpectedly")]
    ReaderDisconnected,
}

/// Errors from the GPU renderer.
#[derive(Error, Debug)]
pub enum RendererError {
    #[error("No suitable GPU adapter found. Ensure drivers are installed.")]
    NoAdapter,

    #[error("Failed to create GPU device: {0}")]
    DeviceCreation(String),

    #[error("Failed to create rendering surface: {0}")]
    SurfaceCreation(String),

    #[error("Failed to acquire swap chain frame: {0}")]
    SwapChainFrame(String),

    #[error("Shader compilation error: {0}")]
    ShaderCompilation(String),

    #[error("Font loading error: {0}")]
    FontLoading(String),

    #[error("Glyph atlas is full — cannot rasterize character '{0}'")]
    AtlasFull(char),
}

/// Errors from the configuration subsystem.
#[derive(Error, Debug)]
pub enum ConfigError {
    #[error("Failed to read config file '{path}': {source}")]
    ReadFailed {
        path: String,
        source: std::io::Error,
    },

    #[error("Failed to parse config file: {0}")]
    ParseFailed(String),

    #[error("Invalid configuration value for '{key}': {reason}")]
    InvalidValue {
        key: String,
        reason: String,
    },
}
