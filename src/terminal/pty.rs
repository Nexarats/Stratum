//! PTY session management — spawning shells and communicating with them.

use anyhow::{Context, Result};
use portable_pty::{native_pty_system, CommandBuilder, MasterPty, PtySize};
use std::io::{Read, Write};

/// Manages a pseudo-terminal session with a child shell process.
pub struct PtySession {
    master: Box<dyn MasterPty + Send>,
    writer: Box<dyn Write + Send>,
}

impl PtySession {
    /// Spawn a new PTY session with the given shell command.
    pub fn new(shell: &str, cols: u16, rows: u16, is_nos_shell: bool) -> Result<Self> {
        let pty_system = native_pty_system();

        let pair = pty_system
            .openpty(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to open PTY")?;

        // Build command
        let mut cmd = CommandBuilder::new(shell);
        cmd.env("TERM", "xterm-256color");

        // If launching NOS Shell, enable IPC protocol
        if is_nos_shell {
            cmd.env("STRATUM_IPC", "1");
        }

        // Spawn the shell as a child process
        let _child = pair
            .slave
            .spawn_command(cmd)
            .context("Failed to spawn shell")?;

        // Drop the slave side — we only use the master
        drop(pair.slave);

        let writer = pair
            .master
            .take_writer()
            .context("Failed to take PTY writer")?;

        Ok(Self {
            master: pair.master,
            writer,
        })
    }

    /// Take the reader from the PTY master.
    /// This can only be called once — the reader is consumed.
    pub fn take_reader(&mut self) -> Result<Box<dyn Read + Send>> {
        self.master
            .try_clone_reader()
            .context("Failed to clone PTY reader")
    }

    /// Write bytes to the PTY (sends input to the shell).
    pub fn write(&mut self, data: &[u8]) -> Result<()> {
        self.writer
            .write_all(data)
            .context("Failed to write to PTY")?;
        self.writer.flush().context("Failed to flush PTY writer")?;
        Ok(())
    }

    /// Resize the PTY to the given dimensions.
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.master
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .context("Failed to resize PTY")?;
        Ok(())
    }
}
