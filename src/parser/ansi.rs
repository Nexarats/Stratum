//! ANSI escape sequence parser — finite state machine.
//!
//! This parser processes a byte stream from the PTY and
//! translates escape sequences into screen operations
//! (cursor movement, color changes, text insertion, etc.).

use crate::screen::{Color, ScreenGrid};

/// Parser states for the ANSI FSM.
#[derive(Debug, Clone, Copy, PartialEq)]
enum State {
    /// Normal text — characters go directly to the grid.
    Ground,
    /// Just received ESC (0x1B) — waiting for next byte.
    Escape,
    /// CSI sequence — ESC [ ... — collecting parameters.
    CsiEntry,
    /// OSC sequence — ESC ] ... — operating system command.
    OscString,
}

/// ANSI escape sequence parser.
pub struct AnsiParser {
    state: State,
    /// Collected CSI parameters (semicolon-separated numbers).
    params: Vec<u32>,
    /// Current parameter being built.
    current_param: u32,
    /// Whether we've started building a parameter.
    has_param: bool,
    /// Intermediate bytes collected during CSI.
    intermediates: Vec<u8>,
    /// OSC string buffer.
    osc_buffer: Vec<u8>,
    /// NOS Shell IPC messages (structured JSON data received via OSC).
    nos_ipc_messages: Vec<String>,
    /// Stratum slash commands intercepted by the shell hook.
    pub stratum_commands: Vec<String>,
}

impl AnsiParser {
    /// Create a new parser starting in ground state.
    pub fn new() -> Self {
        Self {
            state: State::Ground,
            params: Vec::with_capacity(16),
            current_param: 0,
            has_param: false,
            intermediates: Vec::with_capacity(4),
            osc_buffer: Vec::with_capacity(256),
            nos_ipc_messages: Vec::new(),
            stratum_commands: Vec::new(),
        }
    }

    /// Process a single byte from the PTY output stream.
    pub fn advance(&mut self, byte: u8, screen: &mut ScreenGrid) {
        match self.state {
            State::Ground => self.ground(byte, screen),
            State::Escape => self.escape(byte, screen),
            State::CsiEntry => self.csi(byte, screen),
            State::OscString => self.osc(byte),
        }
    }

    /// Drain any NOS Shell IPC messages received since last call.
    pub fn take_nos_ipc(&mut self) -> Vec<String> {
        std::mem::take(&mut self.nos_ipc_messages)
    }

    /// Drain any Stratum slash commands received since last call.
    pub fn take_stratum_commands(&mut self) -> Vec<String> {
        std::mem::take(&mut self.stratum_commands)
    }

    /// Ground state — normal characters and C0 controls.
    fn ground(&mut self, byte: u8, screen: &mut ScreenGrid) {
        match byte {
            // ESC — enter escape state
            0x1B => {
                self.state = State::Escape;
            }

            // Printable ASCII + UTF-8 start bytes
            0x20..=0x7E | 0xC0..=0xFF => {
                screen.put_char(byte as char);
            }

            // Backspace
            0x08 => screen.move_cursor_left(1),

            // Tab
            0x09 => screen.tab(),

            // Line Feed / Vertical Tab / Form Feed
            0x0A | 0x0B | 0x0C => screen.line_feed(),

            // Carriage Return
            0x0D => screen.carriage_return(),

            // Bell
            0x07 => { /* TODO: acoustic feedback */ }

            _ => {}
        }
    }

    /// Escape state — received ESC, waiting for sequence type.
    fn escape(&mut self, byte: u8, screen: &mut ScreenGrid) {
        match byte {
            // CSI — Control Sequence Introducer
            b'[' => {
                self.state = State::CsiEntry;
                self.params.clear();
                self.current_param = 0;
                self.has_param = false;
                self.intermediates.clear();
            }

            // OSC — Operating System Command
            b']' => {
                self.state = State::OscString;
                self.osc_buffer.clear();
            }

            // RIS — Full Reset
            b'c' => {
                screen.reset();
                self.state = State::Ground;
            }

            // IND — Index (move cursor down, scroll if at bottom)
            b'D' => {
                screen.line_feed();
                self.state = State::Ground;
            }

            // NEL — Next Line
            b'E' => {
                screen.carriage_return();
                screen.line_feed();
                self.state = State::Ground;
            }

            // RI — Reverse Index (move cursor up, scroll down if at top)
            b'M' => {
                screen.reverse_index();
                self.state = State::Ground;
            }

            // DECSC — Save Cursor Position
            b'7' => {
                screen.save_cursor();
                self.state = State::Ground;
            }

            // DECRC — Restore Cursor Position
            b'8' => {
                screen.restore_cursor();
                self.state = State::Ground;
            }

            // Unknown — return to ground
            _ => {
                self.state = State::Ground;
            }
        }
    }

