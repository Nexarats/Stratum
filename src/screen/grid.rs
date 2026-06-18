//! Screen grid — the 2D character buffer that represents the terminal display.
//!
//! Each cell in the grid holds a character, foreground color, background color,
//! and text attributes. The grid handles cursor movement, scrolling, line
//! editing, and scroll region support.

/// Terminal color values.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Color {
    Default,
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    White,
    BrightBlack,
    BrightRed,
    BrightGreen,
    BrightYellow,
    BrightBlue,
    BrightMagenta,
    BrightCyan,
    BrightWhite,
    /// 256-color palette index.
    Indexed(u8),
    /// 24-bit true color.
    Rgb(u8, u8, u8),
}

/// Text attributes for a cell.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Attributes {
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub inverse: bool,
}

impl Default for Attributes {
    fn default() -> Self {
        Self {
            bold: false,
            italic: false,
            underline: false,
            inverse: false,
        }
    }
}

/// A single cell in the terminal grid.
#[derive(Debug, Clone, Copy)]
pub struct Cell {
    pub ch: char,
    pub fg: Color,
    pub bg: Color,
    pub attrs: Attributes,
}

impl Default for Cell {
    fn default() -> Self {
        Self {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            attrs: Attributes::default(),
        }
    }
}

/// The terminal screen grid.
pub struct ScreenGrid {
    width: usize,
    height: usize,
    cells: Vec<Cell>,
    cursor_x: usize,
    cursor_y: usize,
    saved_cursor_x: usize,
    saved_cursor_y: usize,
    current_fg: Color,
    current_bg: Color,
    current_attrs: Attributes,
    scroll_top: usize,
    scroll_bottom: usize,
    /// Scrollback buffer — stores lines scrolled off the top.
    scrollback: Vec<Vec<Cell>>,
    /// Maximum scrollback lines.
    scrollback_limit: usize,
}

impl ScreenGrid {
    /// Create a new grid with the given dimensions.
    pub fn new(width: usize, height: usize) -> Self {
        let cells = vec![Cell::default(); width * height];
        Self {
            width,
            height,
            cells,
            cursor_x: 0,
            cursor_y: 0,
            saved_cursor_x: 0,
            saved_cursor_y: 0,
            current_fg: Color::Default,
            current_bg: Color::Default,
            current_attrs: Attributes::default(),
            scroll_top: 0,
            scroll_bottom: height.saturating_sub(1),
            scrollback: Vec::new(),
            scrollback_limit: 10_000,
        }
    }

    /// Get grid width.
    pub fn width(&self) -> usize {
        self.width
    }

    /// Get grid height.
    pub fn height(&self) -> usize {
        self.height
    }

    /// Get current scrollback history length.
    pub fn scrollback_len(&self) -> usize {
        self.scrollback.len()
    }

    /// Get a cell at the given position.
    pub fn get_cell(&self, col: usize, row: usize) -> &Cell {
        static DEFAULT_CELL: Cell = Cell {
            ch: ' ',
            fg: Color::Default,
            bg: Color::Default,
            attrs: Attributes {
                bold: false,
                italic: false,
                underline: false,
                inverse: false,
            },
        };
        let idx = row * self.width + col;
        self.cells.get(idx).unwrap_or(&DEFAULT_CELL)
    }

    /// Get a cell at the given position, taking scrollback history into account.
    pub fn get_cell_scrolled(&self, col: usize, row: usize, scroll_offset: usize) -> &Cell {
        let scrollback_len = self.scrollback.len();
        let scroll_offset = scroll_offset.min(scrollback_len);
        if scroll_offset == 0 {
            return self.get_cell(col, row);
        }

        let line_idx = row + scrollback_len - scroll_offset;
        if line_idx < scrollback_len {
            if let Some(line) = self.scrollback.get(line_idx) {
                static DEFAULT_CELL: Cell = Cell {
                    ch: ' ',
                    fg: Color::Default,
                    bg: Color::Default,
                    attrs: Attributes {
                        bold: false,
                        italic: false,
                        underline: false,
                        inverse: false,
                    },
                };
                return line.get(col).unwrap_or(&DEFAULT_CELL);
            }
        } else {
            let active_row = line_idx - scrollback_len;
            return self.get_cell(col, active_row);
        }

        self.get_cell(col, row)
    }

    /// Get cursor position (col, row).
    pub fn cursor_position(&self) -> (usize, usize) {
        (self.cursor_x, self.cursor_y)
    }

    /// Put a character at the cursor position and advance the cursor.
    pub fn put_char(&mut self, ch: char) {
        if self.cursor_x >= self.width {
            // Line wrap
            self.cursor_x = 0;
            self.line_feed();
        }

        let idx = self.cursor_y * self.width + self.cursor_x;
        if idx < self.cells.len() {
            self.cells[idx] = Cell {
                ch,
                fg: self.current_fg,
                bg: self.current_bg,
                attrs: self.current_attrs,
            };
        }
        self.cursor_x += 1;
    }

