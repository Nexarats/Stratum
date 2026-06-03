//! SAP JSON-RPC 2.0 protocol message definitions.
//!
//! All messages conform to JSON-RPC 2.0 spec with Stratum-specific methods.

use serde::{Deserialize, Serialize};

// =============================================================================
// JSON-RPC Envelope
// =============================================================================

/// Inbound JSON-RPC request from the agent.
#[derive(Debug, Deserialize)]
pub struct RpcRequest {
    pub jsonrpc: String,
    pub id: Option<RpcId>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Outbound JSON-RPC response to the agent.
#[derive(Debug, Serialize)]
pub struct RpcResponse {
    pub jsonrpc: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<RpcId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

/// Outbound JSON-RPC notification (server → agent, no id).
#[derive(Debug, Serialize)]
pub struct RpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: serde_json::Value,
}

/// JSON-RPC error object.
#[derive(Debug, Serialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<serde_json::Value>,
}

/// JSON-RPC id — can be a number or string.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcId {
    Num(i64),
    Str(String),
}

// =============================================================================
// Error Codes (JSON-RPC 2.0 + Custom)
// =============================================================================

/// Standard JSON-RPC error codes.
pub const PARSE_ERROR: i32 = -32700;
pub const INVALID_REQUEST: i32 = -32600;
pub const METHOD_NOT_FOUND: i32 = -32601;
pub const INVALID_PARAMS: i32 = -32602;
pub const INTERNAL_ERROR: i32 = -32603;

/// Custom Stratum error codes.
pub const TERMINAL_BUSY: i32 = -1001;
pub const PTY_ERROR: i32 = -1002;

// =============================================================================
// Request Parameters
// =============================================================================

/// Parameters for `terminal/execute`.
#[derive(Debug, Deserialize)]
pub struct ExecuteParams {
    /// Command text to execute.
    pub command: String,
    /// If true, wait for the command to complete and return output.
    #[serde(default)]
    pub wait: bool,
    /// Maximum time to wait (milliseconds). 0 = no limit.
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
}

/// Parameters for `terminal/write`.
#[derive(Debug, Deserialize)]
pub struct WriteParams {
    /// Raw text to write to PTY stdin.
    pub text: String,
}

/// Parameters for `terminal/read`.
#[derive(Debug, Deserialize)]
pub struct ReadParams {
    /// Number of rows to read from the screen (0 = all visible rows).
    #[serde(default)]
    pub rows: usize,
    /// Starting row offset (0 = top of visible area).
    #[serde(default)]
    pub offset: usize,
}

/// Parameters for `terminal/resize`.
#[derive(Debug, Deserialize)]
pub struct ResizeParams {
    pub cols: usize,
    pub rows: usize,
}

/// Parameters for `terminal/setTheme`.
#[derive(Debug, Deserialize)]
pub struct SetThemeParams {
    pub name: String,
}

/// Parameters for `terminal/subscribe`.
#[derive(Debug, Deserialize)]
pub struct SubscribeParams {
    /// Events to subscribe to: "output", "resize", "exit", "all".
    pub events: Vec<String>,
}

/// Parameters for `terminal/signal`.
#[derive(Debug, Deserialize)]
pub struct SignalParams {
    /// Signal name: "SIGINT", "SIGTERM", "SIGKILL", etc.
    pub signal: String,
}

// =============================================================================
// Response Results
// =============================================================================

/// Result for `terminal/info`.
#[derive(Debug, Serialize)]
pub struct TerminalInfo {
    pub version: String,
    pub cols: usize,
    pub rows: usize,
    pub shell: String,
    pub theme: String,
    pub pid: u32,
}

/// Result for `terminal/read`.
#[derive(Debug, Serialize)]
pub struct ScreenContent {
    pub rows: Vec<String>,
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub width: usize,
    pub height: usize,
}

/// Result for `terminal/execute` with wait=true.
#[derive(Debug, Serialize)]
pub struct ExecuteResult {
    pub output: String,
    pub exit_code: Option<i32>,
}

/// Result for `terminal/getState`.
#[derive(Debug, Serialize)]
pub struct TerminalState {
    pub cursor_col: usize,
    pub cursor_row: usize,
    pub cols: usize,
    pub rows: usize,
    pub title: String,
    pub selection: Option<String>,
    pub scrollback_lines: usize,
}

// =============================================================================
// Event Notifications (server → agent)
// =============================================================================

/// Event types that agents can subscribe to.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum EventKind {
    /// New output appeared on the terminal.
    Output,
    /// Terminal was resized.
    Resize,
    /// Shell process exited.
    Exit,
}

