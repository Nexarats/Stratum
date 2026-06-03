//! Text selection state for the terminal grid.
//!
//! Manages rectangular and line-based selections with drag tracking.
//! Used by mouse selection and keyboard selection (Shift+Arrow).

/// A coordinate in the terminal grid.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GridPos {
    pub col: usize,
    pub row: usize,
}

impl GridPos {
    pub fn new(col: usize, row: usize) -> Self {
        Self { col, row }
    }

    /// Order two positions so `start` is before `end` in reading order.
    pub fn ordered(a: GridPos, b: GridPos) -> (GridPos, GridPos) {
        if a.row < b.row || (a.row == b.row && a.col <= b.col) {
            (a, b)
        } else {
            (b, a)
        }
    }
}

/// Selection mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionMode {
    /// Normal character-level selection (click + drag).
    Normal,
    /// Word selection (double-click).
    Word,
    /// Line selection (triple-click).
    Line,
}

/// Tracks the current selection state in the terminal.
#[derive(Debug, Clone)]
pub struct Selection {
    /// Where the selection started (anchor).
    pub anchor: GridPos,
    /// Where the selection currently ends (follows cursor/mouse).
    pub extent: GridPos,
    /// Selection mode.
    pub mode: SelectionMode,
    /// Whether a selection is currently active.
    pub active: bool,
    /// Whether the user is currently dragging (mouse button held).
    pub dragging: bool,
}

impl Selection {
    pub fn new() -> Self {
        Self {
            anchor: GridPos::new(0, 0),
            extent: GridPos::new(0, 0),
            mode: SelectionMode::Normal,
            active: false,
            dragging: false,
        }
    }

    /// Start a new selection at the given position.
    pub fn start(&mut self, pos: GridPos, mode: SelectionMode) {
        self.anchor = pos;
        self.extent = pos;
        self.mode = mode;
        self.active = true;
        self.dragging = true;
    }

    /// Update the extent of the current selection (mouse moved).
    pub fn update(&mut self, pos: GridPos) {
        if self.dragging {
            self.extent = pos;
        }
    }

    /// Finalize the selection (mouse released).
    pub fn finish(&mut self) {
        self.dragging = false;
        // If anchor == extent and mode is Normal, there's no selection
        if self.mode == SelectionMode::Normal && self.anchor == self.extent {
            self.active = false;
        }
    }

    /// Clear the selection.
    pub fn clear(&mut self) {
        self.active = false;
        self.dragging = false;
    }

    /// Get the normalized (ordered) start and end of the selection.
    pub fn range(&self) -> (GridPos, GridPos) {
        GridPos::ordered(self.anchor, self.extent)
    }

    /// Check if a cell at (col, row) is within the current selection.
    pub fn contains(&self, col: usize, row: usize) -> bool {
        if !self.active {
            return false;
        }

        let (start, end) = self.range();

        match self.mode {
            SelectionMode::Normal => {
                if row < start.row || row > end.row {
                    return false;
                }
                if start.row == end.row {
                    // Same line: col must be between start and end
                    col >= start.col && col <= end.col
                } else if row == start.row {
                    col >= start.col
                } else if row == end.row {
                    col <= end.col
                } else {
                    true // Fully selected middle row
                }
            }
            SelectionMode::Word => {
                // Word mode behaves like normal for contains check
                // (word boundaries are computed at selection start)
                if row < start.row || row > end.row {
                    return false;
                }
                if start.row == end.row {
                    col >= start.col && col <= end.col
                } else if row == start.row {
                    col >= start.col
                } else if row == end.row {
                    col <= end.col
                } else {
                    true
                }
            }
            SelectionMode::Line => {
                row >= start.row && row <= end.row
            }
        }
    }

    /// Extract selected text from a grid.
    /// `get_cell` returns the character at (col, row).
    /// `grid_width` is the number of columns.
    pub fn extract_text<F>(&self, grid_width: usize, grid_height: usize, get_cell: F) -> String
    where
        F: Fn(usize, usize) -> char,
    {
        if !self.active {
            return String::new();
        }

        let (start, end) = self.range();
        let mut result = String::new();

        for row in start.row..=end.row.min(grid_height.saturating_sub(1)) {
            let col_start = if row == start.row && self.mode != SelectionMode::Line {
                start.col
            } else {
                0
            };
            let col_end = if row == end.row && self.mode != SelectionMode::Line {
                end.col
            } else {
                grid_width.saturating_sub(1)
            };

            let mut line = String::new();
            for col in col_start..=col_end.min(grid_width.saturating_sub(1)) {
                line.push(get_cell(col, row));
            }

            // Trim trailing whitespace from each line
            let trimmed = line.trim_end();
            result.push_str(trimmed);

            // Add newline between rows (but not after the last)
            if row < end.row {
                result.push('\n');
            }
        }

        result
    }
}

