//! Agent request handler — dispatches JSON-RPC requests to terminal actions.
//!
//! Each method handler receives the request params and an `AgentContext`
//! that provides access to the terminal state.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use super::protocol::*;

/// Shared context that the handler uses to interact with the terminal.
///
/// The `AgentHandler` does NOT directly own the terminal — it communicates
/// via channels. This struct holds the state needed to process requests.
pub struct AgentContext {
    /// Terminal version string.
    pub version: String,
    /// Shell command that was launched.
    pub shell: String,
    /// Current theme name.
    pub theme_name: String,
    /// Grid dimensions (cols, rows).
    pub grid_size: (usize, usize),
    /// Process ID of the shell.
    pub shell_pid: u32,
    /// Current screen content (rows of text).
    pub screen_rows: Vec<String>,
    /// Cursor position (col, row).
    pub cursor_pos: (usize, usize),
    /// Current selection text, if any.
    pub selection_text: Option<String>,
    /// Scrollback line count.
    pub scrollback_lines: usize,
}

/// Commands that the handler needs the terminal to execute.
/// These are sent back via a channel to the AppState event loop.
#[derive(Debug, Clone)]
pub enum AgentCommand {
    /// Execute a command in the PTY.
    Execute(String),
    /// Write raw text to PTY stdin.
    Write(String),
    /// Resize the terminal grid.
    Resize { cols: usize, rows: usize },
    /// Switch theme.
    SetTheme(String),
    /// Send a signal to the shell process.
    Signal(String),
    /// Clear the terminal screen.
    Clear,
    /// Shutdown the terminal.
    Shutdown,
    /// Request a state snapshot (context will be updated asynchronously).
    RequestState,
}

/// The agent request handler — stateless dispatch of JSON-RPC methods.
pub struct AgentHandler {
    /// Events the agent has subscribed to.
    pub subscriptions: HashSet<EventKind>,
    /// Shared context (updated by the terminal event loop).
    pub context: Arc<Mutex<AgentContext>>,
}

impl AgentHandler {
    /// Create a new handler with default context.
    pub fn new(version: String, shell: String, pid: u32) -> Self {
        Self {
            subscriptions: HashSet::new(),
            context: Arc::new(Mutex::new(AgentContext {
                version,
                shell,
                theme_name: "stratum-dark".into(),
                grid_size: (120, 40),
                shell_pid: pid,
                screen_rows: Vec::new(),
                cursor_pos: (0, 0),
                selection_text: None,
                scrollback_lines: 0,
            })),
        }
    }

    /// Dispatch a JSON-RPC request and return the response + optional command.
    pub fn handle(&mut self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        match req.method.as_str() {
            methods::INITIALIZE => self.handle_initialize(req),
            methods::SHUTDOWN => self.handle_shutdown(req),
            methods::EXECUTE => self.handle_execute(req),
            methods::WRITE => self.handle_write(req),
            methods::READ => self.handle_read(req),
            methods::INFO => self.handle_info(req),
            methods::GET_STATE => self.handle_get_state(req),
            methods::SET_THEME => self.handle_set_theme(req),
            methods::LIST_THEMES => self.handle_list_themes(req),
            methods::SIGNAL => self.handle_signal(req),
            methods::SUBSCRIBE => self.handle_subscribe(req),
            methods::CLEAR => self.handle_clear(req),
            methods::RESIZE => self.handle_resize(req),
            _ => (
                RpcResponse::err(
                    req.id.clone(),
                    METHOD_NOT_FOUND,
                    format!("Unknown method: {}", req.method),
                ),
                None,
            ),
        }
    }

    fn handle_initialize(&self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        let ctx = self.context.lock().unwrap();
        let result = serde_json::json!({
            "name": "stratum",
            "version": ctx.version,
            "protocol": "sap/1.0",
            "capabilities": {
                "execute": true,
                "read": true,
                "write": true,
                "resize": true,
                "themes": true,
                "subscribe": ["output", "resize", "exit"],
            }
        });
        (RpcResponse::ok(req.id.clone(), result), None)
    }

