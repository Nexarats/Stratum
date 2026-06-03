//! Execution Consequence Scoring — scores commands before execution.
//!
//! Every command receives a consequence score on three dimensions:
//! - **Reversibility**: Can the effects be undone?
//! - **Blast Radius**: How many files, services, or processes are affected?
//! - **Novelty**: Has the user run this command before in this context?
//!
//! High-risk commands trigger a "deliberate execution" interlock.

use std::collections::HashSet;

/// Consequence score for a command.
#[derive(Debug, Clone)]
pub struct ConsequenceScore {
    /// 0.0 (fully reversible) to 1.0 (irreversible).
    pub reversibility: f32,
    /// 0.0 (no impact) to 1.0 (system-wide impact).
    pub blast_radius: f32,
    /// 0.0 (frequently used) to 1.0 (never seen before).
    pub novelty: f32,
}

impl ConsequenceScore {
    /// Returns true if the command should require deliberate confirmation.
    pub fn requires_deliberate_execution(&self) -> bool {
        // High irreversibility + high blast radius = dangerous
        (self.reversibility > 0.7 && self.blast_radius > 0.5)
            || (self.reversibility > 0.5 && self.blast_radius > 0.7)
            || (self.reversibility > 0.8)
    }

    /// Overall risk level as a human-readable string.
    pub fn risk_level(&self) -> &'static str {
        let score = (self.reversibility + self.blast_radius + self.novelty) / 3.0;
        if score > 0.7 {
            "HIGH"
        } else if score > 0.4 {
            "MEDIUM"
        } else {
            "LOW"
        }
    }
}

/// Analyzes commands and produces consequence scores.
pub struct ConsequenceAnalyzer {
    /// Commands the user has run before (for novelty scoring).
    history: HashSet<String>,
    /// Known destructive command patterns.
    destructive_patterns: Vec<DestructivePattern>,
}

/// A pattern that indicates destructive behavior.
struct DestructivePattern {
    /// Pattern to match against the command string.
    pattern: String,
    /// Base reversibility score if matched.
    reversibility: f32,
    /// Base blast radius score if matched.
    blast_radius: f32,
}

impl ConsequenceAnalyzer {
    /// Create a new analyzer with default destructive patterns.
    pub fn new() -> Self {
        let destructive_patterns = vec![
            DestructivePattern {
                pattern: "rm -rf /".into(),
                reversibility: 1.0,
                blast_radius: 1.0,
            },
            DestructivePattern {
                pattern: "rm -rf".into(),
                reversibility: 0.9,
                blast_radius: 0.7,
            },
            DestructivePattern {
                pattern: "rm ".into(),
                reversibility: 0.7,
                blast_radius: 0.3,
            },
            DestructivePattern {
                pattern: "mkfs".into(),
                reversibility: 1.0,
                blast_radius: 1.0,
            },
            DestructivePattern {
                pattern: "dd if=".into(),
                reversibility: 0.9,
                blast_radius: 0.8,
            },
            DestructivePattern {
                pattern: "chmod -R 777".into(),
                reversibility: 0.6,
                blast_radius: 0.8,
            },
            DestructivePattern {
                pattern: "chown -R".into(),
                reversibility: 0.5,
                blast_radius: 0.7,
            },
            DestructivePattern {
                pattern: "> /dev/".into(),
                reversibility: 1.0,
                blast_radius: 0.9,
            },
            DestructivePattern {
                pattern: "kill -9".into(),
                reversibility: 0.4,
                blast_radius: 0.3,
            },
            DestructivePattern {
                pattern: "systemctl stop".into(),
                reversibility: 0.2,
                blast_radius: 0.4,
            },
            DestructivePattern {
                pattern: "reboot".into(),
                reversibility: 0.3,
                blast_radius: 0.9,
            },
            DestructivePattern {
                pattern: "shutdown".into(),
                reversibility: 0.3,
                blast_radius: 1.0,
            },
            DestructivePattern {
                pattern: "curl | bash".into(),
                reversibility: 0.8,
                blast_radius: 0.9,
            },
            DestructivePattern {
                pattern: "curl | sh".into(),
                reversibility: 0.8,
                blast_radius: 0.9,
            },
        ];

        Self {
            history: HashSet::new(),
            destructive_patterns,
        }
    }

    /// Analyze a command and return its consequence score.
    pub fn analyze(&self, command: &str) -> ConsequenceScore {
        let cmd_lower = command.to_lowercase();

        // Find matching destructive patterns
        let mut max_reversibility = 0.0f32;
        let mut max_blast_radius = 0.0f32;

        for pattern in &self.destructive_patterns {
            if cmd_lower.contains(&pattern.pattern) {
                max_reversibility = max_reversibility.max(pattern.reversibility);
                max_blast_radius = max_blast_radius.max(pattern.blast_radius);
            }
        }

        // Novelty: 1.0 if never seen, 0.0 if seen before
        let novelty = if self.history.contains(&cmd_lower) {
            0.0
        } else {
            // Check if a similar command was used (same base command)
            let base_cmd = cmd_lower.split_whitespace().next().unwrap_or("");
            let has_similar = self.history.iter().any(|h| {
                h.split_whitespace().next().unwrap_or("") == base_cmd
            });
            if has_similar {
                0.3 // Same base command, different args
            } else {
                1.0 // Completely new command
            }
        };

        ConsequenceScore {
            reversibility: max_reversibility,
            blast_radius: max_blast_radius,
            novelty,
        }
    }

    /// Record a command in the history (for future novelty scoring).
    pub fn record_command(&mut self, command: &str) {
        self.history.insert(command.to_lowercase());
    }
}

impl Default for ConsequenceAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_safe_command() {
        let analyzer = ConsequenceAnalyzer::new();
        let score = analyzer.analyze("ls -la");
        assert!(score.reversibility < 0.1);
        assert!(score.blast_radius < 0.1);
        assert!(!score.requires_deliberate_execution());
    }

    #[test]
    fn test_destructive_command() {
        let analyzer = ConsequenceAnalyzer::new();
        let score = analyzer.analyze("rm -rf /home/user");
        assert!(score.reversibility > 0.7);
        assert!(score.blast_radius > 0.5);
        assert!(score.requires_deliberate_execution());
    }

    #[test]
    fn test_system_destruction() {
        let analyzer = ConsequenceAnalyzer::new();
        let score = analyzer.analyze("rm -rf /");
        assert_eq!(score.reversibility, 1.0);
        assert_eq!(score.blast_radius, 1.0);
        assert!(score.requires_deliberate_execution());
        assert_eq!(score.risk_level(), "HIGH");
    }

    #[test]
    fn test_novelty_tracking() {
        let mut analyzer = ConsequenceAnalyzer::new();
        let score1 = analyzer.analyze("git status");
        assert_eq!(score1.novelty, 1.0); // Never seen

        analyzer.record_command("git status");
        let score2 = analyzer.analyze("git status");
        assert_eq!(score2.novelty, 0.0); // Seen before

        let score3 = analyzer.analyze("git log");
        assert!(score3.novelty > 0.0); // Same base, different args
        assert!(score3.novelty < 1.0);
    }
}
