use super::chat::ChatEngine;
use super::memory::LongTermMemory;
use super::provider::AiProvider;
use crate::features::consequence::ConsequenceAnalyzer;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Known shell commands (used for natural language detection).
/// Covers common commands across Windows, macOS, and Linux.
const KNOWN_COMMANDS: &[&str] = &[
    "ls", "cd", "pwd", "mkdir", "rmdir", "rm", "cp", "mv", "cat", "less", "more", "head",
    "tail", "grep", "find", "sort", "uniq", "wc", "echo", "printf", "touch", "chmod",
    "chown", "ps", "top", "htop", "kill", "ping", "curl", "wget", "tar", "gzip", "gunzip",
    "zip", "unzip", "ssh", "scp", "rsync", "git", "docker", "docker-compose", "kubectl",
    "npm", "yarn", "pnpm", "npx", "node", "python", "python3", "pip", "pip3", "cargo",
    "rustc", "rustup", "go", "deno", "bun", "make", "cmake", "gcc", "g++", "clang",
    "apt", "apt-get", "yum", "dnf", "pacman", "brew", "choco", "winget", "scoop",
    "systemctl", "journalctl", "service", "reboot", "shutdown", "poweroff", "halt",
    "mount", "umount", "df", "du", "free", "uname", "whoami", "id", "who", "w",
    "env", "export", "source", "alias", "type", "which", "where", "man", "help",
    "clear", "history", "exit", "logout",
    "cmd", "powershell", "pwsh",
    "code", "vim", "vi", "nano", "emacs", "nvim",
    "cargo", "rustc", "ripgrep", "websearch", "webread", "web-search", "web-read",
];

/// A command proposed by the AI agent.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposedCommand {
    /// The shell command to execute.
    pub command: String,
    /// Human-readable description of what this does.
    pub description: String,
}

/// The result of processing a natural language request.
#[derive(Debug, Clone)]
pub struct NaturalLanguageResult {
    /// AI's explanation/plan.
    pub explanation: String,
    /// Proposed shell commands.
    pub proposed_commands: Vec<ProposedCommand>,
}

/// System prompt for the natural language agent.
const AGENT_SYSTEM_PROMPT: &str = r#"You are Stratum AI Agent — an autonomous terminal assistant built into the Stratum terminal emulator.

You interpret natural language requests from the user and translate them into shell commands.

## Your capabilities:
- Understand what the user wants to do.
- Propose shell commands to achieve the goal (e.g., git, docker, AWS, file operations).
- Navigate and change directories: Proposing `cd <path>` commands will actually change the active directory of the terminal session, allowing you to move around the filesystem.
- Search globally and system-wide: Always prefer using ripgrep (`rg`) for finding files or searching text contents. To find file names, use `rg --files | rg <pattern>` or `fd`. Avoid slow native search utilities like Windows `dir /s` or Linux `find /`. You MUST use `rg` for high performance.
- Automatic ripgrep installation: If `rg` is not installed on the user's system (e.g., if a previous search fails or you want to ensure it is available), propose installing it via `npm install -g ripgrep-bin` or `cargo install ripgrep`.
- Web Search and Reading: If you need to search the web, fetch documentation, resolve doubts, or get installation steps, use:
  * `websearch "<query>"`: Search the web via DuckDuckGo and get top results.
  * `webread "<url>"`: Read the text content of a URL (strips JS, CSS, and HTML).
  The information retrieved from these web tools will be logged in the terminal and automatically processed and saved to memory for later sessions.
- Dynamic Skills & Document Generation: If the user asks you to write reports, generate PDFs, create Word documents, or render diagrams:
  * Check if required tools/packages are present, or propose installing them (e.g. `npm install docx` for MS Word docs, `npm install pdfkit` for PDFs, or `npm install -g @mermaid-js/mermaid-cli` for diagrams).
  * Write a self-contained Node.js script (e.g., `make_doc.js`) that uses the installed npm package to generate the document with beautiful typography, margins, colors, and layout (no placeholders/stubs).
  * Execute it with `node make_doc.js`.
  * For Mermaid diagrams, write a `.mmd` definition file and run `mmdc -i diagram.mmd -o diagram.png` to render it.