    fn handle_shutdown(&self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        (
            RpcResponse::ok(req.id.clone(), serde_json::json!({"status": "shutting_down"})),
            Some(AgentCommand::Shutdown),
        )
    }

    fn handle_execute(&self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        match serde_json::from_value::<ExecuteParams>(req.params.clone()) {
            Ok(params) => {
                let cmd = AgentCommand::Execute(params.command.clone());
                let result = serde_json::json!({
                    "status": "submitted",
                    "command": params.command,
                });
                (RpcResponse::ok(req.id.clone(), result), Some(cmd))
            }
            Err(e) => (
                RpcResponse::err(req.id.clone(), INVALID_PARAMS, format!("Invalid params: {}", e)),
                None,
            ),
        }
    }

    fn handle_write(&self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        match serde_json::from_value::<WriteParams>(req.params.clone()) {
            Ok(params) => {
                let cmd = AgentCommand::Write(params.text);
                (
                    RpcResponse::ok(req.id.clone(), serde_json::json!({"status": "written"})),
                    Some(cmd),
                )
            }
            Err(e) => (
                RpcResponse::err(req.id.clone(), INVALID_PARAMS, format!("Invalid params: {}", e)),
                None,
            ),
        }
    }

    fn handle_read(&self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        let ctx = self.context.lock().unwrap();
        let rows = if let Ok(params) = serde_json::from_value::<ReadParams>(req.params.clone()) {
            let start = params.offset;
            let count = if params.rows == 0 { ctx.screen_rows.len() } else { params.rows };
            ctx.screen_rows
                .iter()
                .skip(start)
                .take(count)
                .cloned()
                .collect::<Vec<_>>()
        } else {
            ctx.screen_rows.clone()
        };

        let content = ScreenContent {
            rows,
            cursor_col: ctx.cursor_pos.0,
            cursor_row: ctx.cursor_pos.1,
            width: ctx.grid_size.0,
            height: ctx.grid_size.1,
        };
        (
            RpcResponse::ok(req.id.clone(), serde_json::to_value(content).unwrap()),
            None,
        )
    }

    fn handle_info(&self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        let ctx = self.context.lock().unwrap();
        let info = TerminalInfo {
            version: ctx.version.clone(),
            cols: ctx.grid_size.0,
            rows: ctx.grid_size.1,
            shell: ctx.shell.clone(),
            theme: ctx.theme_name.clone(),
            pid: ctx.shell_pid,
        };
        (
            RpcResponse::ok(req.id.clone(), serde_json::to_value(info).unwrap()),
            None,
        )
    }

    fn handle_get_state(&self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        let ctx = self.context.lock().unwrap();
        let state = TerminalState {
            cursor_col: ctx.cursor_pos.0,
            cursor_row: ctx.cursor_pos.1,
            cols: ctx.grid_size.0,
            rows: ctx.grid_size.1,
            title: String::new(),
            selection: ctx.selection_text.clone(),
            scrollback_lines: ctx.scrollback_lines,
        };
        (
            RpcResponse::ok(req.id.clone(), serde_json::to_value(state).unwrap()),
            None,
        )
    }

    fn handle_set_theme(&mut self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        match serde_json::from_value::<SetThemeParams>(req.params.clone()) {
            Ok(params) => {
                let name = params.name.clone();
                // Validate theme exists
                let resolved = crate::config::theme::get_theme(&name);
                let mut ctx = self.context.lock().unwrap();
                ctx.theme_name = resolved.name.to_string();
                (
                    RpcResponse::ok(req.id.clone(), serde_json::json!({"theme": resolved.name})),
                    Some(AgentCommand::SetTheme(name)),
                )
            }
            Err(e) => (
                RpcResponse::err(req.id.clone(), INVALID_PARAMS, format!("Invalid params: {}", e)),
                None,
            ),
        }
    }

