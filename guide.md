# ⚡ Stratum Terminal — Complete AI & Usage Guide

Welcome to the **Stratum Terminal** guide! Stratum is a GPU-rendered, AI-aware, and agent-ready terminal emulator designed to run as a high-performance standalone graphical window. 

This guide covers compiling the application, setting up AI providers, using the integrated AI commands, and taking advantage of its advanced suggestion and safety features.

---

## 📦 1. Installation & Standalone Window Setup

Stratum is compiled as a native graphical application targeting the Windows GUI subsystem. It does not run inside Windows Terminal; it opens in its own custom GPU-accelerated window, exactly like Git Bash or Alacritty.

### Prerequisites
*   **Rust 1.80+** ([rustup.rs](https://rustup.rs/))
*   **GPU** supporting Vulkan, DirectX 12, or Metal.

### Compiling and Installing
Open PowerShell in the `stratum` root directory and run the installer:
```powershell
powershell -ExecutionPolicy Bypass -File .\scripts\install.ps1
```

#### What the installer does:
1.  **Release Build:** Compiles the optimized `stratum.exe` binary.
2.  **Icon Embedding:** Compiles and embeds the custom multi-resolution `stratum.ico` app icon into the executable.
3.  **Local Installation:** Copies the binary to `~/.stratum/bin/stratum.exe`.
4.  **PATH Configuration:** Adds the directory to your user `PATH` environment variable.
5.  **Start Menu Integration:** Creates a shortcut in your Start Menu:  
    `C:\Users\<YourUser>\AppData\Roaming\Microsoft\Windows\Start Menu\Programs\Stratum Terminal.lnk`

Once installed, you can launch it by typing **"Stratum Terminal"** in the Windows Start Menu search bar. By default, it will automatically detect and run a **`bash`** shell (if `bash.exe` is present in your PATH), falling back to `powershell.exe` otherwise.

---

## 🤖 2. Setting Up AI Providers

Stratum includes a unified AI provider interface supporting **29 LLM backends**. You can configure your API keys either via environment variables or using inline terminal commands.

### Configuring via Environment Variables
Set the appropriate API key environment variable before starting the terminal. For example:
*   **Google Gemini:** `GEMINI_API_KEY`
*   **Anthropic Claude:** `ANTHROPIC_API_KEY`
*   **DeepSeek:** `DEEPSEEK_API_KEY`
*   **OpenAI:** `OPENAI_API_KEY`
*   **OpenRouter:** `OPENROUTER_API_KEY`

### Configuring via Inline Commands
Inside the Stratum Terminal window, you can manage your keys interactively using special `/` prefix commands:

*   **List Providers:** Type `/ai-providers` to view all 29 supported backends and their status:
    ```
    /ai-providers
    ```
*   **Set an API Key:** Save a provider key to the local credential store:
    ```
    /ai-set-key gemini AIzaSyYourApiKeyHere
    /ai-set-key anthropic sk-ant-yourApiKeyHere
    ```
    *(API keys are saved securely in `~/.stratum/config/credentials`)*
*   **Check Active Config:** View the currently active provider, model, and count of configured backends:
    ```
    /ai-config
    # OR:
    /ai
    ```
*   **List Provider Models:** View all models supported by a specific provider (the active one is marked with an arrow `→`):
    ```
    /ai-models gemini
    ```
*   **Test Connection:** Perform a test ping to ensure your selected provider is responding correctly:
    ```
    /ai-test
    # Output: ✓ Google Gemini (gemini-2.0-flash) responded: Stratum AI connection successful.
    ```

---

## 🔮 3. Using AI Commands & Chat

Stratum intercepts input commands starting with a `/` character and routes them directly to the active AI engine rather than forwarding them to your PTY shell.

### Inline AI Commands

| Command | Description | Example |
| :--- | :--- | :--- |
| **`/ask <question>`** | Start or continue a natural language conversation with the AI. | `/ask How do I extract a .tar.gz file?` |
| **`/explain [context]`** | Asks the AI to explain the provided text or the last output error. | `/explain Permission denied (os error 13)` |
| **`/suggest`** | Examines your current working directory and suggests 3-5 useful commands. | `/suggest` |
| **`/translate <command>`** | Translates a shell command between Windows (CMD/PowerShell), macOS, and Linux. | `/translate ls -la | awk '{print $9}'` |
| **`/clear-chat`** | Resets the conversation history for your current active terminal session. | `/clear-chat` |

AI responses are displayed inline as a formatted overlay blocks containing the model name and total token usage:
```
To extract a tar.gz file, use:
tar -xzf archive.tar.gz

─── gemini-2.0-flash │ 42 tokens ───
```

---

## 📊 4. The Suggestion Engine & Autocompletions

As you type, Stratum's ambient suggestion cards overlay context-aware completions.

### Navigating suggestions:
*   **Arrow Up / Down:** Cycle through suggestion candidates.
*   **Tab:** Accept the highlighted suggestion.
*   **Escape:** Close the suggestion card overlay.

### Completion Providers:
The Suggestion Engine polls 11 distinct completion contexts dynamically:
*   **Filesystem:** Auto-suggests folders and files relative to the current workspace.
*   **History:** Completes commands you have previously executed in this terminal.
*   **System Environment:** Autocompletes environmental variables when you type `$`.
*   **Package Managers:** Integrates completions for `npm`, `cargo`, and Python packages.
*   **Infrastructure Containers:** Autocompletes commands, images, and containers for `git` and `docker`.

---

## 🛡️ 5. Execution Consequence Scoring (Safety Guardrail)

Stratum helps prevent destructive accidents by running commands through a real-time risk evaluator before sending them to the PTY shell process.

### Risk Dimensions Analyzed:
*   **Reversibility:** Evaluating whether the action is destructive (e.g. `rm`, `del`, `format`, `dd`) or reversible.
*   **Blast Radius:** The scope of files, partitions, or system configurations affected.
*   **Novelty:** How frequently you run this command (unusual/unknown commands score a higher novelty risk).

### How to respond:
When a command exceeds safety limits, Stratum intercepts execution and displays a warning:
```
$ rm -rf /var/data
⚠️ CRITICAL: Irreversible operation affecting system paths
  Reversibility: 5%  |  Blast radius: 95%
  [Press Enter to confirm, Escape to cancel]
```
*   Press **Enter** to proceed and force-execute the command.
*   Press **Escape** to discard the command safely.

---

## 🪟 6. Layout & Window Management

Stratum supports native binary-tree tab and pane multiplexing:

| Shortcut | Action |
| :--- | :--- |
| **`Ctrl+Shift+T`** | Create new tab |
| **`Ctrl+Shift+W`** | Close active pane/tab |
| **`Ctrl+Shift+E`** | Split active pane vertically |
| **`Ctrl+Shift+O`** | Split active pane horizontally |
| **`Ctrl+Tab`** | Cycle to the next tab |
| **`Ctrl+Shift+Tab`** | Cycle to the previous tab |
| **`Ctrl+Shift+]`** | Switch focus to the next split pane |
| **`Ctrl+Shift+[`** | Switch focus to the previous split pane |

You can also run `/theme <theme-name>` (e.g., `/theme dracula`) to switch color palettes, and `/themes` to list all built-in themes.

---

## 🤖 7. Stratum Agent Protocol (SAP)

For automated pipelines or AI coding agents, Stratum hosts an IPC socket running JSON-RPC 2.0.

### Starting in Agent Mode:
Run Stratum with the `--agent-mode` flag:
```bash
stratum --agent-mode
```
In this mode, the terminal runs headlessly, allowing an external agent to connect via standard input/output (stdio) and issue commands:

*   **`initialize`:** Setup the initial agent capabilities.
*   **`terminal/execute`:** Safely run commands inside the PTY.
*   **`terminal/write`:** Write raw characters to standard input.
*   **`terminal/read`:** Read terminal grid rows, styles, and cursors.
*   **`terminal/setTheme`:** Change color schemes dynamically.
*   **`terminal/subscribe`:** Subscribe to live shell output streams, resizing events, or process exit updates.