    /// CSI state — collecting parameters for a control sequence.
    fn csi(&mut self, byte: u8, screen: &mut ScreenGrid) {
        match byte {
            // Digit — build parameter
            b'0'..=b'9' => {
                self.current_param = self.current_param * 10 + (byte - b'0') as u32;
                self.has_param = true;
            }

            // Semicolon — parameter separator
            b';' => {
                self.params.push(if self.has_param {
                    self.current_param
                } else {
                    0
                });
                self.current_param = 0;
                self.has_param = false;
            }

            // Private mode prefix bytes: ?, >, <, =
            // e.g., ESC[?25h (DEC show cursor), ESC[?1049h (alt screen)
            0x3C..=0x3F => {
                self.intermediates.push(byte);
            }

            // Intermediate bytes (space through /)
            0x20..=0x2F => {
                self.intermediates.push(byte);
            }

            // Final byte — dispatch the CSI sequence
            0x40..=0x7E => {
                // Push last parameter
                if self.has_param {
                    self.params.push(self.current_param);
                }

                self.dispatch_csi(byte, screen);
                self.state = State::Ground;
            }

            // ESC during CSI — restart escape
            0x1B => {
                self.state = State::Escape;
            }

            // Anything else — abort
            _ => {
                self.state = State::Ground;
            }
        }
    }

    /// Dispatch a CSI sequence based on the final byte.
    fn dispatch_csi(&self, final_byte: u8, screen: &mut ScreenGrid) {
        let param = |i: usize, default: u32| -> u32 {
            self.params.get(i).copied().filter(|&v| v > 0).unwrap_or(default)
        };

        // Check for DEC private mode prefix (?)
        let is_private = self.intermediates.contains(&b'?');

        // DEC private mode sequences (ESC[?Nh / ESC[?Nl)
        if is_private {
            match final_byte {
                // DECSET — Set Mode
                b'h' => {
                    for &p in &self.params {
                        match p {
                            1 => {} // DECCKM — Application cursor keys (handled)
                            7 => {} // DECAWM — Auto-wrap mode
                            12 => {} // Cursor blink
                            25 => {} // DECTCEM — Show cursor
                            47 | 1047 => {} // Alternate screen buffer
                            1000 => {} // Mouse tracking
                            1002 => {} // Mouse button tracking
                            1003 => {} // All mouse tracking
                            1004 => {} // Focus events
                            1005 => {} // UTF-8 mouse
                            1006 => {} // SGR mouse
                            1049 => {} // Alternate screen + save cursor
                            2004 => {} // Bracketed paste mode
                            _ => {
                                tracing::trace!("Unhandled DECSET: {}", p);
                            }
                        }
                    }
                }
                // DECRST — Reset Mode
                b'l' => {
                    for &p in &self.params {
                        match p {
                            1 | 7 | 12 | 25 | 47 | 1000 | 1002 | 1003 | 1004
                            | 1005 | 1006 | 1047 | 1049 | 2004 => {} // Silently handle
                            _ => {
                                tracing::trace!("Unhandled DECRST: {}", p);
                            }
                        }
                    }
                }
                _ => {
                    tracing::trace!("Unhandled private CSI: ?{:?} {}", self.params, final_byte as char);
                }
            }
            return;
        }

        match final_byte {
            // CUU — Cursor Up
            b'A' => screen.move_cursor_up(param(0, 1) as usize),

            // CUD — Cursor Down
            b'B' => screen.move_cursor_down(param(0, 1) as usize),

            // CUF — Cursor Forward (Right)
            b'C' => screen.move_cursor_right(param(0, 1) as usize),

            // CUB — Cursor Back (Left)
            b'D' => screen.move_cursor_left(param(0, 1) as usize),

            // CNL — Cursor Next Line
            b'E' => {
                screen.move_cursor_down(param(0, 1) as usize);
                screen.carriage_return();
            }

            // CPL — Cursor Previous Line
            b'F' => {
                screen.move_cursor_up(param(0, 1) as usize);
                screen.carriage_return();
            }

            // CHA — Cursor Horizontal Absolute (column, 1-based)
            b'G' => {
                let col = param(0, 1) as usize;
                let (_, row) = screen.cursor_position();
                screen.set_cursor_position(col.saturating_sub(1), row);
            }

            // CUP / HVP — Cursor Position (row;col, 1-based)
            b'H' | b'f' => {
                let row = param(0, 1) as usize;
                let col = param(1, 1) as usize;
                screen.set_cursor_position(col.saturating_sub(1), row.saturating_sub(1));
            }

            // ED — Erase in Display
            b'J' => {
                let mode = param(0, 0);
                match mode {
                    0 => screen.erase_below(),
                    1 => screen.erase_above(),
                    2 | 3 => screen.erase_all(),
                    _ => {}
                }
            }

            // EL — Erase in Line
            b'K' => {
                let mode = param(0, 0);
                match mode {
                    0 => screen.erase_line_right(),
                    1 => screen.erase_line_left(),
                    2 => screen.erase_line(),
                    _ => {}
                }
            }

            // SGR — Select Graphic Rendition
            b'm' => {
                self.dispatch_sgr(screen);
            }

            // IL — Insert Lines
            b'L' => screen.insert_lines(param(0, 1) as usize),

            // DL — Delete Lines
            b'M' => screen.delete_lines(param(0, 1) as usize),

            // DCH — Delete Characters
            b'P' => screen.delete_chars(param(0, 1) as usize),

            // ICH — Insert Characters
            b'@' => screen.insert_chars(param(0, 1) as usize),

            // ECH — Erase Characters
            b'X' => screen.erase_chars(param(0, 1) as usize),

            // VPA — Vertical Position Absolute (row, 1-based)
            b'd' => {
                let row = param(0, 1) as usize;
                let (col, _) = screen.cursor_position();
                screen.set_cursor_position(col, row.saturating_sub(1));
            }

            // SU — Scroll Up
            b'S' => {
                screen.scroll_up(param(0, 1) as usize);
            }

            // SD — Scroll Down
            b'T' => {
                screen.scroll_down(param(0, 1) as usize);
            }

            // SM — Set Mode (non-private)
            b'h' => {} // IRM, SRM, etc. — silently consume

            // RM — Reset Mode (non-private)
            b'l' => {} // silently consume

            // DSR — Device Status Report
            b'n' => {
                // Ignored — would need write-back to PTY
                tracing::trace!("DSR request: {:?}", self.params);
            }

            // DECSTBM — Set Scrolling Region
            b'r' => {
                let top = param(0, 1) as usize;
                let bottom = param(1, screen.height() as u32) as usize;
                screen.set_scroll_region(top.saturating_sub(1), bottom.saturating_sub(1));
            }

            // Other CSI sequences — ignore silently
            _ => {
                tracing::trace!("Unhandled CSI: {:?} {}", self.params, final_byte as char);
            }
        }
    }

