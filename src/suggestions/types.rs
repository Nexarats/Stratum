//! Core types for the universal command suggestion system.
//!
//! Every command gets suggestions via three layers:
//!   1. Static   — built-in docs, flags, synopsis (inline_docs.rs)
//!   2. Dynamic  — contextual completions from providers (filesystem, git, docker, etc.)
//!   3. AI       — intelligent suggestions from history + AI model
//!
//! Each provider implements `ContextProvider` and returns `CompletionItem`s.

/// A single completion/suggestion item shown in the popup card.
#[derive(Debug, Clone)]
pub struct CompletionItem {
    /// Display label (e.g., "main", "src/", "README.md", "--force").
    pub label: String,
    /// Category of this completion.
    pub kind: CompletionKind,
    /// Optional detail text shown to the right (e.g., "(branch)", "4.2 KB").
    pub detail: Option<String>,
    /// Icon character for rendering.
    pub icon: char,
    /// The text to insert when selected (may differ from label).
    pub insert_text: String,
    /// Sort priority (lower = higher in list). Used to order items.
    pub priority: u32,
}

impl CompletionItem {
    /// Create a directory completion.
    pub fn directory(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            insert_text: format!("{}/", name),
            label: format!("{}/", name),
            kind: CompletionKind::Directory,
            detail: None,
            icon: '\u{1F4C1}', // 📁
            priority: 10,
        }
    }

    /// Create a file completion.
    pub fn file(name: impl Into<String>, size: Option<String>) -> Self {
        let name = name.into();
        Self {
            insert_text: name.clone(),
            label: name,
            kind: CompletionKind::File,
            detail: size,
            icon: '\u{1F4C4}', // 📄
            priority: 20,
        }
    }

    /// Create a branch completion.
    pub fn branch(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            insert_text: name.clone(),
            label: name,
            kind: CompletionKind::Branch,
            detail: Some("(branch)".into()),
            icon: '\u{1F33F}', // 🌿
            priority: 10,
        }
    }

    /// Create a flag completion.
    pub fn flag(flag: impl Into<String>, description: impl Into<String>) -> Self {
        let flag = flag.into();
        Self {
            insert_text: flag.clone(),
            label: flag,
            kind: CompletionKind::Flag,
            detail: Some(description.into()),
            icon: '-',
            priority: 30,
        }
    }

    /// Create a subcommand completion.
    pub fn subcommand(name: impl Into<String>, description: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            insert_text: name.clone(),
            label: name,
            kind: CompletionKind::Subcommand,
            detail: Some(description.into()),
            icon: '\u{25B6}', // ▶
            priority: 5,
        }
    }

    /// Create a process completion.
    pub fn process(name: impl Into<String>, pid: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            insert_text: name.clone(),
            label: name,
            kind: CompletionKind::Process,
            detail: Some(format!("PID {}", pid.into())),
            icon: '\u{1F534}', // 🔴
            priority: 10,
        }
    }

    /// Create a container completion.
    pub fn container(name: impl Into<String>, status: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            insert_text: name.clone(),
            label: name,
            kind: CompletionKind::Container,
            detail: Some(status.into()),
            icon: '\u{1F433}', // 🐳
            priority: 10,
        }
    }

    /// Create a script completion (from package.json, Makefile, etc.).
    pub fn script(name: impl Into<String>, source: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            insert_text: name.clone(),
            label: name,
            kind: CompletionKind::Script,
            detail: Some(source.into()),
            icon: '\u{1F3C3}', // 🏃
            priority: 10,
        }
    }

    /// Create an environment variable completion.
    pub fn env_var(name: impl Into<String>, value_preview: Option<String>) -> Self {
        let name = name.into();
        Self {
            insert_text: name.clone(),
            label: name,
            kind: CompletionKind::EnvVar,
            detail: value_preview,
            icon: '\u{1F511}', // 🔑
            priority: 20,
        }
    }

    /// Create a host completion (from SSH config).
    pub fn host(name: impl Into<String>, hostname: Option<String>) -> Self {
        let name = name.into();
        Self {
            insert_text: name.clone(),
            label: name,
            kind: CompletionKind::Host,
            detail: hostname,
            icon: '\u{1F5A5}', // 🖥
            priority: 10,
        }
    }

    /// Create a history-based suggestion.
    pub fn history(command: impl Into<String>) -> Self {
        let command = command.into();
        Self {
            insert_text: command.clone(),
            label: command,
            kind: CompletionKind::History,
            detail: Some("(history)".into()),
            icon: '\u{1F552}', // 🕒
            priority: 50,
        }
    }

    /// Create a target completion (Makefile, Cargo, etc.).
    pub fn target(name: impl Into<String>, source: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            insert_text: name.clone(),
            label: name,
            kind: CompletionKind::Target,
            detail: Some(source.into()),
            icon: '\u{1F3AF}', // 🎯
            priority: 10,
        }
    }

    /// Create an image completion (Docker).
    pub fn image(name: impl Into<String>, tag: Option<String>) -> Self {
        let name = name.into();
        Self {
            insert_text: name.clone(),
            label: name,
            kind: CompletionKind::Image,
            detail: tag,
            icon: '\u{1F4E6}', // 📦
            priority: 15,
        }
    }
}