    /// Move cursor up by `n` rows.
    pub fn move_cursor_up(&mut self, n: usize) {
        self.cursor_y = self.cursor_y.saturating_sub(n);
    }

    /// Move cursor down by `n` rows.
    pub fn move_cursor_down(&mut self, n: usize) {
        self.cursor_y = (self.cursor_y + n).min(self.height - 1);
    }

    /// Move cursor left by `n` columns.
    pub fn move_cursor_left(&mut self, n: usize) {
        self.cursor_x = self.cursor_x.saturating_sub(n);
    }

    /// Move cursor right by `n` columns.
    pub fn move_cursor_right(&mut self, n: usize) {
        self.cursor_x = (self.cursor_x + n).min(self.width - 1);
    }

    /// Set cursor position (0-based).
    pub fn set_cursor_position(&mut self, col: usize, row: usize) {
        self.cursor_x = col.min(self.width.saturating_sub(1));
        self.cursor_y = row.min(self.height.saturating_sub(1));
    }

    /// Line feed — move cursor down; scroll if at bottom of scroll region.
    pub fn line_feed(&mut self) {
        if self.cursor_y == self.scroll_bottom {
            self.scroll_up(1);
        } else if self.cursor_y < self.height - 1 {
            self.cursor_y += 1;
        }
    }

    /// Carriage return — move cursor to column 0.
    pub fn carriage_return(&mut self) {
        self.cursor_x = 0;
    }

    /// Tab — advance cursor to next tab stop (every 8 columns).
    pub fn tab(&mut self) {
        let next_tab = ((self.cursor_x / 8) + 1) * 8;
        self.cursor_x = next_tab.min(self.width - 1);
    }

    /// Reverse index — move cursor up; scroll down if at top of scroll region.
    pub fn reverse_index(&mut self) {
        if self.cursor_y == self.scroll_top {
            self.scroll_down(1);
        } else {
            self.cursor_y = self.cursor_y.saturating_sub(1);
        }
    }

    /// Scroll the scroll region up by `n` lines.
    pub fn scroll_up(&mut self, n: usize) {
        for _ in 0..n {
            // Save line to scrollback
            let start = self.scroll_top * self.width;
            let end = start + self.width;
            if start < self.cells.len() && end <= self.cells.len() {
                let line: Vec<Cell> = self.cells[start..end].to_vec();
                self.scrollback.push(line);
                if self.scrollback.len() > self.scrollback_limit {
                    self.scrollback.remove(0);
                }
            }

            // Shift lines up within scroll region
            for row in self.scroll_top..self.scroll_bottom {
                let src_start = (row + 1) * self.width;
                let dst_start = row * self.width;
                for col in 0..self.width {
                    if src_start + col < self.cells.len() && dst_start + col < self.cells.len() {
                        self.cells[dst_start + col] = self.cells[src_start + col];
                    }
                }
            }

            // Clear the bottom line
            let bottom_start = self.scroll_bottom * self.width;
            for col in 0..self.width {
                if bottom_start + col < self.cells.len() {
                    self.cells[bottom_start + col] = Cell::default();
                }
            }
        }
    }

    /// Scroll the scroll region down by `n` lines.
    pub fn scroll_down(&mut self, n: usize) {
        for _ in 0..n {
            // Shift lines down within scroll region
            for row in (self.scroll_top + 1..=self.scroll_bottom).rev() {
                let src_start = (row - 1) * self.width;
                let dst_start = row * self.width;
                for col in 0..self.width {
                    if src_start + col < self.cells.len() && dst_start + col < self.cells.len() {
                        self.cells[dst_start + col] = self.cells[src_start + col];
                    }
                }
            }

            // Clear the top line
            let top_start = self.scroll_top * self.width;
            for col in 0..self.width {
                if top_start + col < self.cells.len() {
                    self.cells[top_start + col] = Cell::default();
                }
            }
        }
    }

    /// Erase from cursor to end of display.
    pub fn erase_below(&mut self) {
        // Erase from cursor to end of current line
        self.erase_line_right();
        // Erase all lines below
        for row in (self.cursor_y + 1)..self.height {
            let start = row * self.width;
            for col in 0..self.width {
                if start + col < self.cells.len() {
                    self.cells[start + col] = Cell::default();
                }
            }
        }
    }

    /// Erase from start of display to cursor.
    pub fn erase_above(&mut self) {
        for row in 0..self.cursor_y {
            let start = row * self.width;
            for col in 0..self.width {
                if start + col < self.cells.len() {
                    self.cells[start + col] = Cell::default();
                }
            }
        }
        self.erase_line_left();
    }

