//! Stratum Agent Protocol (SAP) — enables external agents to control the terminal.
//!
//! The SAP uses a JSON-RPC 2.0 transport over stdin/stdout. This allows
//! any process (IDE agents, AI coding assistants, automation scripts) to:
//!
//! - Execute commands in the terminal
//! - Read terminal output
//! - Query terminal state (cursor position, screen contents, selection)
//! - Subscribe to events (output, resize, exit)
//! - Switch themes and modify settings
//!
//! ## Architecture
//!
//! ```text
//! ┌──────────────┐   JSON-RPC    ┌──────────────┐
//! │  External    │◀─────────────▶│   Agent      │
//! │  Agent       │   stdin/out   │   Server     │
//! │  (e.g. IDE)  │               │              │
//! └──────────────┘               └──────┬───────┘
//!                                       │ mpsc
//!                                ┌──────▼───────┐
//!                                │  AppState    │
//!                                │  (event loop)│
//!                                └──────────────┘
//! ```

pub mod handler;
pub mod protocol;
pub mod server;

pub use handler::AgentHandler;
pub use protocol::*;
pub use server::AgentServer;
