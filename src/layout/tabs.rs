//! Tab management — each tab contains its own pane tree.

/// Unique identifier for a tab.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct TabId(pub u32);

/// A single tab containing a title and associated pane tree ID.
pub struct Tab {
    pub id: TabId,
    pub title: String,
}

/// Manages multiple tabs.
pub struct TabManager {
    tabs: Vec<Tab>,
    active_tab: usize,
    next_id: u32,
}

impl TabManager {
    /// Create a tab manager with one initial tab.
    pub fn new() -> Self {
        let initial_tab = Tab {
            id: TabId(0),
            title: String::from("Terminal"),
        };
        Self {
            tabs: vec![initial_tab],
            active_tab: 0,
            next_id: 1,
        }
    }

    /// Get the active tab.
    pub fn active_tab(&self) -> &Tab {
        &self.tabs[self.active_tab]
    }

    /// Get the active tab ID.
    pub fn active_tab_id(&self) -> TabId {
        self.tabs[self.active_tab].id
    }

    /// Get the active tab index.
    pub fn active_index(&self) -> usize {
        self.active_tab
    }

    /// Create a new tab and make it active.
    pub fn new_tab(&mut self) -> TabId {
        let id = TabId(self.next_id);
        self.next_id += 1;

        let tab = Tab {
            id,
            title: format!("Terminal {}", self.next_id),
        };
        self.tabs.push(tab);
        self.active_tab = self.tabs.len() - 1;
        id
    }

    /// Close the active tab.
    /// Returns true if successfully closed, false if it's the last tab.
    pub fn close_active(&mut self) -> bool {
        if self.tabs.len() <= 1 {
            return false;
        }

        self.tabs.remove(self.active_tab);
        if self.active_tab >= self.tabs.len() {
            self.active_tab = self.tabs.len() - 1;
        }
        true
    }

    /// Switch to the next tab.
    pub fn next_tab(&mut self) {
        self.active_tab = (self.active_tab + 1) % self.tabs.len();
    }

    /// Switch to the previous tab.
    pub fn prev_tab(&mut self) {
        if self.active_tab == 0 {
            self.active_tab = self.tabs.len() - 1;
        } else {
            self.active_tab -= 1;
        }
    }

    /// Switch to a specific tab by index.
    pub fn switch_to(&mut self, index: usize) {
        if index < self.tabs.len() {
            self.active_tab = index;
        }
    }

    /// Get all tabs.
    pub fn tabs(&self) -> &[Tab] {
        &self.tabs
    }

    /// Get the number of tabs.
    pub fn tab_count(&self) -> usize {
        self.tabs.len()
    }

    /// Set the title of the active tab.
    pub fn set_active_title(&mut self, title: String) {
        self.tabs[self.active_tab].title = title;
    }
}

impl Default for TabManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_initial_state() {
        let manager = TabManager::new();
        assert_eq!(manager.tab_count(), 1);
        assert_eq!(manager.active_index(), 0);
        assert_eq!(manager.active_tab().title, "Terminal");
    }

    #[test]
    fn test_new_tab() {
        let mut manager = TabManager::new();
        let id = manager.new_tab();
        assert_eq!(manager.tab_count(), 2);
        assert_eq!(manager.active_tab_id(), id);
    }

    #[test]
    fn test_tab_cycling() {
        let mut manager = TabManager::new();
        manager.new_tab();
        manager.new_tab();

        assert_eq!(manager.active_index(), 2);
        manager.next_tab();
        assert_eq!(manager.active_index(), 0);
        manager.prev_tab();
        assert_eq!(manager.active_index(), 2);
    }

    #[test]
    fn test_close_tab() {
        let mut manager = TabManager::new();
        manager.new_tab();
        assert_eq!(manager.tab_count(), 2);

        assert!(manager.close_active());
        assert_eq!(manager.tab_count(), 1);

        // Can't close last tab
        assert!(!manager.close_active());
        assert_eq!(manager.tab_count(), 1);
    }
}