- Markdown & Reports: You can write reports, manuals, or research papers in clean Markdown, text, or generated DOCX/PDF formats as requested.
- Detect dangerous or destructive commands and warn the user.
- Guide the user step-by-step. For instance, when asked to "push to github", first propose checking SSH/git authentication (e.g. `ssh -T git@github.com` and `git config --get user.name`). If not configured, you can propose the config commands or guide the user.

## When responding, you MUST:
1. First, provide a brief explanation of what you'll do.
2. Then, list the exact shell commands needed, one per line.
3. Each command should have a brief description of what it does.
4. If a command is dangerous, prefix it with [DANGEROUS].
5. If you need information from the user (like a repo name or config details), set the "commands" list to empty and ask the user in the "explanation" field.
6. Keep explanations concise — users are in a terminal.

## Response format:
Your response must be valid JSON with this structure:
{
  "explanation": "Brief explanation of the plan or a question/prompt for the user",
  "commands": [
    {"command": "ls -la", "description": "List files in current directory"},
    {"command": "git status", "description": "Check git repository status"}
  ]
}

If the request is something you cannot do (requires GUI, external accounts, etc.), explain why.
If the request is unsafe, explain the risk and refuse.
"#;

/// Check if a given input string looks like natural language vs. a shell command.
pub fn is_natural_language(input: &str) -> bool {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return false;
    }

    // Slash commands are handled by the existing system
    if trimmed.starts_with('/') {
        return false;
    }

    let first_word = match trimmed.split_whitespace().next() {
        Some(w) => w,
        None => return false,
    };

    // Check if first word is a known command
    let first_word_lower = first_word.to_lowercase();

    // Remove path prefix (e.g., ./script.sh, /usr/bin/ls)
    let base_command = first_word_lower
        .rsplit('/')
        .next()
        .unwrap_or(&first_word_lower);

    // Remove common file extensions
    let base_command = base_command
        .trim_end_matches(".exe")
        .trim_end_matches(".bat")
        .trim_end_matches(".ps1")
        .trim_end_matches(".sh");

    if KNOWN_COMMANDS.contains(&base_command) {
        return false;
    }

    // Check if it's a relative/absolute path to an executable
    if base_command.contains('.') || base_command.contains('\\') {
        return false;
    }

    // Check if it looks like a flag (starts with -)
    if base_command.starts_with('-') {
        return false;
    }

    // Check if it looks like a variable assignment
    if base_command.contains('=') && !trimmed.contains(' ') {
        return false;
    }

    // Common English words that strongly indicate natural language
    let english_starters = [
        "i", "we", "you", "they", "he", "she", "it", "my", "your", "our", "their",
        "hi", "hello", "hey", "yo",
        "a", "an", "the", "this", "that", "these", "those",
        "tell", "show", "give", "find", "list", "print",
        "push", "pull", "run", "build", "deploy", "install", "update", "upgrade",
        "remove", "delete", "create", "make", "copy", "move", "rename", "edit",
        "open", "close", "start", "stop", "restart", "check", "test", "compile",
        "execute", "launch", "setup", "configure", "download", "upload", "sync",
        "backup", "restore", "clean", "fix", "help",
        "what", "how", "why", "when", "where", "who", "which", "whom",
        "can", "could", "would", "should", "please", "need", "want",
        "is", "are", "do", "does", "did", "was", "were", "has", "have", "had",
        "will", "shall", "may", "might", "must",
    ];

    // --- Strong signals for natural language ---

    // Check for English pronoun/verb starters (case-insensitive for "I")
    let first_word_lower = first_word.to_lowercase();
    if english_starters.contains(&first_word_lower.as_str()) {
        // Single word that's an English word: check if it's a greeting or question
        let word_count = trimmed.split_whitespace().count();
        if word_count >= 2 || first_word_lower == "hi" || first_word_lower == "hello"
            || first_word_lower == "hey" || first_word_lower == "help"
        {
            return true;
        }
    }

    // If first word starts with uppercase (proper English sentence), treat as natural
    if first_word.chars().next().map_or(false, |c| c.is_uppercase()) {
        let word_count = trimmed.split_whitespace().count();
        if word_count >= 3 {
            return true;
        }
    }

    // Check for common English articles/prepositions in multi-word input
    let english_signals = ["the", "is", "are", "was", "were", "to", "for", "of", "in",
        "on", "at", "with", "by", "from", "and", "or", "but", "not", "this", "that",
        "please", "could", "would", "should"];
    let words: Vec<&str> = trimmed.split_whitespace().collect();
    if words.len() >= 3 {
        let has_signal = words.iter().any(|w| english_signals.contains(&w.to_lowercase().as_str()));
        if has_signal {
            return true;
        }
    }

    // For 2-word inputs: check if first is an English word and second isn't a command
    if words.len() == 2 {
        let second_lower = words[1].to_lowercase();
        let second_base = second_lower.rsplit('/').next().unwrap_or(&second_lower);
        if !KNOWN_COMMANDS.contains(&second_base) && !second_base.contains('.')
            && english_starters.contains(&first_word_lower.as_str())
        {
            return true;
        }
    }

    // If the input has shell special characters, it's likely a command
    let shell_chars = ['|', '>', '<', '&', ';', '$', '`', '~', '*', '?', '[', ']', '{', '}', '(', ')'];
    if trimmed.chars().any(|c| shell_chars.contains(&c)) {
        return false;
    }

    // If we have a single word that isn't a known command, might be a program name
    // Let the shell handle it
    false
}