/// Find word boundaries around a position.
/// Returns (start_col, end_col) of the word at `col` in the given row.
pub fn word_boundaries<F>(col: usize, grid_width: usize, get_cell: F) -> (usize, usize)
where
    F: Fn(usize) -> char,
{
    let ch = get_cell(col);
    let is_word_char = |c: char| c.is_alphanumeric() || c == '_' || c == '-' || c == '.';

    if !is_word_char(ch) {
        return (col, col);
    }

    // Scan left
    let mut start = col;
    while start > 0 && is_word_char(get_cell(start - 1)) {
        start -= 1;
    }

    // Scan right
    let mut end = col;
    while end < grid_width.saturating_sub(1) && is_word_char(get_cell(end + 1)) {
        end += 1;
    }

    (start, end)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_selection_single_line() {
        let mut sel = Selection::new();
        sel.start(GridPos::new(5, 3), SelectionMode::Normal);
        sel.update(GridPos::new(15, 3));
        sel.finish();

        assert!(sel.active);
        assert!(sel.contains(5, 3));
        assert!(sel.contains(10, 3));
        assert!(sel.contains(15, 3));
        assert!(!sel.contains(4, 3));
        assert!(!sel.contains(16, 3));
        assert!(!sel.contains(10, 2));
    }

    #[test]
    fn test_selection_multi_line() {
        let mut sel = Selection::new();
        sel.start(GridPos::new(10, 1), SelectionMode::Normal);
        sel.update(GridPos::new(5, 3));
        sel.finish();

        assert!(sel.active);
        // Row 1: col 10+
        assert!(sel.contains(10, 1));
        assert!(sel.contains(79, 1));
        assert!(!sel.contains(9, 1));
        // Row 2: all cols
        assert!(sel.contains(0, 2));
        assert!(sel.contains(79, 2));
        // Row 3: up to col 5
        assert!(sel.contains(0, 3));
        assert!(sel.contains(5, 3));
        assert!(!sel.contains(6, 3));
    }

    #[test]
    fn test_selection_line_mode() {
        let mut sel = Selection::new();
        sel.start(GridPos::new(5, 2), SelectionMode::Line);
        sel.update(GridPos::new(10, 4));
        sel.finish();

        assert!(sel.active);
        // All columns in rows 2-4
        assert!(sel.contains(0, 2));
        assert!(sel.contains(79, 3));
        assert!(sel.contains(0, 4));
        assert!(!sel.contains(0, 1));
        assert!(!sel.contains(0, 5));
    }

    #[test]
    fn test_extract_text() {
        let mut sel = Selection::new();
        sel.start(GridPos::new(0, 0), SelectionMode::Normal);
        sel.update(GridPos::new(4, 0));
        sel.finish();

        let text = sel.extract_text(80, 24, |col, _row| {
            "Hello, World!".chars().nth(col).unwrap_or(' ')
        });
        assert_eq!(text, "Hello");
    }

    #[test]
    fn test_word_boundaries() {
        let line = "hello world  foo_bar";
        let get = |col: usize| -> char {
            line.chars().nth(col).unwrap_or(' ')
        };

        assert_eq!(word_boundaries(0, 20, &get), (0, 4));  // "hello"
        assert_eq!(word_boundaries(6, 20, &get), (6, 10)); // "world"
        assert_eq!(word_boundaries(14, 20, &get), (13, 19)); // "foo_bar" (includes underscore)
    }

    #[test]
    fn test_clear_selection() {
        let mut sel = Selection::new();
        sel.start(GridPos::new(0, 0), SelectionMode::Normal);
        sel.update(GridPos::new(10, 5));
        sel.finish();
        assert!(sel.active);

        sel.clear();
        assert!(!sel.active);
        assert!(!sel.contains(5, 3));
    }
}