    /// Dispatch SGR (Select Graphic Rendition) — colors and text attributes.
    fn dispatch_sgr(&self, screen: &mut ScreenGrid) {
        if self.params.is_empty() {
            screen.reset_attributes();
            return;
        }

        let mut i = 0;
        while i < self.params.len() {
            match self.params[i] {
                0 => screen.reset_attributes(),
                1 => screen.set_bold(true),
                3 => screen.set_italic(true),
                4 => screen.set_underline(true),
                7 => screen.set_inverse(true),
                22 => screen.set_bold(false),
                23 => screen.set_italic(false),
                24 => screen.set_underline(false),
                27 => screen.set_inverse(false),

                // Foreground colors (standard)
                30 => screen.set_fg(Color::Black),
                31 => screen.set_fg(Color::Red),
                32 => screen.set_fg(Color::Green),
                33 => screen.set_fg(Color::Yellow),
                34 => screen.set_fg(Color::Blue),
                35 => screen.set_fg(Color::Magenta),
                36 => screen.set_fg(Color::Cyan),
                37 => screen.set_fg(Color::White),
                39 => screen.set_fg(Color::Default),

                // Background colors (standard)
                40 => screen.set_bg(Color::Black),
                41 => screen.set_bg(Color::Red),
                42 => screen.set_bg(Color::Green),
                43 => screen.set_bg(Color::Yellow),
                44 => screen.set_bg(Color::Blue),
                45 => screen.set_bg(Color::Magenta),
                46 => screen.set_bg(Color::Cyan),
                47 => screen.set_bg(Color::White),
                49 => screen.set_bg(Color::Default),

                // Bright foreground
                90..=97 => {
                    let color = match self.params[i] - 90 {
                        0 => Color::BrightBlack,
                        1 => Color::BrightRed,
                        2 => Color::BrightGreen,
                        3 => Color::BrightYellow,
                        4 => Color::BrightBlue,
                        5 => Color::BrightMagenta,
                        6 => Color::BrightCyan,
                        7 => Color::BrightWhite,
                        _ => Color::Default,
                    };
                    screen.set_fg(color);
                }

                // Bright background
                100..=107 => {
                    let color = match self.params[i] - 100 {
                        0 => Color::BrightBlack,
                        1 => Color::BrightRed,
                        2 => Color::BrightGreen,
                        3 => Color::BrightYellow,
                        4 => Color::BrightBlue,
                        5 => Color::BrightMagenta,
                        6 => Color::BrightCyan,
                        7 => Color::BrightWhite,
                        _ => Color::Default,
                    };
                    screen.set_bg(color);
                }

                // 256-color foreground: 38;5;N
                38 => {
                    if i + 2 < self.params.len() && self.params[i + 1] == 5 {
                        screen.set_fg(Color::Indexed(self.params[i + 2] as u8));
                        i += 2;
                    } else if i + 4 < self.params.len() && self.params[i + 1] == 2 {
                        // 24-bit color: 38;2;R;G;B
                        screen.set_fg(Color::Rgb(
                            self.params[i + 2] as u8,
                            self.params[i + 3] as u8,
                            self.params[i + 4] as u8,
                        ));
                        i += 4;
                    }
                }

                // 256-color / 24-bit background: 48;5;N or 48;2;R;G;B
                48 => {
                    if i + 2 < self.params.len() && self.params[i + 1] == 5 {
                        screen.set_bg(Color::Indexed(self.params[i + 2] as u8));
                        i += 2;
                    } else if i + 4 < self.params.len() && self.params[i + 1] == 2 {
                        screen.set_bg(Color::Rgb(
                            self.params[i + 2] as u8,
                            self.params[i + 3] as u8,
                            self.params[i + 4] as u8,
                        ));
                        i += 4;
                    }
                }

                _ => {}
            }
            i += 1;
        }
    }

