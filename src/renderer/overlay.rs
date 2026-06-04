//! Overlay system — renders feature UIs on top of the terminal.
//!
//! Manages status bar, inline docs popup, mutation preview panel,
//! consequence warning dialogs, and suggestion card popups.
//! Each overlay is positioned relative to the terminal grid and
//! rendered as an additional GPU draw pass after the main grid.

/// An overlay element to render on top of the terminal.
#[derive(Debug, Clone)]
pub enum OverlayElement {
    /// Bottom status bar with pane/tab info and consequence score.
    StatusBar(StatusBarContent),
    /// Inline documentation popup near the cursor.
    InlineDoc(InlineDocContent),
    /// Mutation preview panel (side panel or bottom).
    MutationPreview(MutationPreviewContent),
    /// Consequence warning dialog (modal-like).
    ConsequenceWarning(ConsequenceWarningContent),
    /// Notification toast (top-right, auto-dismiss).
    Toast(ToastContent),
    /// GPU-rendered structured table from NOS Shell output.
    StructuredTable(StructuredTableContent),
    /// Universal command suggestion card popup.
    SuggestionCard(SuggestionCardContent),
}

/// A structured table for GPU rendering (from NOS Shell IPC).
#[derive(Debug, Clone)]
pub struct StructuredTableContent {
    /// Column headers.
    pub columns: Vec<String>,
    /// Rows of cell values (each row is a Vec matching columns).
    pub rows: Vec<Vec<String>>,
    /// Row on screen where the table should start rendering.
    pub start_row: usize,
    /// Column widths (calculated from content).
    pub col_widths: Vec<usize>,
}

impl StructuredTableContent {
    /// Parse a JSON string from NOS Shell into a structured table.
    /// Returns None if the JSON doesn't represent tabular data.
    pub fn from_json(json: &str, cursor_row: usize) -> Option<Self> {
        // Minimal JSON array-of-objects parser for NOS Shell IPC
        let json = json.trim();
        if !json.starts_with('[') {
            // Single object — show as key-value table
            if json.starts_with('{') {
                return Self::parse_object_table(json, cursor_row);
            }
            return None;
        }

        Self::parse_array_table(json, cursor_row)
    }

