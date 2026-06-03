//! Dimensional Output Panes — structured output detection and rendering.
//!
//! Instead of treating all command output as flat text, Stratum detects
//! structured content (tables, file listings, JSON, key-value pairs,
//! process lists) and renders them as interactive, queryable panes.
//!
//! Each output pane is autonomous — tables can be sorted by column,
//! file listings can be navigated, and JSON can be collapsed/expanded.

use std::collections::HashMap;

/// The type of structured content detected in command output.
#[derive(Debug, Clone, PartialEq)]
pub enum OutputKind {
    /// Plain unstructured text (fallback).
    PlainText,
    /// Tabular data with headers and rows.
    Table(TableData),
    /// File/directory listing.
    FileListing(Vec<FileEntry>),
    /// JSON data.
    Json(String),
    /// Key-value pairs (e.g., from `env`, `sysctl`).
    KeyValue(Vec<(String, String)>),
    /// Process list (e.g., from `ps`, `top`).
    ProcessList(Vec<ProcessEntry>),
    /// Git status output.
    GitStatus(GitStatusData),
    /// Error output (stderr).
    ErrorOutput(String),
}

/// Tabular data with column headers and typed rows.
#[derive(Debug, Clone, PartialEq)]
pub struct TableData {
    pub headers: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub sort_column: Option<usize>,
    pub sort_ascending: bool,
}

impl TableData {
    /// Sort by the specified column.
    pub fn sort_by_column(&mut self, col: usize) {
        if col >= self.headers.len() {
            return;
        }

        if self.sort_column == Some(col) {
            self.sort_ascending = !self.sort_ascending;
        } else {
            self.sort_column = Some(col);
            self.sort_ascending = true;
        }

        let ascending = self.sort_ascending;
        self.rows.sort_by(|a, b| {
            let va = a.get(col).map(|s| s.as_str()).unwrap_or("");
            let vb = b.get(col).map(|s| s.as_str()).unwrap_or("");

            // Try numeric comparison first
            if let (Ok(na), Ok(nb)) = (va.parse::<f64>(), vb.parse::<f64>()) {
                let ord = na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal);
                return if ascending { ord } else { ord.reverse() };
            }

            // Fall back to string comparison
            let ord = va.cmp(vb);
            if ascending { ord } else { ord.reverse() }
        });
    }

    /// Filter rows containing the given search term.
    pub fn filter(&self, query: &str) -> TableData {
        let query_lower = query.to_lowercase();
        let filtered_rows: Vec<Vec<String>> = self
            .rows
            .iter()
            .filter(|row| {
                row.iter()
                    .any(|cell| cell.to_lowercase().contains(&query_lower))
            })
            .cloned()
            .collect();

        TableData {
            headers: self.headers.clone(),
            rows: filtered_rows,
            sort_column: self.sort_column,
            sort_ascending: self.sort_ascending,
        }
    }
}

/// A single file/directory entry.
#[derive(Debug, Clone, PartialEq)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: Option<u64>,
    pub permissions: Option<String>,
    pub modified: Option<String>,
    pub owner: Option<String>,
}

/// A single process entry.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcessEntry {
    pub pid: u32,
    pub name: String,
    pub cpu_percent: f32,
    pub mem_percent: f32,
    pub status: String,
    pub user: String,
    pub command: String,
}

/// Git status data.
#[derive(Debug, Clone, PartialEq)]
pub struct GitStatusData {
    pub branch: String,
    pub staged: Vec<GitFileChange>,
    pub unstaged: Vec<GitFileChange>,
    pub untracked: Vec<String>,
}

/// A git file change entry.
#[derive(Debug, Clone, PartialEq)]
pub struct GitFileChange {
    pub path: String,
    pub status: GitChangeStatus,
}

#[derive(Debug, Clone, PartialEq)]
pub enum GitChangeStatus {
    Added,
    Modified,
    Deleted,
    Renamed,
    Copied,
}

/// Detects structured content type from raw command output.
pub struct OutputDetector {
    /// Known command → output type mappings.
    command_hints: HashMap<String, OutputHint>,
}

#[derive(Debug, Clone)]
enum OutputHint {
    Table,
    FileListing,
    ProcessList,
    KeyValue,
    GitStatus,
}

