//! Pane management — binary tree layout for split panes.
//!
//! Panes are arranged in a binary tree where each internal node
//! represents a split (horizontal or vertical) and each leaf
//! represents a terminal pane with its own PTY session and screen.

use std::collections::HashMap;

/// Unique identifier for a pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PaneId(pub u32);

/// Split direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Direction {
    Horizontal,
    Vertical,
}

/// A rectangle representing a pane's area on screen (in pixels).
#[derive(Debug, Clone, Copy)]
pub struct PaneRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl PaneRect {
    /// Split this rectangle into two halves.
    pub fn split(&self, direction: Direction, ratio: f32) -> (PaneRect, PaneRect) {
        match direction {
            Direction::Horizontal => {
                let split_y = self.y + self.height * ratio;
                (
                    PaneRect {
                        x: self.x,
                        y: self.y,
                        width: self.width,
                        height: self.height * ratio,
                    },
                    PaneRect {
                        x: self.x,
                        y: split_y,
                        width: self.width,
                        height: self.height * (1.0 - ratio),
                    },
                )
            }
            Direction::Vertical => {
                let split_x = self.x + self.width * ratio;
                (
                    PaneRect {
                        x: self.x,
                        y: self.y,
                        width: self.width * ratio,
                        height: self.height,
                    },
                    PaneRect {
                        x: split_x,
                        y: self.y,
                        width: self.width * (1.0 - ratio),
                        height: self.height,
                    },
                )
            }
        }
    }

    /// Convert pixel dimensions to terminal grid size.
    pub fn grid_size(&self, cell_width: f32, cell_height: f32) -> (usize, usize) {
        let cols = (self.width / cell_width).floor() as usize;
        let rows = (self.height / cell_height).floor() as usize;
        (cols.max(1), rows.max(1))
    }
}

/// Node in the pane tree.
enum PaneNode {
    /// A leaf node — represents an actual terminal pane.
    Leaf(PaneId),
    /// An internal node — splits space between two children.
    Split {
        direction: Direction,
        ratio: f32,
        first: Box<PaneNode>,
        second: Box<PaneNode>,
    },
}

/// Binary tree managing pane layout.
pub struct PaneTree {
    root: PaneNode,
    next_id: u32,
    active_pane: PaneId,
    /// Computed rectangles for each pane (updated on layout).
    rects: HashMap<PaneId, PaneRect>,
}

impl PaneTree {
    /// Create a pane tree with a single initial pane.
    pub fn new() -> Self {
        let initial_id = PaneId(0);
        Self {
            root: PaneNode::Leaf(initial_id),
            next_id: 1,
            active_pane: initial_id,
            rects: HashMap::new(),
        }
    }

    /// Get the active pane ID.
    pub fn active_pane(&self) -> PaneId {
        self.active_pane
    }

    /// Set the active pane.
    pub fn set_active(&mut self, id: PaneId) {
        self.active_pane = id;
    }

    /// Split the active pane in the given direction.
    /// Returns the ID of the new pane created.
    pub fn split(&mut self, direction: Direction) -> PaneId {
        let new_id = PaneId(self.next_id);
        self.next_id += 1;

        let target = self.active_pane;
        self.root = Self::split_node(self.root.take(), target, new_id, direction);

        new_id
    }

    /// Recursively find and split the target pane.
    fn split_node(
        node: PaneNode,
        target: PaneId,
        new_id: PaneId,
        direction: Direction,
    ) -> PaneNode {
        match node {
            PaneNode::Leaf(id) if id == target => PaneNode::Split {
                direction,
                ratio: 0.5,
                first: Box::new(PaneNode::Leaf(id)),
                second: Box::new(PaneNode::Leaf(new_id)),
            },
            PaneNode::Split {
                direction: dir,
                ratio,
                first,
                second,
            } => PaneNode::Split {
                direction: dir,
                ratio,
                first: Box::new(Self::split_node(*first, target, new_id, direction)),
                second: Box::new(Self::split_node(*second, target, new_id, direction)),
            },
            other => other,
        }
    }

    /// Compute layout rectangles for all panes.
    pub fn layout(&mut self, total_rect: PaneRect) {
        self.rects.clear();
        Self::compute_layout(&self.root, total_rect, &mut self.rects);
    }