    fn parse_object_table(json: &str, cursor_row: usize) -> Option<Self> {
        let pairs = Self::extract_kv_pairs(json)?;
        let columns = vec!["Key".to_string(), "Value".to_string()];
        let rows: Vec<Vec<String>> = pairs
            .iter()
            .map(|(k, v)| vec![k.clone(), v.clone()])
            .collect();

        let mut col_widths = vec![3, 5]; // min widths for "Key", "Value"
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cell.len());
                }
            }
        }

        Some(Self { columns, rows, start_row: cursor_row, col_widths })
    }

    fn parse_array_table(json: &str, cursor_row: usize) -> Option<Self> {
        // Extract objects from the array
        let inner = json.trim_start_matches('[').trim_end_matches(']').trim();
        if inner.is_empty() {
            return None;
        }

        // Split into individual objects — handle nested braces
        let objects = Self::split_objects(inner);
        if objects.is_empty() {
            return None;
        }

        // Extract columns from first object
        let first_pairs = Self::extract_kv_pairs(&objects[0])?;
        let columns: Vec<String> = first_pairs.iter().map(|(k, _)| k.clone()).collect();

        // Extract rows
        let mut rows: Vec<Vec<String>> = Vec::new();
        for obj in &objects {
            let pairs = Self::extract_kv_pairs(obj).unwrap_or_default();
            let row: Vec<String> = columns
                .iter()
                .map(|col| {
                    pairs
                        .iter()
                        .find(|(k, _)| k == col)
                        .map(|(_, v)| v.clone())
                        .unwrap_or_default()
                })
                .collect();
            rows.push(row);
        }

        // Calculate column widths
        let mut col_widths: Vec<usize> = columns.iter().map(|c| c.len()).collect();
        for row in &rows {
            for (i, cell) in row.iter().enumerate() {
                if i < col_widths.len() {
                    col_widths[i] = col_widths[i].max(cell.len());
                }
            }
        }

        // Limit widths to 30 chars
        for w in &mut col_widths {
            *w = (*w).min(30);
        }

        Some(Self { columns, rows, start_row: cursor_row, col_widths })
    }

    /// Split a comma-separated list of JSON objects, handling nested braces.
    fn split_objects(s: &str) -> Vec<String> {
        let mut objects = Vec::new();
        let mut depth = 0;
        let mut start = 0;

        for (i, ch) in s.char_indices() {
            match ch {
                '{' => {
                    if depth == 0 {
                        start = i;
                    }
                    depth += 1;
                }
                '}' => {
                    depth -= 1;
                    if depth == 0 {
                        objects.push(s[start..=i].to_string());
                    }
                }
                _ => {}
            }
        }
        objects
    }

    /// Extract key-value pairs from a JSON object string.
    fn extract_kv_pairs(json: &str) -> Option<Vec<(String, String)>> {
        let inner = json.trim().trim_start_matches('{').trim_end_matches('}').trim();
        if inner.is_empty() {
            return None;
        }

        let mut pairs = Vec::new();
        let mut chars = inner.chars().peekable();

        loop {
            // Skip whitespace
            while chars.peek().map(|c| c.is_whitespace() || *c == ',').unwrap_or(false) {
                chars.next();
            }
            if chars.peek().is_none() {
                break;
            }

            // Parse key (quoted string)
            if chars.peek() != Some(&'"') {
                break;
            }
            chars.next(); // consume opening "
            let mut key = String::new();
            while let Some(&ch) = chars.peek() {
                if ch == '"' {
                    chars.next();
                    break;
                }
                if ch == '\\' {
                    chars.next();
                    if let Some(&esc) = chars.peek() {
                        key.push(esc);
                        chars.next();
                    }
                } else {
                    key.push(ch);
                    chars.next();
                }
            }

            // Skip colon
            while chars.peek().map(|c| c.is_whitespace() || *c == ':').unwrap_or(false) {
                chars.next();
            }

            // Parse value
            let value = Self::parse_json_value(&mut chars);
            pairs.push((key, value));
        }

        if pairs.is_empty() {
            None
        } else {
            Some(pairs)
        }
    }

    /// Parse a JSON value and return its display string.
    fn parse_json_value(chars: &mut std::iter::Peekable<std::str::Chars>) -> String {
        match chars.peek() {
            Some('"') => {
                // String
                chars.next();
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch == '"' {
                        chars.next();
                        break;
                    }
                    if ch == '\\' {
                        chars.next();
                        if let Some(&esc) = chars.peek() {
                            match esc {
                                'n' => s.push('\n'),
                                't' => s.push('\t'),
                                'r' => s.push('\r'),
                                _ => s.push(esc),
                            }
                            chars.next();
                        }
                    } else {
                        s.push(ch);
                        chars.next();
                    }
                }
                s
            }
            Some('{') => {
                // Nested object — check if it's a tagged type (__type)
                let mut depth = 0;
                let mut obj_str = String::new();
                while let Some(&ch) = chars.peek() {
                    obj_str.push(ch);
                    chars.next();
                    if ch == '{' { depth += 1; }
                    if ch == '}' { depth -= 1; if depth == 0 { break; } }
                }
                // Check for __type: "Size" or "Duration"
                if obj_str.contains("\"__type\"") {
                    if let Some(display_start) = obj_str.find("\"display\":\"") {
                        let rest = &obj_str[display_start + 11..];
                        if let Some(end) = rest.find('"') {
                            return rest[..end].to_string();
                        }
                    }
                }
                obj_str
            }
            Some('[') => {
                // Array
                let mut depth = 0;
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    s.push(ch);
                    chars.next();
                    if ch == '[' { depth += 1; }
                    if ch == ']' { depth -= 1; if depth == 0 { break; } }
                }
                s
            }
            _ => {
                // Number, bool, null
                let mut s = String::new();
                while let Some(&ch) = chars.peek() {
                    if ch == ',' || ch == '}' || ch == ']' {
                        break;
                    }
                    s.push(ch);
                    chars.next();
                }
                s.trim().to_string()
            }
        }
    }
}

/// Status bar at the bottom of the terminal.
#[derive(Debug, Clone)]
pub struct StatusBarContent {
    pub left: String,
    pub center: String,
    pub right: String,
    pub bg_color: [f32; 4],
    pub fg_color: [f32; 4],
}

impl StatusBarContent {
    /// Build status bar from current app state.
    pub fn from_state(
        pane_count: usize,
        active_pane: usize,
        tab_count: usize,
        active_tab: usize,
        shell: &str,
        grid_cols: usize,
        grid_rows: usize,
    ) -> Self {
        let left = if pane_count > 1 {
            format!(" Pane {}/{} ", active_pane + 1, pane_count)
        } else {
            String::from(" Stratum ")
        };

        let center = if tab_count > 1 {
            format!("Tab {}/{}", active_tab + 1, tab_count)
        } else {
            String::new()
        };

        let right = format!(" {}×{} │ Stratum v{} ", grid_cols, grid_rows, env!("CARGO_PKG_VERSION"));

        Self {
            left,
            center,
            right,
            bg_color: [0.15, 0.15, 0.22, 0.95],
            fg_color: [0.75, 0.78, 0.85, 1.0],
        }
    }
}