    /// OSC state — collecting operating system command string.
    fn osc(&mut self, byte: u8) {
        match byte {
            // BEL terminates OSC
            0x07 => {
                self.dispatch_osc();
                self.state = State::Ground;
            }
            // ESC might start ST (String Terminator = ESC \)
            0x1B => {
                // Simplified: treat ESC in OSC as terminator
                self.dispatch_osc();
                self.state = State::Ground;
            }
            _ => {
                self.osc_buffer.push(byte);
            }
        }
    }

    /// Dispatch OSC command.
    fn dispatch_osc(&mut self) {
        // OSC commands are used for setting window title, etc.
        // Parse: first number before ';' is the command type
        let s = String::from_utf8_lossy(&self.osc_buffer);
        tracing::info!("OSC dispatch: buffer={:?} len={}", s, self.osc_buffer.len());
        if let Some((cmd, _value)) = s.split_once(';') {
            match cmd {
                "0" | "2" => {
                    // Set window title (ignored in basic mode)
                    tracing::trace!("OSC set title: {}", _value);
                }
                "NOS" => {
                    // NOS Shell IPC: structured data as JSON
                    tracing::debug!("NOS IPC received: {} bytes", _value.len());
                    self.nos_ipc_messages.push(_value.to_string());
                }
                "STRATUM" => {
                    // Stratum shell hook: intercepted slash command
                    tracing::info!("STRATUM command received via OSC: {}", _value);
                    self.stratum_commands.push(_value.to_string());
                }
                _ => {
                    tracing::info!("Unhandled OSC cmd={}: value={}", cmd, _value);
                }
            }
        } else {
            tracing::info!("OSC with no semicolon: {:?}", s);
        }
    }
}

impl Default for AnsiParser {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text() {
        let mut parser = AnsiParser::new();
        let mut screen = ScreenGrid::new(80, 24);

        for byte in b"Hello" {
            parser.advance(*byte, &mut screen);
        }

        assert_eq!(screen.get_cell(0, 0).ch, 'H');
        assert_eq!(screen.get_cell(1, 0).ch, 'e');
        assert_eq!(screen.get_cell(2, 0).ch, 'l');
        assert_eq!(screen.get_cell(3, 0).ch, 'l');
        assert_eq!(screen.get_cell(4, 0).ch, 'o');
    }

    #[test]
    fn test_cursor_movement() {
        let mut parser = AnsiParser::new();
        let mut screen = ScreenGrid::new(80, 24);

        // Move cursor to row 5, col 10 (1-based)
        for byte in b"\x1b[5;10H" {
            parser.advance(*byte, &mut screen);
        }

        let (col, row) = screen.cursor_position();
        assert_eq!(col, 9); // 0-based
        assert_eq!(row, 4); // 0-based
    }

    #[test]
    fn test_erase_display() {
        let mut parser = AnsiParser::new();
        let mut screen = ScreenGrid::new(80, 24);

        // Write text
        for byte in b"Hello World" {
            parser.advance(*byte, &mut screen);
        }

        // Erase entire display
        for byte in b"\x1b[2J" {
            parser.advance(*byte, &mut screen);
        }

        assert_eq!(screen.get_cell(0, 0).ch, ' ');
    }

    #[test]
    fn test_color_setting() {
        let mut parser = AnsiParser::new();
        let mut screen = ScreenGrid::new(80, 24);

        // Set red foreground
        for byte in b"\x1b[31m" {
            parser.advance(*byte, &mut screen);
        }

        // Write a character — it should have red foreground
        parser.advance(b'X', &mut screen);
        assert_eq!(screen.get_cell(0, 0).fg, Color::Red);
    }
}
