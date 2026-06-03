//! Keyboard input handling and key binding configuration.

use serde::Deserialize;
use std::collections::HashMap;

/// Represents a key binding action.
#[derive(Debug, Clone, Deserialize)]
pub enum Action {
    /// Send raw bytes to the PTY.
    SendBytes(Vec<u8>),
    /// Copy selection to clipboard.
    Copy,
    /// Paste from clipboard.
    Paste,
    /// Split pane horizontally.
    SplitHorizontal,
    /// Split pane vertically.
    SplitVertical,
    /// Create a new tab.
    NewTab,
    /// Close current pane/tab.
    Close,
    /// Switch to next tab.
    NextTab,
    /// Switch to previous tab.
    PrevTab,
    /// Focus next pane.
    FocusNextPane,
    /// Focus previous pane.
    FocusPrevPane,
    /// Scroll up by page.
    ScrollPageUp,
    /// Scroll down by page.
    ScrollPageDown,
    /// Toggle fullscreen.
    ToggleFullscreen,
    /// Open command palette.
    CommandPalette,
    /// Increase font size.
    FontIncrease,
    /// Decrease font size.
    FontDecrease,
    /// Reset font size.
    FontReset,
}

/// User-configurable keybinding overrides from config file.
#[derive(Debug, Default, Deserialize)]
#[serde(default)]
pub struct KeybindingConfig {
    pub copy: Option<String>,
    pub paste: Option<String>,
    pub new_tab: Option<String>,
    pub close: Option<String>,
    pub split_horizontal: Option<String>,
    pub split_vertical: Option<String>,
    pub next_tab: Option<String>,
    pub prev_tab: Option<String>,
    pub focus_next: Option<String>,
    pub focus_prev: Option<String>,
    pub scroll_page_up: Option<String>,
    pub scroll_page_down: Option<String>,
    pub fullscreen: Option<String>,
    pub command_palette: Option<String>,
    pub font_increase: Option<String>,
    pub font_decrease: Option<String>,
    pub font_reset: Option<String>,
}

/// Key binding manager.
pub struct KeyBindings {
    bindings: HashMap<String, Action>,
}

impl KeyBindings {
    /// Create key bindings with defaults.
    pub fn new() -> Self {
        let mut bindings = HashMap::new();

        // Default bindings
        bindings.insert("Ctrl+Shift+C".into(), Action::Copy);
        bindings.insert("Ctrl+Shift+V".into(), Action::Paste);
        bindings.insert("Ctrl+Shift+T".into(), Action::NewTab);
        bindings.insert("Ctrl+Shift+W".into(), Action::Close);
        bindings.insert("Ctrl+Tab".into(), Action::NextTab);
        bindings.insert("Ctrl+Shift+Tab".into(), Action::PrevTab);
        bindings.insert("Ctrl+Shift+H".into(), Action::SplitHorizontal);
        bindings.insert("Ctrl+Shift+E".into(), Action::SplitVertical);
        bindings.insert("Ctrl+Shift+J".into(), Action::FocusNextPane);
        bindings.insert("Ctrl+Shift+K".into(), Action::FocusPrevPane);
        bindings.insert("Shift+PageUp".into(), Action::ScrollPageUp);
        bindings.insert("Shift+PageDown".into(), Action::ScrollPageDown);
        bindings.insert("F11".into(), Action::ToggleFullscreen);
        bindings.insert("Ctrl+Shift+P".into(), Action::CommandPalette);
        bindings.insert("Ctrl+Plus".into(), Action::FontIncrease);
        bindings.insert("Ctrl+Minus".into(), Action::FontDecrease);
        bindings.insert("Ctrl+0".into(), Action::FontReset);

        Self { bindings }
    }

    /// Merge user keybinding overrides from config.
    ///
    /// Any non-None field in `config` replaces the default binding.
    /// The old key (if any) for that action is removed.
    pub fn apply_config(&mut self, config: &KeybindingConfig) {
        let overrides: Vec<(Option<&String>, Action)> = vec![
            (config.copy.as_ref(), Action::Copy),
            (config.paste.as_ref(), Action::Paste),
            (config.new_tab.as_ref(), Action::NewTab),
            (config.close.as_ref(), Action::Close),
            (config.split_horizontal.as_ref(), Action::SplitHorizontal),
            (config.split_vertical.as_ref(), Action::SplitVertical),
            (config.next_tab.as_ref(), Action::NextTab),
            (config.prev_tab.as_ref(), Action::PrevTab),
            (config.focus_next.as_ref(), Action::FocusNextPane),
            (config.focus_prev.as_ref(), Action::FocusPrevPane),
            (config.scroll_page_up.as_ref(), Action::ScrollPageUp),
            (config.scroll_page_down.as_ref(), Action::ScrollPageDown),
            (config.fullscreen.as_ref(), Action::ToggleFullscreen),
            (config.command_palette.as_ref(), Action::CommandPalette),
            (config.font_increase.as_ref(), Action::FontIncrease),
            (config.font_decrease.as_ref(), Action::FontDecrease),
            (config.font_reset.as_ref(), Action::FontReset),
        ];

        for (key_opt, action) in overrides {
            if let Some(key) = key_opt {
                // Remove any existing binding for this action type
                let action_name = format!("{:?}", action);
                self.bindings.retain(|_, v| format!("{:?}", v) != action_name);
                // Insert new binding
                self.bindings.insert(key.clone(), action);
            }
        }
    }

    /// Look up an action for a key combination.
    pub fn get_action(&self, key_str: &str) -> Option<&Action> {
        self.bindings.get(key_str)
    }

    /// Get all current bindings (for display/debugging).
    pub fn all_bindings(&self) -> &HashMap<String, Action> {
        &self.bindings
    }
}

impl Default for KeyBindings {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_bindings() {
        let kb = KeyBindings::new();
        assert!(matches!(kb.get_action("Ctrl+Shift+C"), Some(Action::Copy)));
        assert!(matches!(kb.get_action("Ctrl+Shift+V"), Some(Action::Paste)));
        assert!(matches!(kb.get_action("Ctrl+Shift+T"), Some(Action::NewTab)));
    }

    #[test]
    fn test_apply_config_override() {
        let mut kb = KeyBindings::new();
        let config = KeybindingConfig {
            copy: Some("Ctrl+C".into()),
            ..Default::default()
        };
        kb.apply_config(&config);

        // Old binding should be gone
        assert!(kb.get_action("Ctrl+Shift+C").is_none());
        // New binding should work
        assert!(matches!(kb.get_action("Ctrl+C"), Some(Action::Copy)));
    }

    #[test]
    fn test_no_override_keeps_defaults() {
        let mut kb = KeyBindings::new();
        let config = KeybindingConfig::default();
        kb.apply_config(&config);
        assert!(matches!(kb.get_action("Ctrl+Shift+C"), Some(Action::Copy)));
    }
}