/// Inline documentation popup content.
#[derive(Debug, Clone)]
pub struct InlineDocContent {
    pub command: String,
    pub synopsis: String,
    pub flags: Vec<InlineDocFlag>,
    pub completions: Vec<String>,
    pub history: Vec<String>,
    pub position: OverlayPosition,
}

#[derive(Debug, Clone)]
pub struct InlineDocFlag {
    pub flag: String,
    pub description: String,
}

/// Mutation preview panel content.
#[derive(Debug, Clone)]
pub struct MutationPreviewContent {
    pub command: String,
    pub summary_line: String,
    pub changes: Vec<MutationChangeLine>,
    pub has_destructive: bool,
    pub position: OverlayPosition,
}

#[derive(Debug, Clone)]
pub struct MutationChangeLine {
    pub icon: char,
    pub text: String,
    pub color: [f32; 4],
}

impl MutationChangeLine {
    pub fn create(text: String) -> Self {
        Self {
            icon: '+',
            text,
            color: [0.3, 0.9, 0.4, 1.0], // green
        }
    }

    pub fn delete(text: String) -> Self {
        Self {
            icon: '-',
            text,
            color: [0.95, 0.3, 0.3, 1.0], // red
        }
    }

    pub fn modify(text: String) -> Self {
        Self {
            icon: '~',
            text,
            color: [0.9, 0.8, 0.3, 1.0], // yellow
        }
    }

    pub fn r#move(text: String) -> Self {
        Self {
            icon: '→',
            text,
            color: [0.4, 0.7, 1.0, 1.0], // blue
        }
    }
}

/// Consequence warning dialog.
#[derive(Debug, Clone)]
pub struct ConsequenceWarningContent {
    pub command: String,
    pub risk_level: String,
    pub risk_color: [f32; 4],
    pub reversibility: String,
    pub blast_radius: String,
    pub message: String,
    pub requires_confirmation: bool,
}

/// Toast notification.
#[derive(Debug, Clone)]
pub struct ToastContent {
    pub message: String,
    pub color: [f32; 4],
    pub duration_ms: u32,
    pub created_at: std::time::Instant,
}

impl ToastContent {
    pub fn info(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            color: [0.3, 0.6, 1.0, 1.0],
            duration_ms: 3000,
            created_at: std::time::Instant::now(),
        }
    }

    pub fn warning(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            color: [1.0, 0.7, 0.2, 1.0],
            duration_ms: 5000,
            created_at: std::time::Instant::now(),
        }
    }

    pub fn is_expired(&self) -> bool {
        self.created_at.elapsed() > std::time::Duration::from_millis(self.duration_ms as u64)
    }
}

/// Position of an overlay element.
#[derive(Debug, Clone)]
pub enum OverlayPosition {
    /// Below the cursor line.
    BelowCursor { col: usize, row: usize },
    /// Above the cursor line.
    AboveCursor { col: usize, row: usize },
    /// Bottom of the screen.
    Bottom,
    /// Right side panel.
    RightPanel { width_fraction: f32 },
    /// Center of screen (modal).
    Center,
}

/// Suggestion card overlay content.
#[derive(Debug, Clone)]
pub struct SuggestionCardContent {
    /// The command being completed.
    pub command: String,
    /// Items to display in the card.
    pub items: Vec<SuggestionCardItem>,
    /// Currently selected index.
    pub selected_index: i32,
    /// Position of the card.
    pub position: OverlayPosition,
    /// Scroll offset for long lists.
    pub scroll_offset: usize,
    /// Max visible items.
    pub max_visible: usize,
}

/// A single item in the suggestion card.
#[derive(Debug, Clone)]
pub struct SuggestionCardItem {
    /// Icon character.
    pub icon: char,
    /// Display label.
    pub label: String,
    /// Detail/description text.
    pub detail: Option<String>,
    /// Color for this item.
    pub color: [f32; 4],
}

/// Manages all active overlays.
pub struct OverlayManager {
    /// Active overlay elements, rendered in order (bottom to top).
    pub elements: Vec<OverlayElement>,
    /// Whether the status bar should be shown.
    pub show_status_bar: bool,
    /// Whether inline docs are enabled.
    pub show_inline_docs: bool,
    /// Whether mutation preview is enabled.
    pub show_mutation_preview: bool,
}

