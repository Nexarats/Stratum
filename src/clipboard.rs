//! Cross-platform clipboard integration.
//!
//! Uses `arboard` for system clipboard access on Windows, macOS, and Linux
//! (X11 + Wayland). Provides copy/paste operations with graceful error handling.

use arboard::Clipboard;
use std::sync::Mutex;

/// Thread-safe clipboard wrapper.
///
/// `arboard::Clipboard` is not Send/Sync on all platforms, so we wrap it
/// in a lazy-initialized Mutex and recreate on failure.
static CLIPBOARD: Mutex<Option<Clipboard>> = Mutex::new(None);

/// Initialize the clipboard (called once at startup).
fn get_clipboard() -> Result<Clipboard, String> {
    Clipboard::new().map_err(|e| format!("Clipboard init failed: {}", e))
}

/// Copy text to the system clipboard.
///
/// Returns `Ok(())` on success or an error message string.
pub fn copy_to_clipboard(text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }

    // Try using the cached clipboard first
    {
        let mut guard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut cb) = *guard {
            match cb.set_text(text) {
                Ok(()) => return Ok(()),
                Err(_) => {
                    // Clipboard may have become invalid; clear and retry below
                    *guard = None;
                }
            }
        }
    }

    // Create a fresh clipboard
    let mut cb = get_clipboard()?;
    let result = cb.set_text(text).map_err(|e| format!("Copy failed: {}", e));
    if result.is_ok() {
        let mut guard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(cb);
    }
    result
}

/// Paste text from the system clipboard.
///
/// Returns the clipboard text or an error message string.
pub fn paste_from_clipboard() -> Result<String, String> {
    // Try cached clipboard
    {
        let mut guard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(ref mut cb) = *guard {
            match cb.get_text() {
                Ok(text) => return Ok(text),
                Err(_) => {
                    *guard = None;
                }
            }
        }
    }

    // Create fresh
    let mut cb = get_clipboard()?;
    let text = cb.get_text().map_err(|e| format!("Paste failed: {}", e));
    if text.is_ok() {
        let mut guard = CLIPBOARD.lock().unwrap_or_else(|e| e.into_inner());
        *guard = Some(cb);
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_copy_empty() {
        // Copying empty string should be a no-op
        assert!(copy_to_clipboard("").is_ok());
    }

    // Note: Full clipboard tests require a display server (X11/Wayland/Windows)
    // and can't run in headless CI. They're verified manually.
}