    fn handle_list_themes(&self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        let ctx = self.context.lock().unwrap();
        let names = crate::config::theme::theme_names();
        (
            RpcResponse::ok(req.id.clone(), serde_json::json!({
                "themes": names,
                "active": ctx.theme_name,
            })),
            None,
        )
    }

    fn handle_signal(&self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        match serde_json::from_value::<SignalParams>(req.params.clone()) {
            Ok(params) => (
                RpcResponse::ok(req.id.clone(), serde_json::json!({"status": "sent", "signal": params.signal})),
                Some(AgentCommand::Signal(params.signal)),
            ),
            Err(e) => (
                RpcResponse::err(req.id.clone(), INVALID_PARAMS, format!("Invalid params: {}", e)),
                None,
            ),
        }
    }

    fn handle_subscribe(&mut self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        match serde_json::from_value::<SubscribeParams>(req.params.clone()) {
            Ok(params) => {
                let mut subscribed = Vec::new();
                for event_name in &params.events {
                    if event_name == "all" {
                        self.subscriptions.insert(EventKind::Output);
                        self.subscriptions.insert(EventKind::Resize);
                        self.subscriptions.insert(EventKind::Exit);
                        subscribed = vec!["output", "resize", "exit"];
                        break;
                    }
                    if let Some(kind) = EventKind::from_str(event_name) {
                        self.subscriptions.insert(kind);
                        subscribed.push(event_name.as_str());
                    }
                }
                (
                    RpcResponse::ok(req.id.clone(), serde_json::json!({"subscribed": subscribed})),
                    None,
                )
            }
            Err(e) => (
                RpcResponse::err(req.id.clone(), INVALID_PARAMS, format!("Invalid params: {}", e)),
                None,
            ),
        }
    }

    fn handle_clear(&self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        (
            RpcResponse::ok(req.id.clone(), serde_json::json!({"status": "cleared"})),
            Some(AgentCommand::Clear),
        )
    }

    fn handle_resize(&self, req: &RpcRequest) -> (RpcResponse, Option<AgentCommand>) {
        match serde_json::from_value::<ResizeParams>(req.params.clone()) {
            Ok(params) => (
                RpcResponse::ok(req.id.clone(), serde_json::json!({
                    "cols": params.cols,
                    "rows": params.rows,
                })),
                Some(AgentCommand::Resize { cols: params.cols, rows: params.rows }),
            ),
            Err(e) => (
                RpcResponse::err(req.id.clone(), INVALID_PARAMS, format!("Invalid params: {}", e)),
                None,
            ),
        }
    }