impl OutputDetector {
    /// Create a new detector with built-in command knowledge.
    pub fn new() -> Self {
        let mut hints = HashMap::new();

        // File listing commands
        for cmd in &["ls", "ll", "la", "dir", "exa", "eza"] {
            hints.insert(cmd.to_string(), OutputHint::FileListing);
        }

        // Process listing commands
        for cmd in &["ps", "top", "htop", "procs"] {
            hints.insert(cmd.to_string(), OutputHint::ProcessList);
        }

        // Key-value commands
        for cmd in &["env", "printenv", "set", "sysctl"] {
            hints.insert(cmd.to_string(), OutputHint::KeyValue);
        }

        // Table-producing commands
        for cmd in &["df", "mount", "lsblk", "fdisk", "lsusb", "lspci", "ip", "ss", "netstat"] {
            hints.insert(cmd.to_string(), OutputHint::Table);
        }

        // Git
        hints.insert("git".to_string(), OutputHint::GitStatus);

        Self {
            command_hints: hints,
        }
    }

    /// Detect the output type from command name and raw output text.
    pub fn detect(&self, command: Option<&str>, output: &str) -> OutputKind {
        // Check command hint first
        if let Some(cmd) = command {
            let base_cmd = cmd
                .split_whitespace()
                .next()
                .unwrap_or("")
                .rsplit('/')
                .next()
                .unwrap_or("");

            if let Some(hint) = self.command_hints.get(base_cmd) {
                match hint {
                    OutputHint::FileListing => {
                        if let Some(listing) = self.parse_ls_output(output) {
                            return OutputKind::FileListing(listing);
                        }
                    }
                    OutputHint::ProcessList => {
                        if let Some(procs) = self.parse_ps_output(output) {
                            return OutputKind::ProcessList(procs);
                        }
                    }
                    OutputHint::KeyValue => {
                        if let Some(kvs) = self.parse_kv_output(output) {
                            return OutputKind::KeyValue(kvs);
                        }
                    }
                    OutputHint::Table => {
                        if let Some(table) = self.parse_table_output(output) {
                            return OutputKind::Table(table);
                        }
                    }
                    OutputHint::GitStatus => {
                        if cmd.contains("status") {
                            if let Some(status) = self.parse_git_status(output) {
                                return OutputKind::GitStatus(status);
                            }
                        }
                    }
                }
            }
        }

        // Content-based detection (no command hint)
        if output.trim_start().starts_with('{') || output.trim_start().starts_with('[') {
            if serde_json::from_str::<serde_json::Value>(output).is_ok() {
                return OutputKind::Json(output.to_string());
            }
        }

        // Auto-detect tables (lines with consistent column alignment)
        if let Some(table) = self.parse_table_output(output) {
            if table.headers.len() >= 2 && table.rows.len() >= 2 {
                return OutputKind::Table(table);
            }
        }

        OutputKind::PlainText
    }

    /// Parse `ls -l` style output into FileEntry list.
    fn parse_ls_output(&self, output: &str) -> Option<Vec<FileEntry>> {
        let mut entries = Vec::new();

        for line in output.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with("total ") {
                continue;
            }

            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 9 {
                // Long format: drwxr-xr-x 2 user group 4096 Jan 1 12:00 name
                let permissions = parts[0].to_string();
                let is_dir = permissions.starts_with('d');
                let size = parts[4].parse::<u64>().ok();
                let owner = Some(parts[2].to_string());
                let modified = Some(format!("{} {} {}", parts[5], parts[6], parts[7]));
                let name = parts[8..].join(" ");

                entries.push(FileEntry {
                    name,
                    is_dir,
                    size,
                    permissions: Some(permissions),
                    modified,
                    owner,
                });
            } else if parts.len() >= 1 {
                // Simple format: just names
                entries.push(FileEntry {
                    name: parts[0].to_string(),
                    is_dir: false,
                    size: None,
                    permissions: None,
                    modified: None,
                    owner: None,
                });
            }
        }