    /// Erase entire display.
    pub fn erase_all(&mut self) {
        for cell in &mut self.cells {
            *cell = Cell::default();
        }
    }

    /// Erase from cursor to end of line.
    pub fn erase_line_right(&mut self) {
        let start = self.cursor_y * self.width + self.cursor_x;
        let end = (self.cursor_y + 1) * self.width;
        for i in start..end.min(self.cells.len()) {
            self.cells[i] = Cell::default();
        }
    }

    /// Erase from start of line to cursor.
    pub fn erase_line_left(&mut self) {
        let start = self.cursor_y * self.width;
        let end = start + self.cursor_x + 1;
        for i in start..end.min(self.cells.len()) {
            self.cells[i] = Cell::default();
        }
    }

    /// Erase entire line.
    pub fn erase_line(&mut self) {
        let start = self.cursor_y * self.width;
        let end = start + self.width;
        for i in start..end.min(self.cells.len()) {
            self.cells[i] = Cell::default();
        }
    }

    /// Insert `n` blank lines at cursor, pushing existing lines down.
    pub fn insert_lines(&mut self, n: usize) {
        for _ in 0..n {
            // Shift lines down from cursor to scroll bottom
            for row in (self.cursor_y + 1..=self.scroll_bottom).rev() {
                let src_start = (row - 1) * self.width;
                let dst_start = row * self.width;
                for col in 0..self.width {
                    if src_start + col < self.cells.len() && dst_start + col < self.cells.len() {
                        self.cells[dst_start + col] = self.cells[src_start + col];
                    }
                }
            }
            // Clear the cursor line
            let start = self.cursor_y * self.width;
            for col in 0..self.width {
                if start + col < self.cells.len() {
                    self.cells[start + col] = Cell::default();
                }
            }
        }
    }

    /// Delete `n` lines at cursor, pulling lines up.
    pub fn delete_lines(&mut self, n: usize) {
        for _ in 0..n {
            for row in self.cursor_y..self.scroll_bottom {
                let src_start = (row + 1) * self.width;
                let dst_start = row * self.width;
                for col in 0..self.width {
                    if src_start + col < self.cells.len() && dst_start + col < self.cells.len() {
                        self.cells[dst_start + col] = self.cells[src_start + col];
                    }
                }
            }
            let bottom_start = self.scroll_bottom * self.width;
            for col in 0..self.width {
                if bottom_start + col < self.cells.len() {
                    self.cells[bottom_start + col] = Cell::default();
                }
            }
        }
    }

    /// Delete `n` characters at cursor, shifting remaining left.
    pub fn delete_chars(&mut self, n: usize) {
        let row_start = self.cursor_y * self.width;
        let row_end = row_start + self.width;
        for i in (row_start + self.cursor_x)..(row_end - n).min(row_end) {
            if i + n < self.cells.len() {
                self.cells[i] = self.cells[i + n];
            }
        }
        // Clear the vacated cells at end of line
        for i in (row_end - n).max(row_start + self.cursor_x)..row_end.min(self.cells.len()) {
            self.cells[i] = Cell::default();
        }
    }

    /// Insert `n` blank characters at cursor, shifting existing right.
    pub fn insert_chars(&mut self, n: usize) {
        let row_start = self.cursor_y * self.width;
        let row_end = row_start + self.width;
        // Shift right
        for i in (row_start + self.cursor_x + n..row_end.min(self.cells.len())).rev() {
            if i >= n {
                self.cells[i] = self.cells[i - n];
            }
        }
        // Clear inserted cells
        for i in (row_start + self.cursor_x)..(row_start + self.cursor_x + n).min(row_end) {
            if i < self.cells.len() {
                self.cells[i] = Cell::default();
            }
        }
    }

    /// Erase `n` characters starting from cursor position (ECH).
    pub fn erase_chars(&mut self, n: usize) {
        let row_start = self.cursor_y * self.width;
        for i in 0..n {
            let idx = row_start + self.cursor_x + i;
            if idx < row_start + self.width && idx < self.cells.len() {
                self.cells[idx] = Cell::default();
            }
        }
    }

    /// Set the scroll region (0-based, inclusive).
    pub fn set_scroll_region(&mut self, top: usize, bottom: usize) {
        self.scroll_top = top.min(self.height.saturating_sub(1));
        self.scroll_bottom = bottom.min(self.height.saturating_sub(1));
        if self.scroll_top > self.scroll_bottom {
            std::mem::swap(&mut self.scroll_top, &mut self.scroll_bottom);
        }
        // Reset cursor to home
        self.cursor_x = 0;
        self.cursor_y = 0;
    }

