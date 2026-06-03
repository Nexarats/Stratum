//! Configuration module — user settings and themes.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::PathBuf;

pub mod theme;
pub use theme::Theme;

/// Application settings loaded from config file or defaults.
#[derive(Debug, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Font family name.
    pub font_family: String,
    /// Font size in points.
    pub font_size: f32,
    /// Scrollback buffer size (number of lines).
    pub scrollback_lines: usize,
    /// Enable GPU rendering.
    pub gpu_rendering: bool,
    /// Theme name.
    pub theme: String,
    /// Tab width in spaces.
    pub tab_width: usize,
    /// Cursor style: "block", "underline", or "bar".
    pub cursor_style: String,
    /// Cursor blink rate in milliseconds (0 = no blink).
    pub cursor_blink_ms: u64,
    /// Window opacity (0.0 to 1.0).
    pub opacity: f32,
    /// Padding in pixels.
    pub padding: u32,
    /// Shell backend: "system" or "nos".
    pub shell: String,
    /// Custom shell binary path (overrides shell detection).
    pub shell_path: Option<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            font_family: String::from("JetBrains Mono"),
            font_size: 14.0,
            scrollback_lines: 10_000,
            gpu_rendering: true,
            theme: String::from("stratum-dark"),
            tab_width: 8,
            cursor_style: String::from("block"),
            cursor_blink_ms: 500,
            opacity: 1.0,
            padding: 8,
            shell: String::from("system"),
            shell_path: None,
        }
    }
}

impl Settings {
    /// Load settings from a file, or use defaults.
    pub fn load(path: Option<&str>) -> Result<Self> {
        let config_path = if let Some(p) = path {
            PathBuf::from(p)
        } else {
            Self::default_config_path()
        };

        if config_path.exists() {
            let content = std::fs::read_to_string(&config_path)
                .with_context(|| format!("Failed to read config: {}", config_path.display()))?;
            let settings: Settings = toml::from_str(&content)
                .with_context(|| "Failed to parse config file")?;
            tracing::info!("Loaded config from {}", config_path.display());
            Ok(settings)
        } else {
            tracing::info!("Using default settings (no config file found)");
            Ok(Settings::default())
        }
    }

    /// Get the default config file path.
    fn default_config_path() -> PathBuf {
        if let Some(config_dir) = dirs::config_dir() {
            config_dir.join("stratum").join("config.toml")
        } else {
            PathBuf::from("stratum.toml")
        }
    }
}