impl EventKind {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "output" => Some(Self::Output),
            "resize" => Some(Self::Resize),
            "exit" => Some(Self::Exit),
            _ => None,
        }
    }

    pub fn method_name(&self) -> &'static str {
        match self {
            Self::Output => "event/output",
            Self::Resize => "event/resize",
            Self::Exit => "event/exit",
        }
    }
}

/// Output event data.
#[derive(Debug, Serialize)]
pub struct OutputEvent {
    pub text: String,
}

/// Resize event data.
#[derive(Debug, Serialize)]
pub struct ResizeEvent {
    pub cols: usize,
    pub rows: usize,
}

/// Exit event data.
#[derive(Debug, Serialize)]
pub struct ExitEvent {
    pub code: Option<i32>,
}

// =============================================================================
// Helpers
// =============================================================================

fn default_timeout() -> u64 {
    30_000
}

impl RpcResponse {
    /// Create a success response.
    pub fn ok(id: Option<RpcId>, result: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    /// Create an error response.
    pub fn err(id: Option<RpcId>, code: i32, message: impl Into<String>) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            id,
            result: None,
            error: Some(RpcError {
                code,
                message: message.into(),
                data: None,
            }),
        }
    }
}

impl RpcNotification {
    /// Create a notification (no id, no response expected).
    pub fn new(method: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            jsonrpc: "2.0".into(),
            method: method.into(),
            params,
        }
    }
}

// =============================================================================
// SAP Method Constants
// =============================================================================

/// All supported SAP methods.
pub mod methods {
    pub const INITIALIZE: &str = "initialize";
    pub const SHUTDOWN: &str = "shutdown";
    pub const EXECUTE: &str = "terminal/execute";
    pub const WRITE: &str = "terminal/write";
    pub const READ: &str = "terminal/read";
    pub const RESIZE: &str = "terminal/resize";
    pub const INFO: &str = "terminal/info";
    pub const GET_STATE: &str = "terminal/getState";
    pub const SET_THEME: &str = "terminal/setTheme";
    pub const LIST_THEMES: &str = "terminal/listThemes";
    pub const SIGNAL: &str = "terminal/signal";
    pub const SUBSCRIBE: &str = "terminal/subscribe";
    pub const CLEAR: &str = "terminal/clear";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rpc_response_ok() {
        let resp = RpcResponse::ok(
            Some(RpcId::Num(1)),
            serde_json::json!({"status": "ok"}),
        );
        assert!(resp.error.is_none());
        assert!(resp.result.is_some());
    }

    #[test]
    fn test_rpc_response_err() {
        let resp = RpcResponse::err(
            Some(RpcId::Num(2)),
            METHOD_NOT_FOUND,
            "Method not found",
        );
        assert!(resp.result.is_none());
        let err = resp.error.unwrap();
        assert_eq!(err.code, METHOD_NOT_FOUND);
    }

    #[test]
    fn test_rpc_notification() {
        let notif = RpcNotification::new(
            "event/output",
            serde_json::json!({"text": "hello"}),
        );
        assert_eq!(notif.method, "event/output");
    }

    #[test]
    fn test_event_kind_parse() {
        assert_eq!(EventKind::from_str("output"), Some(EventKind::Output));
        assert_eq!(EventKind::from_str("resize"), Some(EventKind::Resize));
        assert_eq!(EventKind::from_str("exit"), Some(EventKind::Exit));
        assert_eq!(EventKind::from_str("invalid"), None);
    }

    #[test]
    fn test_deserialize_execute_params() {
        let json = r#"{"command": "ls -la", "wait": true, "timeout_ms": 5000}"#;
        let params: ExecuteParams = serde_json::from_str(json).unwrap();
        assert_eq!(params.command, "ls -la");
        assert!(params.wait);
        assert_eq!(params.timeout_ms, 5000);
    }

    #[test]
    fn test_deserialize_rpc_request() {
        let json = r#"{"jsonrpc": "2.0", "id": 1, "method": "terminal/execute", "params": {"command": "pwd"}}"#;
        let req: RpcRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.method, "terminal/execute");
        assert!(matches!(req.id, Some(RpcId::Num(1))));
    }

    #[test]
    fn test_serialize_terminal_info() {
        let info = TerminalInfo {
            version: "0.1.0".into(),
            cols: 120,
            rows: 40,
            shell: "pwsh.exe".into(),
            theme: "stratum-dark".into(),
            pid: 12345,
        };
        let json = serde_json::to_string(&info).unwrap();
        assert!(json.contains("stratum-dark"));
    }
}
