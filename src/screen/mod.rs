//! Screen module — character grid, cell, cursor, scrollback, selection, and colors.

mod grid;
pub mod selection;

pub use grid::{Cell, Color, ScreenGrid};
pub use selection::{GridPos, Selection, SelectionMode};