    /// Check if the agent is subscribed to a given event.
    pub fn is_subscribed(&self, kind: &EventKind) -> bool {
        self.subscriptions.contains(kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_handler() -> AgentHandler {
        AgentHandler::new("0.1.0".into(), "bash".into(), 1234)
    }

    fn make_req(method: &str, params: serde_json::Value) -> RpcRequest {
        RpcRequest {
            jsonrpc: "2.0".into(),
            id: Some(RpcId::Num(1)),
            method: method.into(),
            params,
        }
    }

    #[test]
    fn test_initialize() {
        let mut handler = make_handler();
        let req = make_req(methods::INITIALIZE, serde_json::json!({}));
        let (resp, cmd) = handler.handle(&req);
        assert!(resp.result.is_some());
        assert!(cmd.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["protocol"], "sap/1.0");
    }

    #[test]
    fn test_execute() {
        let mut handler = make_handler();
        let req = make_req(methods::EXECUTE, serde_json::json!({"command": "ls"}));
        let (resp, cmd) = handler.handle(&req);
        assert!(resp.result.is_some());
        assert!(matches!(cmd, Some(AgentCommand::Execute(s)) if s == "ls"));
    }

    #[test]
    fn test_write() {
        let mut handler = make_handler();
        let req = make_req(methods::WRITE, serde_json::json!({"text": "hello\n"}));
        let (resp, cmd) = handler.handle(&req);
        assert!(resp.result.is_some());
        assert!(matches!(cmd, Some(AgentCommand::Write(s)) if s == "hello\n"));
    }

    #[test]
    fn test_info() {
        let mut handler = make_handler();
        let req = make_req(methods::INFO, serde_json::json!({}));
        let (resp, cmd) = handler.handle(&req);
        assert!(cmd.is_none());
        let result = resp.result.unwrap();
        assert_eq!(result["shell"], "bash");
        assert_eq!(result["pid"], 1234);
    }

    #[test]
    fn test_list_themes() {
        let mut handler = make_handler();
        let req = make_req(methods::LIST_THEMES, serde_json::json!({}));
        let (resp, _) = handler.handle(&req);
        let result = resp.result.unwrap();
        let themes = result["themes"].as_array().unwrap();
        assert!(themes.len() >= 6);
    }

    #[test]
    fn test_set_theme() {
        let mut handler = make_handler();
        let req = make_req(methods::SET_THEME, serde_json::json!({"name": "dracula"}));
        let (resp, cmd) = handler.handle(&req);
        assert!(resp.result.is_some());
        assert!(matches!(cmd, Some(AgentCommand::SetTheme(s)) if s == "dracula"));
        let result = resp.result.unwrap();
        assert_eq!(result["theme"], "dracula");
    }

    #[test]
    fn test_subscribe() {
        let mut handler = make_handler();
        let req = make_req(methods::SUBSCRIBE, serde_json::json!({"events": ["output", "exit"]}));
        let (resp, _) = handler.handle(&req);
        assert!(resp.result.is_some());
        assert!(handler.is_subscribed(&EventKind::Output));
        assert!(handler.is_subscribed(&EventKind::Exit));
        assert!(!handler.is_subscribed(&EventKind::Resize));
    }

    #[test]
    fn test_subscribe_all() {
        let mut handler = make_handler();
        let req = make_req(methods::SUBSCRIBE, serde_json::json!({"events": ["all"]}));
        handler.handle(&req);
        assert!(handler.is_subscribed(&EventKind::Output));
        assert!(handler.is_subscribed(&EventKind::Resize));
        assert!(handler.is_subscribed(&EventKind::Exit));
    }

    #[test]
    fn test_unknown_method() {
        let mut handler = make_handler();
        let req = make_req("foo/bar", serde_json::json!({}));
        let (resp, _) = handler.handle(&req);
        assert!(resp.error.is_some());
        assert_eq!(resp.error.unwrap().code, METHOD_NOT_FOUND);
    }

    #[test]
    fn test_shutdown() {
        let mut handler = make_handler();
        let req = make_req(methods::SHUTDOWN, serde_json::json!({}));
        let (_, cmd) = handler.handle(&req);
        assert!(matches!(cmd, Some(AgentCommand::Shutdown)));
    }

    #[test]
    fn test_clear() {
        let mut handler = make_handler();
        let req = make_req(methods::CLEAR, serde_json::json!({}));
        let (_, cmd) = handler.handle(&req);
        assert!(matches!(cmd, Some(AgentCommand::Clear)));
    }

    #[test]
    fn test_read_screen() {
        let mut handler = make_handler();
        // Populate screen content
        {
            let mut ctx = handler.context.lock().unwrap();
            ctx.screen_rows = vec!["line1".into(), "line2".into(), "line3".into()];
            ctx.cursor_pos = (5, 1);
        }
        let req = make_req(methods::READ, serde_json::json!({"rows": 2, "offset": 0}));
        let (resp, _) = handler.handle(&req);
        let result = resp.result.unwrap();
        let rows = result["rows"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], "line1");
    }
}