impl OverlayManager {
    pub fn new() -> Self {
        Self {
            elements: Vec::new(),
            show_status_bar: true,
            show_inline_docs: true,
            show_mutation_preview: true,
        }
    }

    /// Clear all transient overlays (keep status bar).
    pub fn clear_transient(&mut self) {
        self.elements.retain(|e| matches!(e, OverlayElement::StatusBar(_)));
    }

    /// Update the status bar.
    pub fn set_status_bar(&mut self, content: StatusBarContent) {
        // Remove old status bar
        self.elements.retain(|e| !matches!(e, OverlayElement::StatusBar(_)));
        if self.show_status_bar {
            self.elements.insert(0, OverlayElement::StatusBar(content));
        }
    }

    /// Show inline documentation.
    pub fn show_doc(&mut self, content: InlineDocContent) {
        if !self.show_inline_docs {
            return;
        }
        self.elements.retain(|e| !matches!(e, OverlayElement::InlineDoc(_)));
        self.elements.push(OverlayElement::InlineDoc(content));
    }

    /// Hide inline documentation.
    pub fn hide_doc(&mut self) {
        self.elements.retain(|e| !matches!(e, OverlayElement::InlineDoc(_)));
    }

    /// Show mutation preview.
    pub fn show_mutation(&mut self, content: MutationPreviewContent) {
        if !self.show_mutation_preview {
            return;
        }
        self.elements.retain(|e| !matches!(e, OverlayElement::MutationPreview(_)));
        self.elements.push(OverlayElement::MutationPreview(content));
    }

    /// Hide mutation preview.
    pub fn hide_mutation(&mut self) {
        self.elements.retain(|e| !matches!(e, OverlayElement::MutationPreview(_)));
    }

    /// Show consequence warning.
    pub fn show_consequence(&mut self, content: ConsequenceWarningContent) {
        self.elements.retain(|e| !matches!(e, OverlayElement::ConsequenceWarning(_)));
        self.elements.push(OverlayElement::ConsequenceWarning(content));
    }

    /// Dismiss consequence warning.
    pub fn dismiss_consequence(&mut self) {
        self.elements.retain(|e| !matches!(e, OverlayElement::ConsequenceWarning(_)));
    }

    /// Add a toast notification.
    pub fn toast(&mut self, content: ToastContent) {
        self.elements.push(OverlayElement::Toast(content));
    }

    /// Remove expired toasts.
    pub fn cleanup_toasts(&mut self) {
        self.elements.retain(|e| {
            if let OverlayElement::Toast(t) = e {
                !t.is_expired()
            } else {
                true
            }
        });
    }

    /// Check if a modal (consequence warning) is active.
    pub fn has_modal(&self) -> bool {
        self.elements.iter().any(|e| matches!(e, OverlayElement::ConsequenceWarning(_)))
    }

    /// Show suggestion card.
    pub fn show_suggestion_card(&mut self, content: SuggestionCardContent) {
        self.elements.retain(|e| !matches!(e, OverlayElement::SuggestionCard(_)));
        self.elements.push(OverlayElement::SuggestionCard(content));
    }

    /// Hide suggestion card.
    pub fn hide_suggestion_card(&mut self) {
        self.elements.retain(|e| !matches!(e, OverlayElement::SuggestionCard(_)));
    }
}

impl Default for OverlayManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_bar() {
        let mut manager = OverlayManager::new();
        let bar = StatusBarContent::from_state(1, 0, 1, 0, "bash", 80, 24);
        manager.set_status_bar(bar);
        assert_eq!(manager.elements.len(), 1);
    }

    #[test]
    fn test_toast_expiry() {
        let toast = ToastContent {
            message: "test".into(),
            color: [1.0; 4],
            duration_ms: 0, // expires immediately
            created_at: std::time::Instant::now() - std::time::Duration::from_secs(1),
        };
        assert!(toast.is_expired());
    }

    #[test]
    fn test_modal_check() {
        let mut manager = OverlayManager::new();
        assert!(!manager.has_modal());

        manager.show_consequence(ConsequenceWarningContent {
            command: "rm -rf /".into(),
            risk_level: "CRITICAL".into(),
            risk_color: [1.0, 0.0, 0.0, 1.0],
            reversibility: "IRREVERSIBLE".into(),
            blast_radius: "SYSTEM".into(),
            message: "This will destroy your system.".into(),
            requires_confirmation: true,
        });
        assert!(manager.has_modal());

        manager.dismiss_consequence();
        assert!(!manager.has_modal());
    }
}
