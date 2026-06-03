//! Layout module — pane splitting and tab management.

pub mod panes;
pub mod tabs;

pub use panes::{Direction, PaneId, PaneTree};
pub use tabs::{TabId, TabManager};