        if entries.is_empty() {
            None
        } else {
            Some(entries)
        }
    }

    /// Parse `ps aux` style output into ProcessEntry list.
    fn parse_ps_output(&self, output: &str) -> Option<Vec<ProcessEntry>> {
        let lines: Vec<&str> = output.lines().collect();
        if lines.len() < 2 {
            return None;
        }

        let mut procs = Vec::new();
        for line in &lines[1..] {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() >= 11 {
                if let Ok(pid) = parts[1].parse::<u32>() {
                    let cpu = parts[2].parse::<f32>().unwrap_or(0.0);
                    let mem = parts[3].parse::<f32>().unwrap_or(0.0);
                    let command = parts[10..].join(" ");

                    procs.push(ProcessEntry {
                        pid,
                        name: parts[10].rsplit('/').next().unwrap_or(parts[10]).to_string(),
                        cpu_percent: cpu,
                        mem_percent: mem,
                        status: parts[7].to_string(),
                        user: parts[0].to_string(),
                        command,
                    });
                }
            }
        }

        if procs.is_empty() {
            None
        } else {
            Some(procs)
        }
    }

    /// Parse key=value output (env, printenv).
    fn parse_kv_output(&self, output: &str) -> Option<Vec<(String, String)>> {
        let mut pairs = Vec::new();

        for line in output.lines() {
            if let Some(eq_pos) = line.find('=') {
                let key = line[..eq_pos].to_string();
                let value = line[eq_pos + 1..].to_string();
                if !key.contains(' ') && !key.is_empty() {
                    pairs.push((key, value));
                }
            }
        }

        if pairs.len() >= 3 {
            Some(pairs)
        } else {
            None
        }
    }

    /// Parse aligned column output into a table.
    fn parse_table_output(&self, output: &str) -> Option<TableData> {
        let lines: Vec<&str> = output.lines().filter(|l| !l.trim().is_empty()).collect();
        if lines.len() < 2 {
            return None;
        }

        // Use first line as headers
        let headers: Vec<String> = lines[0]
            .split_whitespace()
            .map(|s| s.to_string())
            .collect();

        if headers.len() < 2 {
            return None;
        }

        let mut rows = Vec::new();
        for line in &lines[1..] {
            let cells: Vec<String> = line
                .split_whitespace()
                .map(|s| s.to_string())
                .collect();

            // Allow rows with fewer/more columns than headers
            if !cells.is_empty() {
                rows.push(cells);
            }
        }

        Some(TableData {
            headers,
            rows,
            sort_column: None,
            sort_ascending: true,
        })
    }

    /// Parse git status output.
    fn parse_git_status(&self, output: &str) -> Option<GitStatusData> {
        let mut branch = String::from("unknown");
        let mut staged = Vec::new();
        let mut unstaged = Vec::new();
        let mut untracked = Vec::new();

        for line in output.lines() {
            let trimmed = line.trim();

            if trimmed.starts_with("On branch ") {
                branch = trimmed["On branch ".len()..].to_string();
            } else if let Some(path) = trimmed.strip_prefix("new file:") {
                staged.push(GitFileChange {
                    path: path.trim().to_string(),
                    status: GitChangeStatus::Added,
                });
            } else if let Some(path) = trimmed.strip_prefix("modified:") {
                unstaged.push(GitFileChange {
                    path: path.trim().to_string(),
                    status: GitChangeStatus::Modified,
                });
            } else if let Some(path) = trimmed.strip_prefix("deleted:") {
                unstaged.push(GitFileChange {
                    path: path.trim().to_string(),
                    status: GitChangeStatus::Deleted,
                });
            } else if trimmed.starts_with("??") || (!trimmed.is_empty() && !trimmed.starts_with('#') && !trimmed.contains(':') && !trimmed.starts_with("Your") && !trimmed.starts_with("Changes") && !trimmed.starts_with("Untracked") && !trimmed.starts_with("(") && !trimmed.starts_with("no ") && !trimmed.starts_with("nothing")) {
                // Heuristic for untracked files
            }
        }

        if branch == "unknown" && staged.is_empty() && unstaged.is_empty() {
            return None;
        }

        Some(GitStatusData {
            branch,
            staged,
            unstaged,
            untracked,
        })
    }
}

impl Default for OutputDetector {
    fn default() -> Self {
        Self::new()
    }
}

/// A dimensional output pane — holds detected structured output.
pub struct DimensionalPane {
    /// The command that produced this output.
    pub command: String,
    /// The raw text output.
    pub raw_output: String,
    /// The detected structured form.
    pub kind: OutputKind,
    /// Whether the pane is collapsed.
    pub collapsed: bool,
    /// Search/filter query applied by the user.
    pub filter_query: Option<String>,
    /// Scroll offset within the pane.
    pub scroll_offset: usize,
    /// Whether the pane is selected/focused.
    pub focused: bool,
}

impl DimensionalPane {
    /// Create a new dimensional pane from command output.
    pub fn new(command: &str, raw_output: &str, detector: &OutputDetector) -> Self {
        let kind = detector.detect(Some(command), raw_output);

        Self {
            command: command.to_string(),
            raw_output: raw_output.to_string(),
            kind,
            collapsed: false,
            filter_query: None,
            scroll_offset: 0,
            focused: false,
        }
    }

    /// Toggle collapsed state.
    pub fn toggle_collapse(&mut self) {
        self.collapsed = !self.collapsed;
    }

    /// Apply a filter query.
    pub fn set_filter(&mut self, query: &str) {
        self.filter_query = if query.is_empty() {
            None
        } else {
            Some(query.to_string())
        };
    }

    /// Sort table output by column index.
    pub fn sort_by_column(&mut self, col: usize) {
        if let OutputKind::Table(ref mut table) = self.kind {
            table.sort_by_column(col);
        }
    }