/// Categories for completion items — determines icon and rendering style.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompletionKind {
    Directory,
    File,
    Branch,
    Tag,
    Container,
    Image,
    Service,
    Process,
    Package,
    Script,
    Flag,
    Subcommand,
    EnvVar,
    Host,
    Target,
    History,
    AiSuggestion,
}

impl CompletionKind {
    /// Get the rendering color for this kind.
    pub fn color(&self) -> [f32; 4] {
        match self {
            Self::Directory  => [0.35, 0.75, 1.0, 1.0],  // blue
            Self::File       => [0.75, 0.78, 0.85, 1.0],  // light gray
            Self::Branch     => [0.4, 0.9, 0.5, 1.0],     // green
            Self::Tag        => [0.9, 0.8, 0.3, 1.0],     // yellow
            Self::Container  => [0.3, 0.7, 0.9, 1.0],     // docker blue
            Self::Image      => [0.6, 0.5, 0.9, 1.0],     // purple
            Self::Service    => [0.8, 0.6, 0.3, 1.0],     // orange
            Self::Process    => [0.95, 0.3, 0.3, 1.0],    // red
            Self::Package    => [0.3, 0.85, 0.7, 1.0],    // teal
            Self::Script     => [0.9, 0.65, 0.2, 1.0],    // amber
            Self::Flag       => [0.65, 0.7, 0.78, 1.0],   // muted gray
            Self::Subcommand => [0.85, 0.85, 0.95, 1.0],  // bright white
            Self::EnvVar     => [0.9, 0.75, 0.4, 1.0],    // gold
            Self::Host       => [0.5, 0.8, 0.95, 1.0],    // sky blue
            Self::Target     => [0.7, 0.4, 0.9, 1.0],     // violet
            Self::History    => [0.6, 0.6, 0.65, 1.0],    // dim
            Self::AiSuggestion => [0.4, 0.95, 0.85, 1.0], // cyan
        }
    }

    /// Get the icon character for this kind.
    pub fn icon(&self) -> char {
        match self {
            Self::Directory    => '\u{25A0}', // ■ folder
            Self::File         => '\u{25CB}', // ○ file
            Self::Branch       => '\u{2387}', // ⎇ branch
            Self::Tag          => '\u{2605}', // ★ tag
            Self::Container    => '\u{25A3}', // ▣ container
            Self::Image        => '\u{25A6}', // ▦ image
            Self::Service      => '\u{2699}', // ⚙ service
            Self::Process      => '\u{25CF}', // ● process
            Self::Package      => '\u{25A1}', // □ package
            Self::Script       => '\u{25B7}', // ▷ script
            Self::Flag         => '-',         // flag
            Self::Subcommand   => '\u{25B6}', // ▶ subcommand
            Self::EnvVar       => '$',         // $ env
            Self::Host         => '@',         // @ host
            Self::Target       => '\u{25CE}', // ◎ target
            Self::History      => '\u{21BA}', // ↺ history
            Self::AiSuggestion => '\u{2726}', // ✦ AI
        }
    }
}

/// The state of the suggestion card popup.
#[derive(Debug, Clone)]
pub struct SuggestionCardState {
    /// All suggestion items currently available.
    pub items: Vec<CompletionItem>,
    /// Currently selected index (-1 = none).
    pub selected_index: i32,
    /// The command being completed.
    pub command: String,
    /// The partial text being matched against.
    pub filter_text: String,
    /// Whether the card is visible.
    pub visible: bool,
    /// Maximum items to show (scroll if more).
    pub max_visible: usize,
    /// Scroll offset for long lists.
    pub scroll_offset: usize,
}

