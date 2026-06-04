//! Terminal pane — owns a PTY session, parser, and screen grid.
//!
//! Each pane represents an independent terminal session with its own
//! shell process. Multiple panes can exist simultaneously in split view.

use anyhow::{Context, Result};
use std::io::Read;
use std::sync::mpsc;

use crate::parser::AnsiParser;
use crate::screen::ScreenGrid;
use crate::terminal::PtySession;

/// Strip ANSI escape sequences from a string.
/// Handles CSI (ESC[...), OSC (ESC]...), and simple ESC sequences.
fn strip_ansi_escapes(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip ESC and the sequence that follows
            match chars.peek() {
                Some('[') => {
                    // CSI sequence: ESC[...final_byte
                    chars.next(); // skip '['
                    while let Some(&nc) = chars.peek() {
                        chars.next();
                        if nc.is_ascii_alphabetic() || nc == '@' || nc == '~' {
                            break; // final byte
                        }
                    }
                }
                Some(']') => {
                    // OSC sequence: ESC]...BEL or ESC]...ESC\\
                    chars.next(); // skip ']'
                    while let Some(&nc) = chars.peek() {
                        chars.next();
                        if nc == '\x07' || nc == '\x1b' {
                            break;
                        }
                    }
                }
                _ => {
                    // Simple escape: skip one more char
                    chars.next();
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Data produced by a pane's PTY reader thread.
pub enum PtyEvent {
    /// Raw bytes from the shell process.
    Output(Vec<u8>),
    /// Shell process exited.
    Exited,
}

/// A structured data block received from NOS Shell via IPC.
#[derive(Debug, Clone)]
pub struct StructuredBlock {
    /// Raw JSON string from NOS Shell.
    pub json: String,
}

/// An independent terminal session with its own shell, parser, and grid.
pub struct TerminalPane {
    pub pty: PtySession,
    pub parser: AnsiParser,
    pub screen: ScreenGrid,
    pub pty_rx: mpsc::Receiver<PtyEvent>,
    pub title: String,
    pub exited: bool,
    /// Whether this pane is running NOS Shell.
    pub is_nos_shell: bool,
    /// Structured data blocks received from NOS Shell (most recent first).
    pub structured_blocks: Vec<StructuredBlock>,
}

impl TerminalPane {
    /// Create a new terminal pane with the given shell and grid dimensions.
    pub fn new(shell: &str, cols: u16, rows: u16) -> Result<Self> {
        let is_nos = shell.contains("nos-shell") || shell.contains("nos_shell");
        let mut pty = PtySession::new(shell, cols, rows, is_nos)?;
        let parser = AnsiParser::new();
        let screen = ScreenGrid::new(cols as usize, rows as usize);

        // Spawn PTY reader thread
        let (tx, rx) = mpsc::channel::<PtyEvent>();
        let mut reader = pty.take_reader().context("Failed to take PTY reader")?;

        std::thread::Builder::new()
            .name("pty-reader".into())
            .spawn(move || {
                let mut buffer = [0u8; 8192];
                loop {
                    match reader.read(&mut buffer) {
                        Ok(0) => {
                            let _ = tx.send(PtyEvent::Exited);
                            break;
                        }
                        Ok(n) => {
                            if tx.send(PtyEvent::Output(buffer[..n].to_vec())).is_err() {
                                break;
                            }
                        }
                        Err(e) => {
                            tracing::debug!("PTY reader error: {}", e);
                            let _ = tx.send(PtyEvent::Exited);
                            break;
                        }
                    }
                }
            })
            .context("Failed to spawn PTY reader thread")?;

        Ok(Self {
            pty,
            parser,
            screen,
            pty_rx: rx,
            title: String::from("Terminal"),
            exited: false,
            is_nos_shell: is_nos,
            structured_blocks: Vec::new(),
        })
    }

    /// Inject the Stratum shell integration hook.
    /// This writes a PowerShell script into the PTY that intercepts
    /// slash commands (e.g. /ai-test) at the SHELL level and sends
    /// them back to Stratum via OSC escape sequences.
    pub fn inject_shell_hook(&mut self, shell: &str) {
        if shell.contains("powershell") || shell.contains("pwsh") {
            // PowerShell: Use $ExecutionContext.InvokeCommand.CommandNotFoundAction
            // This fires when PowerShell can't find a command, so /ai-test triggers it.
            //
            // IMPORTANT: We use a plain text marker "__STRATUM_CMD__:" instead of
            // OSC escape sequences because Windows ConPTY intercepts ESC sequences
            // internally instead of forwarding them to the terminal emulator.
            //
            // The marker is detected and stripped from PTY output before rendering.
            let hook = format!(
                "{}\r",
                concat!(
                    "$ExecutionContext.InvokeCommand.CommandNotFoundAction = {",
                        "param($Name,$Event) ",
                        "if($Name -like '/*'){",
                            "$cmd=$Name.Substring(1); ",
                            "$Event.StopSearch=$true; ",
                            "$Event.CommandScriptBlock=[scriptblock]::Create(",
                                "'$a=if($args){\" \"+($args-join\" \")}else{\"\"}; ",
                                "Write-Host \"__STRATUM_CMD__:'+$cmd+'$a\" '",
                            ")",
                        "}",
                    "}",
                )
            );
            tracing::info!("Injecting PowerShell shell hook");
            let _ = self.write(hook.as_bytes());
            // Clear the screen so the hook command isn't visible
            let _ = self.write(b"cls\r");
        } else if shell.contains("bash") || shell.contains("zsh") || shell.contains("sh") {
            // Bash/Zsh: Use command_not_found_handle
            let hook = concat!(
                "command_not_found_handle() { ",
                    "if [[ \"$1\" == /* ]]; then ",
                        "local cmd=\"${1:1}\"; ",
                        "printf '\\e]STRATUM;%s\\a' \"$cmd\"; ",
                        "return 0; ",
                    "fi; ",
                    "echo \"$1: command not found\"; ",
                    "return 127; ",
                "}",
                "\n"
            );
            tracing::info!("Injecting Bash/Zsh shell hook");
            let _ = self.write(hook.as_bytes());
        } else if shell.contains("cmd") {
            // CMD: No easy hook available, skip
            tracing::info!("CMD shell detected — slash commands not supported via shell hook");
        }
    }

    /// Process all pending PTY output.
    /// Returns true if there was any output to process.
    pub fn process_pty_output(&mut self) -> bool {
        let mut had_output = false;

        while let Ok(event) = self.pty_rx.try_recv() {
            match event {
                PtyEvent::Output(data) => {
                    // Scan for Stratum command marker in raw output.
                    // The PowerShell hook outputs: __STRATUM_CMD__:command args
                    // We detect this, extract the command, and strip the marker
                    // from the data before passing to the ANSI parser.
                    let text = String::from_utf8_lossy(&data);

                    if let Some(marker_pos) = text.find("__STRATUM_CMD__:") {
                        let cmd_start = marker_pos + "__STRATUM_CMD__:".len();
                        // Find end of command (newline or end of data)
                        let rest = &text[cmd_start..];
                        let cmd_end = rest.find(|c: char| c == '\n' || c == '\r')
                            .unwrap_or(rest.len());
                        let raw_cmd = rest[..cmd_end].trim().to_string();

                        // Strip ANSI escape codes (PSReadLine appends ESC[K etc.)
                        let cmd = strip_ansi_escapes(&raw_cmd);

                        // Validate: command must start with alphanumeric or hyphen
                        // (filters out false positives from the hook echo at startup)
                        let is_valid = cmd.chars().next()
                            .map(|c| c.is_alphanumeric() || c == '-')
                            .unwrap_or(false);

                        if !cmd.is_empty() && is_valid {
                            tracing::info!("STRATUM command intercepted: /{}", cmd);
                            self.parser.stratum_commands.push(cmd);
                        }

                        // Strip the marker line from data before passing to parser.
                        // Find the full line containing the marker (from line start to line end).
                        let line_start = text[..marker_pos].rfind('\n')
                            .map(|i| i + 1)
                            .unwrap_or(marker_pos);
                        let line_end_offset = cmd_start + cmd_end;
                        // Skip past trailing \r\n
                        let mut skip_end = line_end_offset;
                        let text_bytes = text.as_bytes();
                        while skip_end < text_bytes.len() && (text_bytes[skip_end] == b'\r' || text_bytes[skip_end] == b'\n') {
                            skip_end += 1;
                        }

                        // Parse everything except the marker line
                        for byte in &data[..line_start] {
                            self.parser.advance(*byte, &mut self.screen);
                        }
                        for byte in &data[skip_end..] {
                            self.parser.advance(*byte, &mut self.screen);
                        }
                    } else {
                        // No marker — pass all bytes to parser normally
                        for byte in &data {
                            self.parser.advance(*byte, &mut self.screen);
                        }
                    }
                    had_output = true;
                }
                PtyEvent::Exited => {
                    self.exited = true;
                    tracing::info!("Shell process exited");
                    had_output = true;
                }
            }
        }

        // Drain NOS Shell IPC messages
        if self.is_nos_shell {
            let messages = self.parser.take_nos_ipc();
            for json in messages {
                tracing::debug!("Storing structured block: {} bytes", json.len());
                self.structured_blocks.push(StructuredBlock { json });
                // Keep only the most recent blocks (avoid unbounded growth)
                if self.structured_blocks.len() > 20 {
                    self.structured_blocks.remove(0);
                }
            }
        }

        had_output
    }

    /// Get the most recent structured block (if any).
    pub fn latest_structured_block(&self) -> Option<&StructuredBlock> {
        self.structured_blocks.last()
    }

    /// Write bytes to the PTY (send input to the shell).
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        self.pty.write(data)
    }

    /// Resize the pane's PTY and screen grid.
    pub fn resize(&mut self, cols: u16, rows: u16) -> Result<()> {
        self.screen.resize(cols as usize, rows as usize);
        self.pty.resize(cols, rows)
    }

    /// Inject bytes into the screen display only (NOT sent to the shell).
    /// This renders text visually on the terminal grid without executing anything.
    pub fn inject_display(&mut self, data: &[u8]) {
        for byte in data {
            self.parser.advance(*byte, &mut self.screen);
        }
    }
}