    /// Get effective row count for display.
    pub fn visible_row_count(&self) -> usize {
        match &self.kind {
            OutputKind::PlainText => self.raw_output.lines().count(),
            OutputKind::Table(t) => {
                if let Some(ref q) = self.filter_query {
                    t.filter(q).rows.len() + 1
                } else {
                    t.rows.len() + 1
                }
            }
            OutputKind::FileListing(files) => files.len(),
            OutputKind::ProcessList(procs) => procs.len() + 1,
            OutputKind::KeyValue(kvs) => kvs.len(),
            OutputKind::Json(_) => self.raw_output.lines().count(),
            OutputKind::GitStatus(gs) => {
                gs.staged.len() + gs.unstaged.len() + gs.untracked.len() + 3
            }
            OutputKind::ErrorOutput(e) => e.lines().count(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plain_text_detection() {
        let detector = OutputDetector::new();
        let result = detector.detect(Some("echo hello"), "hello\n");
        assert_eq!(result, OutputKind::PlainText);
    }

    #[test]
    fn test_json_detection() {
        let detector = OutputDetector::new();
        let json = r#"{"name": "stratum", "version": "0.1.0"}"#;
        let result = detector.detect(None, json);
        match result {
            OutputKind::Json(_) => {}
            other => panic!("Expected Json, got {:?}", other),
        }
    }

    #[test]
    fn test_kv_detection() {
        let detector = OutputDetector::new();
        let output = "HOME=/home/user\nPATH=/usr/bin\nSHELL=/bin/bash\nTERM=xterm-256color\n";
        let result = detector.detect(Some("env"), output);
        match result {
            OutputKind::KeyValue(pairs) => {
                assert_eq!(pairs.len(), 4);
                assert_eq!(pairs[0].0, "HOME");
                assert_eq!(pairs[0].1, "/home/user");
            }
            other => panic!("Expected KeyValue, got {:?}", other),
        }
    }

    #[test]
    fn test_table_sort() {
        let mut table = TableData {
            headers: vec!["Name".into(), "Size".into()],
            rows: vec![
                vec!["banana".into(), "3".into()],
                vec!["apple".into(), "1".into()],
                vec!["cherry".into(), "2".into()],
            ],
            sort_column: None,
            sort_ascending: true,
        };

        // Sort by name ascending
        table.sort_by_column(0);
        assert_eq!(table.rows[0][0], "apple");
        assert_eq!(table.rows[2][0], "cherry");

        // Sort by name descending (click same column again)
        table.sort_by_column(0);
        assert_eq!(table.rows[0][0], "cherry");

        // Sort by size (numeric)
        table.sort_by_column(1);
        assert_eq!(table.rows[0][1], "1");
        assert_eq!(table.rows[2][1], "3");
    }

    #[test]
    fn test_table_filter() {
        let table = TableData {
            headers: vec!["Name".into(), "Type".into()],
            rows: vec![
                vec!["main.rs".into(), "file".into()],
                vec!["src".into(), "dir".into()],
                vec!["Cargo.toml".into(), "file".into()],
            ],
            sort_column: None,
            sort_ascending: true,
        };

        let filtered = table.filter("file");
        assert_eq!(filtered.rows.len(), 2);

        let filtered2 = table.filter("cargo");
        assert_eq!(filtered2.rows.len(), 1);
    }

    #[test]
    fn test_ps_detection() {
        let detector = OutputDetector::new();
        let output = "USER       PID %CPU %MEM    VSZ   RSS TTY      STAT START   TIME COMMAND\nroot         1  0.0  0.1 169324 13456 ?        Ss   Jan01   0:05 /sbin/init\nuser      1234  2.5  1.2 987654 98765 pts/0    S+   12:00   0:30 /usr/bin/vim\n";
        let result = detector.detect(Some("ps aux"), output);
        match result {
            OutputKind::ProcessList(procs) => {
                assert_eq!(procs.len(), 2);
                assert_eq!(procs[0].pid, 1);
                assert_eq!(procs[1].pid, 1234);
                assert!((procs[1].cpu_percent - 2.5).abs() < 0.01);
            }
            other => panic!("Expected ProcessList, got {:?}", other),
        }
    }

    #[test]
    fn test_dimensional_pane_creation() {
        let detector = OutputDetector::new();
        let pane = DimensionalPane::new("ls -la", "total 8\ndrwxr-xr-x 2 user user 4096 Jan  1 12:00 .\ndrwxr-xr-x 3 user user 4096 Jan  1 12:00 ..\n-rw-r--r-- 1 user user  100 Jan  1 12:00 file.txt\n", &detector);

        match &pane.kind {
            OutputKind::FileListing(files) => {
                assert_eq!(files.len(), 3);
                assert!(files[0].is_dir);
                assert!(!files[2].is_dir);
                assert_eq!(files[2].name, "file.txt");
            }
            other => panic!("Expected FileListing, got {:?}", other),
        }
    }
}