impl SuggestionCardState {
    pub fn new() -> Self {
        Self {
            items: Vec::new(),
            selected_index: -1,
            command: String::new(),
            filter_text: String::new(),
            visible: false,
            max_visible: 12,
            scroll_offset: 0,
        }
    }

    /// Show the card with new items.
    pub fn show(&mut self, command: String, filter_text: String, items: Vec<CompletionItem>) {
        self.command = command;
        self.filter_text = filter_text;
        self.items = items;
        self.selected_index = if self.items.is_empty() { -1 } else { 0 };
        self.visible = !self.items.is_empty();
        self.scroll_offset = 0;
    }

    /// Hide the card.
    pub fn hide(&mut self) {
        self.visible = false;
        self.items.clear();
        self.selected_index = -1;
        self.scroll_offset = 0;
    }

    /// Move selection up.
    pub fn select_prev(&mut self) {
        if self.items.is_empty() {
            return;
        }
        if self.selected_index <= 0 {
            self.selected_index = self.items.len() as i32 - 1;
        } else {
            self.selected_index -= 1;
        }
        self.ensure_visible();
    }

    /// Move selection down.
    pub fn select_next(&mut self) {
        if self.items.is_empty() {
            return;
        }
        self.selected_index = (self.selected_index + 1) % self.items.len() as i32;
        self.ensure_visible();
    }

    /// Get the currently selected item.
    pub fn selected_item(&self) -> Option<&CompletionItem> {
        if self.selected_index >= 0 {
            self.items.get(self.selected_index as usize)
        } else {
            None
        }
    }

    /// Get the insert text for the selected item.
    pub fn accept_selected(&self) -> Option<String> {
        self.selected_item().map(|item| item.insert_text.clone())
    }

    /// Visible items (respecting scroll offset).
    pub fn visible_items(&self) -> &[CompletionItem] {
        let end = (self.scroll_offset + self.max_visible).min(self.items.len());
        &self.items[self.scroll_offset..end]
    }

    /// Ensure the selected item is within the visible window.
    fn ensure_visible(&mut self) {
        if self.selected_index < 0 {
            return;
        }
        let idx = self.selected_index as usize;
        if idx < self.scroll_offset {
            self.scroll_offset = idx;
        } else if idx >= self.scroll_offset + self.max_visible {
            self.scroll_offset = idx - self.max_visible + 1;
        }
    }
}

impl Default for SuggestionCardState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_item_directory() {
        let item = CompletionItem::directory("src");
        assert_eq!(item.label, "src/");
        assert_eq!(item.insert_text, "src/");
        assert_eq!(item.kind, CompletionKind::Directory);
    }

    #[test]
    fn test_completion_item_branch() {
        let item = CompletionItem::branch("main");
        assert_eq!(item.label, "main");
        assert_eq!(item.kind, CompletionKind::Branch);
        assert_eq!(item.detail, Some("(branch)".into()));
    }

    #[test]
    fn test_suggestion_card_navigation() {
        let mut card = SuggestionCardState::new();
        card.show(
            "cd".into(),
            "".into(),
            vec![
                CompletionItem::directory("src"),
                CompletionItem::directory("target"),
                CompletionItem::directory("docs"),
            ],
        );

        assert!(card.visible);
        assert_eq!(card.selected_index, 0);

        card.select_next();
        assert_eq!(card.selected_index, 1);

        card.select_next();
        assert_eq!(card.selected_index, 2);

        // Wrap around
        card.select_next();
        assert_eq!(card.selected_index, 0);

        // Wrap up
        card.select_prev();
        assert_eq!(card.selected_index, 2);
    }

    #[test]
    fn test_accept_selected() {
        let mut card = SuggestionCardState::new();
        card.show(
            "cd".into(),
            "".into(),
            vec![CompletionItem::directory("src")],
        );
        assert_eq!(card.accept_selected(), Some("src/".into()));
    }

    #[test]
    fn test_kind_colors() {
        // Ensure all kinds have non-zero alpha
        let kinds = [
            CompletionKind::Directory, CompletionKind::File, CompletionKind::Branch,
            CompletionKind::Flag, CompletionKind::Process, CompletionKind::Container,
        ];
        for kind in kinds {
            let color = kind.color();
            assert!(color[3] > 0.0, "Kind {:?} has zero alpha", kind);
        }
    }
}
