<div align="center">

# ⚡ Stratum Terminal

**The terminal that understands what you're doing — GPU-rendered, AI-aware, and agent-ready.**

[![CI](https://github.com/nexarats/NOS/actions/workflows/ci.yml/badge.svg)](https://github.com/nexarats/NOS/actions)
[![License: Apache 2.0](https://img.shields.io/badge/License-Apache%202.0-blue.svg)](LICENSE)

*Part of the [NOS (Nexarats Operating System)](https://github.com/nexarats/NOS) project.*

</div>

---

Stratum is a **GPU-rendered, AI-aware, and agent-compatible terminal emulator** built from scratch in Rust. It does not just display text — it understands your commands, previews their side effects, hosts autonomous agents, and presents output as structured, interactive data.

---

## ⚖️ Why Stratum? (How it differs from other terminals)

Traditional terminal emulators (like Alacritty, WezTerm, iTerm2, or Windows Terminal) are passive text display pipelines. They read bytes from a PTY and render them as character grids. Stratum rethinks this architecture entirely:

| Feature | Traditional Terminals | ⚡ Stratum Terminal |
| :--- | :--- | :--- |
| **Agent Integration** | None. Agents must scrape terminal buffers or run blind. | **Stratum Agent Protocol (SAP)** hosts agents directly with interactive IPC control. |
| **Safety Guardrails** | Runs any command blindly, even destructive ones. | **Execution Consequence Scoring** analyzes risk and intercepts hazardous commands. |
| **Data Rendering** | Unstructured flat text streams. | **Dimensional Output Panes** render escape-sequence tables, JSON, and keys. |
| **Filesystem Awareness** | No awareness of side-effects until after they run. | **Live Mutation Preview** lists changes before executing commands. |
| **Footprint & Speed** | Heavy web tech (Electron/WebTech) or barebones text grids. | Lightweight GPU-accelerated pipeline (~7 MB release binary, 120 FPS). |

---

## 🚀 Key Features

### 🤖 1. Stratum Agent Protocol (SAP)
Stratum is designed to run in environments with autonomous AI agents. Through SAP, agents connect to Stratum over a dedicated stdio channel and communicate using a structured JSON protocol. Agents can:
*   **Observe State:** Query grid sizes, focus changes, cursor positions, and scrollback.
*   **Write Inputs:** Synthesize keystrokes or input text directly into active shells.
*   **Control Layouts:** Open new split panes, focus specific tabs, or change theme color schemes dynamically.
*   **Receive Notifications:** Subscribe to real-time events, such as terminal size changes, shell processes spawning/exiting, or warning triggers.

### 🎯 2. Execution Consequence Scoring
Stratum reads commands as you type them and calculates an inline risk score across three dimensions:
*   **Reversibility:** Can this command be easily undone (e.g., deleting vs. renaming files)?
*   **Blast Radius:** How many files or system directories are affected?
*   **Novelty:** Is this a command or utility you rarely or never run?

Commands exceeding safety thresholds trigger a prominent overlay requesting explicit confirm/cancel keypresses before execution.

```
$ rm -rf /var/data
⚠️ CRITICAL: Irreversible operation affecting system paths
  Reversibility: 5%  |  Blast radius: 95%
  [Press Enter to confirm, Escape to cancel]
```

### 🔗 3. Deep NOS Shell Integration
When running inside Stratum, the **NOS Shell** detects the environment and uses custom OSC escape sequences to communicate structured data types (tables, key-value mappings, and arrays). Stratum renders these:
*   **Interactive GPU Tables:** Column headers, zebra row shading, and row counts.
*   **Mouse Interaction:** Click on columns to sort them, scroll through list arrays, or expand/collapse JSON trees.
*   **Seamless Fallback:** Standard CLI tools (`git`, `cargo`, `docker`) pass through normally as raw ANSI text.

### 🔮 4. Live Mutation Preview
As you type command arguments, Stratum performs dry-runs or parses argument specs to display a live mutation preview above the input prompt, showing you which files are slated for creation, deletion, or renaming.

```
$ mv src/old_impl.rs src/new_impl.rs
  MOVE: src/old_impl.rs → src/new_impl.rs
  Summary: 1 file moved
```

### 📝 5. Ambient Documentation & Completions
An integrated documentation sidebar parses flags and commands in real time. If you type a known command (e.g. `tar`, `find`, `cargo`), Stratum shows flag definitions, examples, and your shell history relevant to those tools directly inside the UI.

### ⚡ 6. GPU-Accelerated Text Rendering
Built on top of `wgpu`, Stratum renders the character grid and overlays with custom shaders. It uses `fontdue` to dynamically rasterize fonts (such as JetBrains Mono) into a GPU-bound glyph texture atlas. This enables:
*   Ultra-low input latency.
*   Sub-pixel anti-aliasing and transparent alpha blending.
*   Paced 120 FPS rendering.
*   An incredibly small release memory footprint (~7 MB binary).

---

## 🪟 Layout and Pane Management

Stratum organizes terminal sessions into tabs and binary-tree split panes.

| Shortcut | Action |
| :--- | :--- |
| `Ctrl+Shift+T` | Create new tab |
| `Ctrl+Shift+W` | Close active pane/tab |
| `Ctrl+Shift+E` | Split active pane vertically |
| `Ctrl+Shift+O` | Split active pane horizontally |
| `Ctrl+Tab` | Cycle to next tab |
| `Ctrl+Shift+Tab` | Cycle to previous tab |
| `Ctrl+Shift+]` | Focus next pane |
| `Ctrl+Shift+[` | Focus previous pane |

---

## ⚙️ Usage & Commands

```bash
# Start Stratum with the default NOS Shell
stratum

# Start Stratum in agent-control mode
stratum --agent-mode

# Force Stratum to load a standard system shell (e.g. bash or PowerShell)
stratum --shell system

# Launch with custom font sizes or configurations
stratum --font-size 16 --config ~/.config/stratum/config.toml
```

### Configuration File
Configure themes, fonts, and grid scrollback by creating a config file at `~/.config/stratum/config.toml`:

```toml
[font]
family = "JetBrains Mono"
size = 14.0

[terminal]
scrollback_lines = 10000
shell = "nos"           # "nos" (default) or "system"

[renderer]
gpu_backend = "auto"    # Vulkan, Metal, DX12, or auto

[theme]
background = "#1a1b26"
foreground = "#c0caf5"
```

---

## 🏗️ Architecture

```
stratum/src/
├── main.rs              CLI entry point & argument processing
├── app.rs               Winit event loop, GPU & input loop integration, SAP poller
├── errors.rs            Error hierarchy
├── terminal/
│   ├── pane.rs          PTY communications, ANSI code parsing, character grid state
│   └── pty.rs           Subprocess management, PTY creation, environment injector
├── parser/
│   └── ansi.rs          ANSI escape sequence FSM & structured OSC packet extractor
├── screen/
│   └── grid.rs          Character grid buffers & scrollback manager
├── renderer/
│   ├── gpu.rs           wgpu pipeline, render passes, grid/cursor shaders
│   ├── glyph_atlas.rs   Glyph rasterizer, texture allocation, glyph caching
│   ├── terminal.wgsl    GPU shaders (text, background, cursor overlay)
│   └── overlay.rs       Status bar UI, toasts, confirmation popups, structured tables
├── layout/
│   ├── panes.rs         Binary-tree pane splits and focus controller
│   └── tabs.rs          Tab list manager
├── features/
│   ├── consequence.rs   Execution risk evaluator
│   ├── dimensional_panes.rs  Escape-sequence structured table interpreter
│   ├── mutation_preview.rs   Filesystem mutation analyzer
│   └── inline_docs.rs   Contextual flag documentation lookup
├── input/
│   ├── keyboard.rs      Global shortcut parser
│   └── tracker.rs       Mouse clicks, scroll events, dragging, text selections
├── agent/
│   ├── mod.rs           Module root
│   ├── protocol.rs      Stratum Agent Protocol (SAP) JSON definitions
│   └── server.rs        Dedicated thread stdio reader/writer
└── config/
    └── mod.rs           TOML configuration loading
```

---

## 🛠️ Requirements & Building

### Prerequisites
*   **Rust 1.80+** ([rustup.rs](https://rustup.rs/))
*   **GPU** supporting Vulkan, Metal, or DirectX 12.
*   **Platform Dependencies:**
    *   **Windows:** None (uses built-in DX12/Vulkan).
    *   **Linux:** `libxkbcommon-dev`, `libwayland-dev`, `libx11-dev` are required for windowing.

### Build and Test
```bash
# Clone the repository
git clone https://github.com/Nexarats/Stratum.git
cd Stratum

# Run unit and integration tests
cargo test

# Build release binary
cargo build --release
```

---

## 📄 License & Attribution

Stratum is open-source software licensed under the **[Apache License 2.0](LICENSE)**. 

### Attribution Requirements
You are free to download, use, modify, build upon, and distribute this software for personal, commercial, or public projects. However, under the Apache License 2.0 terms, you must retain all copyright, patent, trademark, and attribution notices from the Source form of the Work, and include a copy of the license.

Please give credit to **Nexarats** if you use Stratum's codebase or design patterns in your own projects!