    /// Resize the grid.
    pub fn resize(&mut self, new_width: usize, new_height: usize) {
        let mut new_cells = vec![Cell::default(); new_width * new_height];

        // Copy existing content
        let copy_width = self.width.min(new_width);
        let copy_height = self.height.min(new_height);
        for row in 0..copy_height {
            for col in 0..copy_width {
                let old_idx = row * self.width + col;
                let new_idx = row * new_width + col;
                if old_idx < self.cells.len() {
                    new_cells[new_idx] = self.cells[old_idx];
                }
            }
        }

        self.cells = new_cells;
        self.width = new_width;
        self.height = new_height;
        self.scroll_top = 0;
        self.scroll_bottom = new_height.saturating_sub(1);

        // Clamp cursor
        self.cursor_x = self.cursor_x.min(new_width.saturating_sub(1));
        self.cursor_y = self.cursor_y.min(new_height.saturating_sub(1));
    }

    /// Reset the grid to empty state.
    pub fn reset(&mut self) {
        self.erase_all();
        self.cursor_x = 0;
        self.cursor_y = 0;
        self.current_fg = Color::Default;
        self.current_bg = Color::Default;
        self.current_attrs = Attributes::default();
        self.scroll_top = 0;
        self.scroll_bottom = self.height.saturating_sub(1);
    }

    /// Save cursor position.
    pub fn save_cursor(&mut self) {
        self.saved_cursor_x = self.cursor_x;
        self.saved_cursor_y = self.cursor_y;
    }

    /// Restore cursor position.
    pub fn restore_cursor(&mut self) {
        self.cursor_x = self.saved_cursor_x;
        self.cursor_y = self.saved_cursor_y;
    }

    // --- Attribute setters ---

    pub fn set_fg(&mut self, color: Color) {
        self.current_fg = color;
    }

    pub fn set_bg(&mut self, color: Color) {
        self.current_bg = color;
    }

    pub fn set_bold(&mut self, bold: bool) {
        self.current_attrs.bold = bold;
    }

    pub fn set_italic(&mut self, italic: bool) {
        self.current_attrs.italic = italic;
    }

    pub fn set_underline(&mut self, underline: bool) {
        self.current_attrs.underline = underline;
    }

    pub fn set_inverse(&mut self, inverse: bool) {
        self.current_attrs.inverse = inverse;
    }

    pub fn reset_attributes(&mut self) {
        self.current_fg = Color::Default;
        self.current_bg = Color::Default;
        self.current_attrs = Attributes::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_grid() {
        let grid = ScreenGrid::new(80, 24);
        assert_eq!(grid.width(), 80);
        assert_eq!(grid.height(), 24);
        assert_eq!(grid.cursor_position(), (0, 0));
    }

    #[test]
    fn test_put_char() {
        let mut grid = ScreenGrid::new(80, 24);
        grid.put_char('A');
        assert_eq!(grid.get_cell(0, 0).ch, 'A');
        assert_eq!(grid.cursor_position(), (1, 0));
    }

    #[test]
    fn test_line_wrap() {
        let mut grid = ScreenGrid::new(5, 3);
        for ch in "Hello World!".chars() {
            grid.put_char(ch);
        }
        // "Hello" on row 0, " Worl" on row 1, "d!" on row 2
        assert_eq!(grid.get_cell(0, 0).ch, 'H');
        assert_eq!(grid.get_cell(4, 0).ch, 'o');
        assert_eq!(grid.get_cell(0, 1).ch, ' ');
    }

    #[test]
    fn test_erase_all() {
        let mut grid = ScreenGrid::new(80, 24);
        grid.put_char('X');
        grid.erase_all();
        assert_eq!(grid.get_cell(0, 0).ch, ' ');
    }

    #[test]
    fn test_scroll_up() {
        let mut grid = ScreenGrid::new(5, 3);
        grid.set_cursor_position(0, 0);
        grid.put_char('A');
        grid.set_cursor_position(0, 1);
        grid.put_char('B');
        grid.set_cursor_position(0, 2);
        grid.put_char('C');

        grid.scroll_up(1);

        // Row 0 should now contain 'B', row 1 'C', row 2 blank
        assert_eq!(grid.get_cell(0, 0).ch, 'B');
        assert_eq!(grid.get_cell(0, 1).ch, 'C');
        assert_eq!(grid.get_cell(0, 2).ch, ' ');
    }

    #[test]
    fn test_resize() {
        let mut grid = ScreenGrid::new(80, 24);
        grid.put_char('Z');
        grid.resize(120, 40);
        assert_eq!(grid.width(), 120);
        assert_eq!(grid.height(), 40);
        assert_eq!(grid.get_cell(0, 0).ch, 'Z');
    }
}
