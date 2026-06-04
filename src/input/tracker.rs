//! Command input tracker — monitors what the user is typing.
//!
//! Captures keystrokes to build a picture of the current command line
//! being typed. This enables proactive features like inline docs,
//! consequence scoring, and mutation preview BEFORE the user hits Enter.

/// Tracks the current command input state.
pub struct InputTracker {
    /// The current line buffer (what the user is typing).
    buffer: String,
    /// Complete command history for this session.
    history: Vec<String>,
    /// Whether the current buffer has changed since last check.
    dirty: bool,
    /// Cursor position within the buffer.
    cursor_pos: usize,
    /// State machine for skipping multi-byte escape sequences.
    escape_state: EscapeState,
}

/// State machine for tracking multi-byte escape sequences.
#[derive(Debug, Clone, Copy, PartialEq)]
enum EscapeState {
    /// Normal input — not inside an escape sequence.
    Normal,
    /// Just saw ESC (0x1b) — waiting for next byte to determine sequence type.
    Escaped,
    /// Inside a CSI sequence (ESC [) — skip until we see a final byte (0x40..=0x7e).
    Csi,
    /// Inside an SS3 sequence (ESC O) — skip exactly one more byte.
    Ss3,
}

impl InputTracker {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
            history: Vec::new(),
            dirty: false,
            cursor_pos: 0,
            escape_state: EscapeState::Normal,
        }
    }

    /// Feed a byte that was sent to the PTY.
    /// Returns Some(command) if Enter was pressed (command submitted).
    pub fn feed(&mut self, data: &[u8]) -> Option<String> {
        for &byte in data {
            // --- Escape sequence state machine ---
            match self.escape_state {
                EscapeState::Escaped => {
                    match byte {
                        b'[' => {
                            self.escape_state = EscapeState::Csi;
                            continue;
                        }
                        b'O' => {
                            self.escape_state = EscapeState::Ss3;
                            continue;
                        }
                        _ => {
                            // Unknown escape — just skip this byte too and reset
                            self.escape_state = EscapeState::Normal;
                            continue;
                        }
                    }
                }
                EscapeState::Csi => {
                    // CSI sequences: ESC [ <params> <final byte>
                    // Parameters are 0x30..=0x3f, intermediates are 0x20..=0x2f
                    // Final byte is 0x40..=0x7e
                    if (0x40..=0x7e).contains(&byte) {
                        // Final byte — sequence complete
                        self.escape_state = EscapeState::Normal;
                    }
                    // Either way, skip this byte (it's part of the escape sequence)
                    continue;
                }
                EscapeState::Ss3 => {
                    // SS3 sequences: ESC O <single byte>
                    self.escape_state = EscapeState::Normal;
                    continue;
                }
                EscapeState::Normal => {
                    // Fall through to normal processing below
                }
            }

            // --- Normal byte processing ---
            match byte {
                // Enter (carriage return) — command submitted
                b'\r' | b'\n' => {
                    if !self.buffer.trim().is_empty() {
                        let command = self.buffer.clone();
                        self.history.push(command.clone());
                        self.buffer.clear();
                        self.cursor_pos = 0;
                        self.dirty = true;
                        return Some(command);
                    }
                    self.buffer.clear();
                    self.cursor_pos = 0;
                    self.dirty = true;
                }
                // Backspace (DEL)
                0x7f => {
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                        self.buffer.remove(self.cursor_pos);
                        self.dirty = true;
                    }
                }
                // Backspace (BS) — some terminals send 0x08 instead of 0x7f
                0x08 => {
                    if self.cursor_pos > 0 {
                        self.cursor_pos -= 1;
                        self.buffer.remove(self.cursor_pos);
                        self.dirty = true;
                    }
                }
                // Ctrl+C — cancel
                0x03 => {
                    self.buffer.clear();
                    self.cursor_pos = 0;
                    self.dirty = true;
                }
                // Ctrl+U — clear line
                0x15 => {
                    self.buffer.clear();
                    self.cursor_pos = 0;
                    self.dirty = true;
                }
                // Ctrl+W — delete word
                0x17 => {
                    while self.cursor_pos > 0
                        && self.buffer.as_bytes().get(self.cursor_pos - 1) == Some(&b' ')
                    {
                        self.cursor_pos -= 1;
                        self.buffer.remove(self.cursor_pos);
                    }
                    while self.cursor_pos > 0
                        && self.buffer.as_bytes().get(self.cursor_pos - 1) != Some(&b' ')
                    {
                        self.cursor_pos -= 1;
                        self.buffer.remove(self.cursor_pos);
                    }
                    self.dirty = true;
                }
                // Tab — don't add to buffer (shell handles completion)
                b'\t' => {}
                // Escape — start of an escape sequence
                0x1b => {
                    self.escape_state = EscapeState::Escaped;
                }
                // Printable ASCII
                0x20..=0x7e => {
                    self.buffer.insert(self.cursor_pos, byte as char);
                    self.cursor_pos += 1;
                    self.dirty = true;
                }
                // UTF-8 continuation bytes or other — try to append
                0x80..=0xff => {
                    // Simplified: just append the byte as a char replacement
                    // Full UTF-8 decoding would require buffering multi-byte sequences
                }
                _ => {}
            }
        }
        None
    }

    /// Get the current buffer contents.
    pub fn current_input(&self) -> &str {
        &self.buffer
    }

    /// Check if input changed since last query, and reset the flag.
    pub fn take_dirty(&mut self) -> bool {
        let was_dirty = self.dirty;
        self.dirty = false;
        was_dirty
    }

    /// Get the base command name being typed (first word).
    pub fn current_command_name(&self) -> Option<&str> {
        self.buffer.split_whitespace().next()
    }

    /// Get all history.
    pub fn history(&self) -> &[String] {
        &self.history
    }

    /// Get the last N commands.
    pub fn recent_history(&self, n: usize) -> &[String] {
        let len = self.history.len();
        if n >= len {
            &self.history
        } else {
            &self.history[len - n..]
        }
    }
}

impl Default for InputTracker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_input() {
        let mut tracker = InputTracker::new();
        tracker.feed(b"ls -la");
        assert_eq!(tracker.current_input(), "ls -la");
        assert_eq!(tracker.current_command_name(), Some("ls"));
    }

    #[test]
    fn test_enter_submits() {
        let mut tracker = InputTracker::new();
        tracker.feed(b"echo hello");
        let cmd = tracker.feed(b"\r");
        assert_eq!(cmd, Some("echo hello".to_string()));
        assert_eq!(tracker.current_input(), "");
    }

    #[test]
    fn test_backspace() {
        let mut tracker = InputTracker::new();
        tracker.feed(b"helloo");
        tracker.feed(&[0x7f]); // backspace
        assert_eq!(tracker.current_input(), "hello");
    }

    #[test]
    fn test_ctrl_c_clears() {
        let mut tracker = InputTracker::new();
        tracker.feed(b"rm -rf /");
        tracker.feed(&[0x03]); // Ctrl+C
        assert_eq!(tracker.current_input(), "");
    }

    #[test]
    fn test_history() {
        let mut tracker = InputTracker::new();
        tracker.feed(b"cmd1");
        tracker.feed(b"\r");
        tracker.feed(b"cmd2");
        tracker.feed(b"\r");
        assert_eq!(tracker.history().len(), 2);
        assert_eq!(tracker.recent_history(1), &["cmd2".to_string()]);
    }
}