    fn compute_layout(
        node: &PaneNode,
        rect: PaneRect,
        rects: &mut HashMap<PaneId, PaneRect>,
    ) {
        match node {
            PaneNode::Leaf(id) => {
                rects.insert(*id, rect);
            }
            PaneNode::Split {
                direction,
                ratio,
                first,
                second,
            } => {
                let (r1, r2) = rect.split(*direction, *ratio);
                Self::compute_layout(first, r1, rects);
                Self::compute_layout(second, r2, rects);
            }
        }
    }

    /// Get the computed rectangle for a pane.
    pub fn get_rect(&self, id: PaneId) -> Option<&PaneRect> {
        self.rects.get(&id)
    }

    /// Get all pane IDs.
    pub fn all_panes(&self) -> Vec<PaneId> {
        self.rects.keys().copied().collect()
    }

    /// Get the number of panes.
    pub fn pane_count(&self) -> usize {
        self.rects.len()
    }

    /// Close a pane, removing it from the tree.
    /// Returns true if the pane was closed, false if it's the last pane.
    pub fn close(&mut self, id: PaneId) -> bool {
        if self.pane_count() <= 1 {
            return false;
        }

        if let Some(new_root) = Self::remove_node(&self.root, id) {
            self.root = new_root;
            // If we closed the active pane, switch to any remaining pane
            if self.active_pane == id {
                if let Some(first) = self.rects.keys().next() {
                    self.active_pane = *first;
                }
            }
            true
        } else {
            false
        }
    }

    fn remove_node(node: &PaneNode, target: PaneId) -> Option<PaneNode> {
        match node {
            PaneNode::Leaf(id) if *id == target => None,
            PaneNode::Leaf(_) => None, // Not found in this branch
            PaneNode::Split {
                direction: _,
                ratio: _,
                first,
                second,
            } => {
                // Check if either child is the target
                if matches!(first.as_ref(), PaneNode::Leaf(id) if *id == target) {
                    // Remove first, keep second
                    Some(second.deep_clone())
                } else if matches!(second.as_ref(), PaneNode::Leaf(id) if *id == target) {
                    // Remove second, keep first
                    Some(first.deep_clone())
                } else {
                    // Recurse into children
                    None
                }
            }
        }
    }
}

impl Default for PaneTree {
    fn default() -> Self {
        Self::new()
    }
}

impl PaneNode {
    /// Take ownership of the node, leaving a dummy in place.
    fn take(&mut self) -> PaneNode {
        std::mem::replace(self, PaneNode::Leaf(PaneId(u32::MAX)))
    }

    /// Deep clone the node subtree.
    fn deep_clone(&self) -> PaneNode {
        match self {
            PaneNode::Leaf(id) => PaneNode::Leaf(*id),
            PaneNode::Split {
                direction,
                ratio,
                first,
                second,
            } => PaneNode::Split {
                direction: *direction,
                ratio: *ratio,
                first: Box::new(first.deep_clone()),
                second: Box::new(second.deep_clone()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_single_pane() {
        let mut tree = PaneTree::new();
        let rect = PaneRect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        tree.layout(rect);
        assert_eq!(tree.pane_count(), 1);

        let r = tree.get_rect(PaneId(0)).unwrap();
        assert_eq!(r.width, 800.0);
        assert_eq!(r.height, 600.0);
    }

    #[test]
    fn test_vertical_split() {
        let mut tree = PaneTree::new();
        let new_id = tree.split(Direction::Vertical);

        let rect = PaneRect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        tree.layout(rect);

        assert_eq!(tree.pane_count(), 2);

        let r0 = tree.get_rect(PaneId(0)).unwrap();
        let r1 = tree.get_rect(new_id).unwrap();
        assert!((r0.width - 400.0).abs() < 1.0);
        assert!((r1.width - 400.0).abs() < 1.0);
    }

    #[test]
    fn test_horizontal_split() {
        let mut tree = PaneTree::new();
        let new_id = tree.split(Direction::Horizontal);

        let rect = PaneRect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        tree.layout(rect);

        assert_eq!(tree.pane_count(), 2);

        let r0 = tree.get_rect(PaneId(0)).unwrap();
        let r1 = tree.get_rect(new_id).unwrap();
        assert!((r0.height - 300.0).abs() < 1.0);
        assert!((r1.height - 300.0).abs() < 1.0);
    }

    #[test]
    fn test_grid_size() {
        let rect = PaneRect {
            x: 0.0,
            y: 0.0,
            width: 800.0,
            height: 600.0,
        };
        let (cols, rows) = rect.grid_size(8.0, 16.0);
        assert_eq!(cols, 100);
        assert_eq!(rows, 37);
    }
}
