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

    /// Process all pending PTY output.
    /// Returns true if there was any output to process.
    pub fn process_pty_output(&mut self) -> bool {
        let mut had_output = false;

        while let Ok(event) = self.pty_rx.try_recv() {
            match event {
                PtyEvent::Output(data) => {
                    for byte in &data {
                        self.parser.advance(*byte, &mut self.screen);
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
}