/// The Natural Language Agent that processes user requests.
pub struct NaturalLanguageAgent {
    pub chat_engine: ChatEngine,
    consequence_analyzer: ConsequenceAnalyzer,
    /// Whether the agent is currently processing a request.
    pub is_processing: bool,
    /// Long-term persistent memory across sessions.
    pub memory: LongTermMemory,
}

impl NaturalLanguageAgent {
    pub fn new() -> Self {
        let memory = LongTermMemory::load();
        Self {
            chat_engine: ChatEngine::with_system_prompt(AGENT_SYSTEM_PROMPT),
            consequence_analyzer: ConsequenceAnalyzer::new(),
            is_processing: false,
            memory,
        }
    }

    /// Process a natural language request and return proposed commands.
    pub async fn process(
        &mut self,
        request: &str,
        cwd: &str,
        provider: &AiProvider,
        is_follow_up: bool,
    ) -> Result<NaturalLanguageResult> {
        self.is_processing = true;

        let user_prompt = if is_follow_up {
            request.to_string()
        } else {
            // Inject long-term memory context into the first message
            let mut memory_context = String::new();
            if !self.memory.learnings.is_empty() {
                memory_context.push_str("\n### Long-Term Context (From Previous Sessions):\n");
                for learn in &self.memory.learnings {
                    memory_context.push_str(&format!("- {}\n", learn));
                }
            }

            if !self.memory.interactions.is_empty() {
                memory_context.push_str("\n### Recent History:\n");
                let start_idx = self.memory.interactions.len().saturating_sub(5);
                for record in &self.memory.interactions[start_idx..] {
                    memory_context.push_str(&format!(
                        "- Directory: {}\n  Request: {}\n  Executed: {:?}\n",
                        record.directory, record.user_request, record.executed_commands
                    ));
                }
            }

            format!(
                "Current directory: {}\nOperating system: {}\n{}\n\nNote: You can actively change directory using `cd <path>` and search globally/system-wide across all directories using ripgrep (`rg`) or other tools.\n\nUser request: {}\n\nRespond with the JSON format specified in the system prompt.",
                cwd,
                std::env::consts::OS,
                memory_context,
                request
            )
        };

        // Use send instead of query to accumulate conversation history for multi-step tasks
        let response = self.chat_engine.send(provider, &user_prompt).await?;
        let content = response.content.trim().to_string();

        // Try to parse JSON response
        let result = Self::parse_json_response(&content).unwrap_or_else(|| {
            // Fallback: treat the whole response as explanation
            NaturalLanguageResult {
                explanation: content,
                proposed_commands: Vec::new(),
            }
        });

        self.is_processing = false;
        Ok(result)
    }

    /// Check if a proposed command is dangerous.
    pub fn check_dangerous(&self, command: &str) -> bool {
        let score = self.consequence_analyzer.analyze(command);
        score.requires_deliberate_execution()
    }

    /// Get danger reason for a command.
    pub fn danger_reason(&self, command: &str) -> String {
        let score = self.consequence_analyzer.analyze(command);
        let risk = score.risk_level();
        format!(
            "⚠ {} risk — Reversibility: {:.0}%, Blast radius: {:.0}%",
            risk,
            score.reversibility * 100.0,
            score.blast_radius * 100.0
        )
    }

