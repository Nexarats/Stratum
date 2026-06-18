#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
#![allow(dead_code, unused_imports, unused_mut, unused_variables)]

//! # Stratum Terminal
//!
//! The terminal that understands what you're doing.
//!
//! Stratum is a GPU-rendered, AI-aware terminal emulator that models
//! every interaction as typed, queryable, composable artifacts.
//!
//! Part of the NOS (Nexarats Operating System) project.
//! https://github.com/nexarats/stratum

mod agent;
mod ai;
mod app;
mod clipboard;
mod config;
mod errors;
mod features;
mod input;
mod layout;
mod parser;
mod renderer;
mod screen;
mod suggestions;
mod talk;
mod terminal;

use anyhow::Result;
use clap::Parser;
use tracing_subscriber::EnvFilter;

/// Stratum — The terminal that understands what you're doing.
#[derive(Parser, Debug)]
#[command(name = "stratum", version, about, long_about = None)]
struct Cli {
    /// Shell command to execute (default: user's login shell or /bin/bash)
    #[arg(short, long)]
    execute: Option<String>,

    /// Shell backend: "nos" for NOS Shell, "system" for system default
    #[arg(long, default_value = "system")]
    shell: String,

    /// Configuration file path
    #[arg(short, long)]
    config: Option<String>,

    /// Enable debug logging
    #[arg(long)]
    debug: bool,

    /// Run in agent mode (headless, JSON-RPC over stdin/stdout)
    #[arg(long)]
    agent_mode: bool,

    /// Font size in points
    #[arg(long, default_value = "14.0")]
    font_size: Option<f32>,

    /// Talk mode — AI agent in your current terminal (no GPU needed)
    #[arg(long)]
    talk: bool,

    /// Run in GUI mode (GPU-rendered window terminal)
    #[arg(long)]
    gui: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize structured logging
    let filter = if cli.debug {
        EnvFilter::new("stratum=debug,wgpu=warn")
    } else {
        EnvFilter::new("stratum=info,wgpu=error")
    };

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .with_thread_ids(true)
        .init();

    tracing::info!(
        "Stratum v{} — The terminal that understands what you're doing",
        env!("CARGO_PKG_VERSION")
    );

    // By default, if not explicitly requesting GUI mode or agent mode, start talk mode directly.
    if cli.talk || (!cli.gui && !cli.agent_mode) {
        return talk::run_talk_mode(cli.debug);
    }

    // Load configuration
    let mut settings = config::Settings::load(cli.config.as_deref())?;

    // CLI overrides
    if let Some(font_size) = cli.font_size {
        settings.font_size = font_size;
    }

    // Determine shell to launch
    let shell = if let Some(exec) = cli.execute {
        exec
    } else if cli.shell == "nos" || settings.shell == "nos" {
        // Find NOS Shell binary
        find_nos_shell().unwrap_or_else(|| {
            tracing::warn!("NOS Shell not found, falling back to system shell");
            default_system_shell()
        })
    } else {
        settings.shell_path.clone().unwrap_or_else(|| {
            std::env::var("SHELL").unwrap_or_else(|_| default_system_shell())
        })
    };

    tracing::info!("Shell: {}", shell);

    // Create and run the terminal application
    let app = app::TerminalApp::new(settings, &shell, cli.agent_mode);
    app.run()?;

    tracing::info!("Stratum shutdown complete");
    Ok(())
}

/// Helper to check if an executable exists in PATH.
fn has_command(cmd: &str) -> bool {
    std::process::Command::new(if cfg!(windows) { "where" } else { "which" })
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Get the default system shell.
fn default_system_shell() -> String {
    if cfg!(windows) {
        // Prefer PowerShell 7+ (pwsh) > PowerShell 5.1 > cmd.exe
        if has_command("pwsh.exe") {
            String::from("pwsh.exe")
        } else if has_command("powershell.exe") {
            String::from("powershell.exe")
        } else {
            String::from("cmd.exe")
        }
    } else {
        String::from("/bin/bash")
    }
}

/// Find the NOS Shell binary.
/// Looks relative to the stratum binary, then in PATH, then relative to cwd.
fn find_nos_shell() -> Option<String> {
    let nos_bin = if cfg!(windows) { "nos-shell.exe" } else { "nos-shell" };

    // Check next to the stratum binary (co-located install)
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let nos_shell = dir.join(nos_bin);
            if nos_shell.exists() {
                tracing::info!("Found NOS Shell next to stratum: {}", nos_shell.display());
                return Some(nos_shell.to_string_lossy().to_string());
            }

            // Check sibling crate's target directory
            // stratum.exe is at: NOS/stratum/target/release/stratum.exe
            // nos-shell.exe at: NOS/nos-shell/target/release/nos-shell.exe
            // So we go up 3 levels (release -> target -> stratum) to reach NOS/
            for depth in &["../..", "../../..", "../../../.."] {
                for profile in &["release", "debug"] {
                    let sibling = dir.join(depth).join("nos-shell").join("target").join(profile).join(nos_bin);
                    if sibling.exists() {
                        let canonical = sibling.canonicalize().unwrap_or(sibling);
                        tracing::info!("Found NOS Shell sibling: {}", canonical.display());
                        return Some(canonical.to_string_lossy().to_string());
                    }
                }
            }
        }
    }

    // Check relative to current working directory
    if let Ok(cwd) = std::env::current_dir() {
        for path in &[
            "nos-shell/target/release",
            "nos-shell/target/debug",
            "../nos-shell/target/release",
            "../nos-shell/target/debug",
        ] {
            let candidate = cwd.join(path).join(nos_bin);
            if candidate.exists() {
                let canonical = candidate.canonicalize().unwrap_or(candidate);
                tracing::info!("Found NOS Shell relative to cwd: {}", canonical.display());
                return Some(canonical.to_string_lossy().to_string());
            }
        }
    }

    // Check in PATH
    if let Ok(output) = std::process::Command::new(if cfg!(windows) { "where" } else { "which" })
        .arg(nos_bin)
        .output()
    {
        if output.status.success() {
            let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !path.is_empty() {
                tracing::info!("Found NOS Shell in PATH: {}", path);
                return Some(path);
            }
        }
    }

    tracing::warn!("NOS Shell binary not found anywhere");
    None
}
