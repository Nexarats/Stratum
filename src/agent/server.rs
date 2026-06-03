//! Agent stdio server — reads JSON-RPC requests from stdin, writes responses to stdout.
//!
//! The server runs in a dedicated thread (not async) to avoid blocking the winit
//! event loop. It communicates with the terminal via mpsc channels.

use std::io::{self, BufRead, Write};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::thread;

use super::handler::{AgentCommand, AgentHandler};
use super::protocol::*;

/// Messages sent from the AgentServer thread to the terminal event loop.
#[derive(Debug)]
pub enum AgentMsg {
    /// A command for the terminal to execute.
    Command(AgentCommand),
    /// Agent server has stopped.
    Disconnected,
}

/// The agent server manages the lifecycle of a single agent connection.
pub struct AgentServer {
    /// Channel to send commands to the terminal event loop.
    cmd_tx: mpsc::Sender<AgentMsg>,
    /// Channel to receive state updates from the terminal.
    state_rx: mpsc::Receiver<AgentStateUpdate>,
    /// The request handler.
    handler: AgentHandler,
}

/// State updates pushed from the terminal to the agent server.
#[derive(Debug, Clone)]
pub enum AgentStateUpdate {
    /// Screen content changed.
    ScreenUpdate {
        rows: Vec<String>,
        cursor_col: usize,
        cursor_row: usize,
        cols: usize,
        height: usize,
    },
    /// Terminal resized.
    Resized { cols: usize, rows: usize },
    /// Terminal exited.
    Exited { code: Option<i32> },
    /// Theme changed.
    ThemeChanged { name: String },
}

impl AgentServer {
    /// Create a new server with channels for bidirectional communication.
    ///
    /// Returns `(server, cmd_receiver, state_sender)` — the caller (AppState)
    /// keeps the receiver and sender to communicate with the agent.
    pub fn create(
        version: String,
        shell: String,
        pid: u32,
    ) -> (
        Self,
        mpsc::Receiver<AgentMsg>,
        mpsc::Sender<AgentStateUpdate>,
    ) {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (state_tx, state_rx) = mpsc::channel();

        let handler = AgentHandler::new(version, shell, pid);

        let server = Self {
            cmd_tx,
            state_rx,
            handler,
        };

        (server, cmd_rx, state_tx)
    }

    /// Get a clone of the handler's context for external updates.
    pub fn context(&self) -> Arc<Mutex<super::handler::AgentContext>> {
        Arc::clone(&self.handler.context)
    }

    /// Run the agent server on a dedicated thread.
    ///
    /// Reads JSON-RPC lines from stdin, dispatches them, writes responses to stdout.
    /// This blocks the calling thread — call from `thread::spawn`.
    pub fn run(mut self) {
        let stdin = io::stdin();
        let stdout = io::stdout();
        let reader = stdin.lock();
        let mut writer = stdout.lock();

        tracing::info!("SAP agent server started — reading from stdin");

        // Write initial ready notification
        let ready = RpcNotification::new(
            "server/ready",
            serde_json::json!({
                "protocol": "sap/1.0",
                "name": "stratum",
            }),
        );
        if let Ok(json) = serde_json::to_string(&ready) {
            let _ = writeln!(writer, "{}", json);
            let _ = writer.flush();
        }

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!("Agent stdin read error: {}", e);
                    break;
                }
            };

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // Process any pending state updates before handling the request
            self.drain_state_updates();

            // Parse JSON-RPC request
            let req: RpcRequest = match serde_json::from_str(trimmed) {
                Ok(r) => r,
                Err(e) => {
                    let err_resp = RpcResponse::err(
                        None,
                        PARSE_ERROR,
                        format!("Failed to parse JSON-RPC: {}", e),
                    );
                    if let Ok(json) = serde_json::to_string(&err_resp) {
                        let _ = writeln!(writer, "{}", json);
                        let _ = writer.flush();
                    }
                    continue;
                }
            };

            tracing::debug!("Agent request: {} (id={:?})", req.method, req.id);

            // Dispatch
            let (response, command) = self.handler.handle(&req);

            // Write response
            if req.id.is_some() {
                if let Ok(json) = serde_json::to_string(&response) {
                    let _ = writeln!(writer, "{}", json);
                    let _ = writer.flush();
                }
            }

            // Forward command to terminal
            if let Some(cmd) = command {
                let is_shutdown = matches!(cmd, AgentCommand::Shutdown);
                if self.cmd_tx.send(AgentMsg::Command(cmd)).is_err() {
                    tracing::error!("Terminal channel disconnected");
                    break;
                }
                if is_shutdown {
                    tracing::info!("Agent requested shutdown");
                    break;
                }
            }
        }

        tracing::info!("SAP agent server stopped");
        let _ = self.cmd_tx.send(AgentMsg::Disconnected);
    }

    /// Drain pending state updates and update handler context.
    fn drain_state_updates(&mut self) {
        while let Ok(update) = self.state_rx.try_recv() {
            let mut ctx = self.handler.context.lock().unwrap();
            match update {
                AgentStateUpdate::ScreenUpdate {
                    rows,
                    cursor_col,
                    cursor_row,
                    cols,
                    height,
                } => {
                    ctx.screen_rows = rows;
                    ctx.cursor_pos = (cursor_col, cursor_row);
                    ctx.grid_size = (cols, height);
                }
                AgentStateUpdate::Resized { cols, rows } => {
                    ctx.grid_size = (cols, rows);
                }
                AgentStateUpdate::Exited { code: _ } => {
                    // Handled externally
                }
                AgentStateUpdate::ThemeChanged { name } => {
                    ctx.theme_name = name;
                }
            }
        }
    }
}

/// Convenience function to spawn the agent server on a background thread.
///
/// Returns `(cmd_receiver, state_sender)` for the terminal to communicate with.
pub fn spawn_agent_server(
    version: String,
    shell: String,
    pid: u32,
) -> (mpsc::Receiver<AgentMsg>, mpsc::Sender<AgentStateUpdate>) {
    let (server, cmd_rx, state_tx) = AgentServer::create(version, shell, pid);

    thread::Builder::new()
        .name("sap-agent-server".into())
        .spawn(move || server.run())
        .expect("Failed to spawn agent server thread");

    (cmd_rx, state_tx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_server() {
        let (server, _cmd_rx, _state_tx) =
            AgentServer::create("0.1.0".into(), "bash".into(), 1234);
        let ctx = server.context();
        let locked = ctx.lock().unwrap();
        assert_eq!(locked.version, "0.1.0");
        assert_eq!(locked.shell, "bash");
        assert_eq!(locked.shell_pid, 1234);
    }

    #[test]
    fn test_state_update() {
        let (mut server, _cmd_rx, state_tx) =
            AgentServer::create("0.1.0".into(), "bash".into(), 1234);

        state_tx
            .send(AgentStateUpdate::Resized { cols: 200, rows: 60 })
            .unwrap();

        server.drain_state_updates();

        let ctx = server.handler.context.lock().unwrap();
        assert_eq!(ctx.grid_size, (200, 60));
    }

    #[test]
    fn test_theme_update() {
        let (mut server, _cmd_rx, state_tx) =
            AgentServer::create("0.1.0".into(), "bash".into(), 1234);

        state_tx
            .send(AgentStateUpdate::ThemeChanged {
                name: "dracula".into(),
            })
            .unwrap();

        server.drain_state_updates();

        let ctx = server.handler.context.lock().unwrap();
        assert_eq!(ctx.theme_name, "dracula");
    }
}