    /// Parse the AI response as JSON.
    fn parse_json_response(content: &str) -> Option<NaturalLanguageResult> {
        // Try to extract JSON from the response (might be wrapped in ```json ... ```)
        let json_str = if let Some(start) = content.find("```json") {
            let after_start = &content[start + 7..];
            if let Some(end) = after_start.find("```") {
                after_start[..end].trim()
            } else {
                content
            }
        } else if let Some(start) = content.find('{') {
            if let Some(end) = content.rfind('}') {
                &content[start..=end]
            } else {
                return None;
            }
        } else {
            return None;
        };

        // Parse the JSON
        #[derive(Deserialize)]
        struct AgentResponse {
            #[serde(default)]
            explanation: String,
            #[serde(default)]
            commands: Vec<ProposedCommand>,
        }

        match serde_json::from_str::<AgentResponse>(json_str) {
            Ok(parsed) => {
                if parsed.explanation.is_empty() && parsed.commands.is_empty() {
                    None
                } else {
                    Some(NaturalLanguageResult {
                        explanation: parsed.explanation,
                        proposed_commands: parsed.commands,
                    })
                }
            }
            Err(_) => None,
        }
    }
}

impl Default for NaturalLanguageAgent {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_natural_language() {
        assert!(is_natural_language("Push this project to repo"));
        assert!(is_natural_language("Run the tests"));
        assert!(is_natural_language("How do I check disk space?"));
        assert!(is_natural_language("Install dependencies for this project"));
        assert!(is_natural_language("I want to know about this project"));
        assert!(is_natural_language("hi"));
        assert!(is_natural_language("hello"));
        assert!(is_natural_language("Tell me about this directory"));
        assert!(is_natural_language("Show all files here"));
        assert!(is_natural_language("What is my current path"));
        assert!(is_natural_language("Can you run this project"));
        assert!(is_natural_language("Please install the dependencies"));
        assert!(!is_natural_language("ls -la"));
        assert!(!is_natural_language("git status"));
        assert!(!is_natural_language("/ask what is this"));
        assert!(!is_natural_language("cd src"));
        assert!(!is_natural_language("npm install"));
        assert!(!is_natural_language("rm -rf /"));
        assert!(!is_natural_language("cat file.txt"));
    }

    #[test]
    fn test_parse_json_response() {
        let json = r#"{
            "explanation": "I'll check the repository status first.",
            "commands": [
                {"command": "git status", "description": "Check current git status"}
            ]
        }"#;
        let result = NaturalLanguageAgent::parse_json_response(json);
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.explanation.contains("repository"));
        assert_eq!(result.proposed_commands.len(), 1);
        assert_eq!(result.proposed_commands[0].command, "git status");
    }

    #[test]
    fn test_parse_json_code_block() {
        let content = "Here's what I'll do:\n\n```json\n{\"explanation\":\"Check git status\",\"commands\":[{\"command\":\"git status\",\"description\":\"Check status\"}]}\n```\n";
        let result = NaturalLanguageAgent::parse_json_response(content);
        assert!(result.is_some());
        assert_eq!(result.unwrap().proposed_commands.len(), 1);
    }

    #[test]
    fn test_dangerous_detection() {
        let agent = NaturalLanguageAgent::new();
        assert!(agent.check_dangerous("rm -rf /"));
        assert!(agent.check_dangerous("dd if=/dev/zero of=/dev/sda"));
        assert!(!agent.check_dangerous("ls -la"));
        assert!(!agent.check_dangerous("git status"));
    }

    #[test]
    fn test_known_commands_not_natural() {
        for cmd in &["ls", "git", "cargo", "docker", "npm", "python", "ssh", "ping"] {
            assert!(!is_natural_language(cmd), "{} should not be natural language", cmd);
        }
    }

    #[test]
    fn test_english_phrases() {
        assert!(is_natural_language("Show me all files"));
        assert!(is_natural_language("Tell me about this directory"));
        assert!(is_natural_language("What is my current path?"));
        assert!(is_natural_language("Please push my changes"));
        assert!(is_natural_language("I want to know about this project"));
        assert!(is_natural_language("hi"));
        assert!(is_natural_language("hello there"));
        assert!(!is_natural_language("npm start"));
        assert!(!is_natural_language("python main.py"));
    }
}
