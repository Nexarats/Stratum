//! Production terminal application — winit event loop + GPU renderer + innovation features.
//!
//! Architecture:
//!   winit key event → InputTracker → active pane PTY
//!   InputTracker → DocOverlay → OverlayManager (inline docs)
//!   Enter pressed → ConsequenceAnalyzer + MutationAnalyzer → warnings/preview
//!   PTY output → ANSI parser → screen grid → GPU renderer → display

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, KeyCode, ModifiersState, NamedKey, PhysicalKey};
use winit::window::{Window, WindowId};

use crate::ai::commands::{AiCommand, AiCommandExecutor};
use crate::ai::natural::{NaturalLanguageAgent, NaturalLanguageResult, ProposedCommand};
use crate::config::Settings;
use crate::features::consequence::ConsequenceAnalyzer;
use crate::features::dimensional_panes::OutputDetector;
use crate::features::inline_docs::DocOverlay;
use crate::features::mutation_preview::MutationAnalyzer;
use crate::input::InputTracker;
use crate::layout::panes::{Direction, PaneId, PaneRect, PaneTree};
use crate::layout::tabs::TabManager;
use crate::renderer::overlay::{
    AiAgentCommandItem, AiAgentContent, ConsequenceWarningContent, InlineDocContent,
    InlineDocFlag, MutationChangeLine, MutationPreviewContent, OverlayManager,
    OverlayPosition, StatusBarContent, ToastContent,
};
use crate::renderer::GpuRenderer;
use crate::screen::{Color, GridPos, ScreenGrid, Selection, SelectionMode};
use crate::suggestions::SuggestionEngine;
use crate::terminal::TerminalPane;
use crate::features::mutation_preview::FsChange;

/// Target frame rate.
const TARGET_FPS: u32 = 120;
const FRAME_DURATION: Duration = Duration::from_micros(1_000_000 / TARGET_FPS as u64);

/// The production terminal application.
pub struct TerminalApp {
    settings: Settings,
    shell: String,
    state: Option<AppState>,
    modifiers: ModifiersState,
    /// Whether to run in agent mode (SAP over stdin/stdout).
    agent_mode: bool,
}

/// Internal state initialized after window creation.
struct AppState {
    window: Arc<Window>,
    renderer: GpuRenderer,
    pane_tree: PaneTree,
    tab_manager: TabManager,
    panes: HashMap<PaneId, TerminalPane>,

    // --- Innovation feature engines ---
    input_tracker: InputTracker,
    consequence_analyzer: ConsequenceAnalyzer,
    mutation_analyzer: MutationAnalyzer,
    doc_overlay: DocOverlay,
    output_detector: OutputDetector,
    overlay_manager: OverlayManager,
    suggestion_engine: SuggestionEngine,

    // --- AI engine ---
    ai_executor: AiCommandExecutor,
    /// Channel to receive async AI responses from the background tokio runtime.
    ai_response_rx: std::sync::mpsc::Receiver<AiResponseMsg>,
    /// Sender cloned into spawned AI tasks.
    ai_response_tx: std::sync::mpsc::Sender<AiResponseMsg>,

    // --- Natural Language Agent ---
    natural_agent: NaturalLanguageAgent,
    /// Pending natural language result awaiting user confirmation.
    natural_result: Option<NaturalLanguageResult>,
    /// Index of the command currently being proposed for confirmation.
    natural_command_index: usize,
    /// The natural language input currently being processed.
    natural_input: Option<String>,

    // --- State ---
    last_frame: Instant,
    needs_redraw: bool,
    font_size: f32,
    shell: String,
    /// Pending command awaiting confirmation (consequence warning active).
    pending_command: Option<String>,
    /// Buffer for keystrokes when input starts with '/' (potential AI command).
    /// Prevents partial commands from leaking to the PTY shell.
    slash_command_buffer: Vec<Vec<u8>>,

    // --- Selection & mouse ---
    selection: Selection,
    /// Last mouse position in physical pixels.
    mouse_pos: (f64, f64),
    /// Timestamp of last click (for double/triple-click detection).
    last_click_time: Instant,
    /// Number of consecutive clicks (1=single, 2=double, 3=triple).
    click_count: u32,
    /// Scrollback viewport offset (0 = live, >0 = scrolled up).
    scroll_offset: usize,

    // --- Agent integration ---
    /// Receiver for commands from the SAP agent server.
    agent_rx: Option<std::sync::mpsc::Receiver<crate::agent::server::AgentMsg>>,
    /// Sender for state updates to the SAP agent server.
    agent_tx: Option<std::sync::mpsc::Sender<crate::agent::server::AgentStateUpdate>>,
}

/// Message sent from background AI tasks back to the main event loop.
enum AiResponseMsg {
    /// A successful AI response to display.
    Success(String),
    /// An error message from an AI call.
    Error(String),
}

impl TerminalApp {
    pub fn new(settings: Settings, shell: &str, agent_mode: bool) -> Self {
        Self {
            settings,
            shell: shell.to_string(),
            state: None,
            modifiers: ModifiersState::empty(),
            agent_mode,
        }
    }

    pub fn run(self) -> Result<()> {
        let event_loop = EventLoop::new().context("Failed to create event loop")?;
        event_loop.set_control_flow(ControlFlow::Poll);

        let mut app = self;
        event_loop
            .run_app(&mut app)
            .context("Event loop terminated with error")?;

        Ok(())
    }
}

impl ApplicationHandler for TerminalApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }

        let window_attrs = Window::default_attributes()
            .with_title("Stratum Terminal")
            .with_inner_size(LogicalSize::new(1200, 800))
            .with_min_inner_size(LogicalSize::new(400, 300));

        let window = match event_loop.create_window(window_attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                tracing::error!("Failed to create window: {}", e);
                event_loop.exit();
                return;
            }
        };

        let font_size = self.settings.font_size;
        let mut renderer = match GpuRenderer::new(window.clone(), font_size) {
            Ok(r) => r,
            Err(e) => {
                tracing::error!("GPU init failed: {}. Ensure drivers are installed.", e);
                event_loop.exit();
                return;
            }
        };

        // Apply theme from config
        renderer.set_theme(&self.settings.theme);
        tracing::info!("Theme: {}", renderer.theme.name);

        let (cols, rows) = renderer.grid_dimensions();
        tracing::info!("Terminal grid: {}x{}", cols, rows);

        let mut panes = HashMap::new();
        let pane_id = PaneId(0);
        let shell_name = self.shell.clone();
        match TerminalPane::new(&self.shell, cols as u16, rows as u16) {
            Ok(mut pane) => {
                // Inject shell integration hook after a brief delay
                // to let the shell initialize
                pane.inject_shell_hook(&shell_name);
                panes.insert(pane_id, pane);
            }
            Err(e) => {
                tracing::error!("Failed to create terminal: {}", e);
                event_loop.exit();
                return;
            }
        }

        let mut pane_tree = PaneTree::new();
        pane_tree.layout(PaneRect {
            x: 0.0,
            y: 0.0,
            width: renderer.size.width as f32,
            height: renderer.size.height as f32,
        });

        let mut overlay_manager = OverlayManager::new();
        overlay_manager.toast(ToastContent::info(format!(
            "Stratum v{} — GPU: {}",
            env!("CARGO_PKG_VERSION"),
            "ready"
        )));

        let (ai_tx, ai_rx) = std::sync::mpsc::channel::<AiResponseMsg>();

        // Spawn agent server if in agent mode
        let (agent_rx, agent_tx) = if self.agent_mode {
            let version = env!("CARGO_PKG_VERSION").to_string();
            let shell_name = self.shell.clone();
            // Get PID of the first pane's PTY process (or use current process)
            let pid = std::process::id();
            let (rx, tx) = crate::agent::server::spawn_agent_server(version, shell_name, pid);
            tracing::info!("SAP agent server spawned — listening on stdin/stdout");
            (Some(rx), Some(tx))
        } else {
            (None, None)
        };

        self.state = Some(AppState {
            window,
            renderer,
            pane_tree,
            tab_manager: TabManager::new(),
            panes,
            input_tracker: InputTracker::new(),
            consequence_analyzer: ConsequenceAnalyzer::new(),
            mutation_analyzer: MutationAnalyzer::new(),
            doc_overlay: DocOverlay::new(),
            output_detector: OutputDetector::new(),
            overlay_manager,
            suggestion_engine: SuggestionEngine::new(),
            ai_executor: AiCommandExecutor::new(),
            ai_response_rx: ai_rx,
            ai_response_tx: ai_tx,
            natural_agent: NaturalLanguageAgent::new(),
            natural_result: None,
            natural_command_index: 0,
            natural_input: None,
            last_frame: Instant::now(),
            needs_redraw: true,
            font_size,
            shell: self.shell.clone(),
            pending_command: None,
            slash_command_buffer: Vec::new(),
            selection: Selection::new(),
            mouse_pos: (0.0, 0.0),
            last_click_time: Instant::now() - Duration::from_secs(10),
            click_count: 0,
            scroll_offset: 0,
            agent_rx,
            agent_tx,
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let state = match &mut self.state {
            Some(s) => s,
            None => return,
        };

        match event {
            WindowEvent::CloseRequested => {
                tracing::info!("Shutdown requested");
                event_loop.exit();
            }

            WindowEvent::Resized(new_size) => {
                state.handle_resize(new_size);
            }

            WindowEvent::ModifiersChanged(new_modifiers) => {
                self.modifiers = new_modifiers.state();
            }

            WindowEvent::KeyboardInput {
                event: key_event,
                is_synthetic: false,
                ..
            } => {
                if key_event.state == ElementState::Pressed {
                    state.handle_key_press(&key_event, self.modifiers, event_loop);
                }
            }

            WindowEvent::RedrawRequested => {
                state.update_and_render();
            }

            // --- Mouse events for selection ---
            WindowEvent::CursorMoved { position, .. } => {
                state.mouse_pos = (position.x, position.y);
                if state.selection.dragging {
                    let grid_pos = state.pixel_to_grid(position.x, position.y);
                    state.selection.update(grid_pos);
                    state.needs_redraw = true;
                }
            }

            WindowEvent::MouseInput { state: btn_state, button, .. } => {
                match (button, btn_state) {
                    (winit::event::MouseButton::Left, ElementState::Pressed) => {
                        state.handle_mouse_click();
                    }
                    (winit::event::MouseButton::Left, ElementState::Released) => {
                        state.selection.finish();
                        // Auto-copy on selection (like most terminals)
                        if state.selection.active {
                            let text = state.extract_selection_text();
                            if !text.is_empty() {
                                if let Err(e) = crate::clipboard::copy_to_clipboard(&text) {
                                    tracing::warn!("Selection copy failed: {}", e);
                                }
                            }
                        }
                    }
                    (winit::event::MouseButton::Right, ElementState::Pressed) => {
                        // Right-click paste
                        state.paste_from_clipboard();
                    }
                    _ => {}
                }
            }

            WindowEvent::MouseWheel { delta, .. } => {
                state.handle_mouse_wheel(delta);
            }

            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let state = match &mut self.state {
            Some(s) => s,
            None => return,
        };

        // Process PTY output from all panes
        for pane in state.panes.values_mut() {
            if pane.process_pty_output() {
                state.needs_redraw = true;
            }
        }

        // Process Stratum slash commands from shell hooks
        // These arrive via OSC sequences: ESC]STRATUM;cmd_name BEL
        let mut stratum_cmds = Vec::new();
        for pane in state.panes.values_mut() {
            let cmds = pane.parser.take_stratum_commands();
            stratum_cmds.extend(cmds);
        }
        for cmd in stratum_cmds {
            tracing::info!("Processing Stratum slash command: /{}", cmd);
            let full_cmd = format!("/{}", cmd);
            let is_ai = state.on_command_submitted(full_cmd.clone());
            if is_ai {
                state.overlay_manager.toast(ToastContent::info(
                    format!("▸ /{}", cmd),
                ));
            } else {
                state.overlay_manager.toast(ToastContent::warning(
                    format!("Unknown command: /{}", cmd),
                ));
            }
            state.needs_redraw = true;
        }

        // Poll for AI responses (non-blocking)
        while let Ok(msg) = state.ai_response_rx.try_recv() {
            match msg {
                AiResponseMsg::Success(text) => {
                    // Check if this is a natural language result (JSON with commands)
                    if text.contains(r#""type":"natural_result""#) {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&text) {
                            let explanation = parsed["explanation"].as_str().unwrap_or("").to_string();
                            let commands: Vec<ProposedCommand> = parsed["commands"]
                                .as_array()
                                .map(|arr| {
                                    arr.iter().filter_map(|c| {
                                        Some(ProposedCommand {
                                            command: c["command"].as_str()?.to_string(),
                                            description: c["description"].as_str()?.to_string(),
                                        })
                                    }).collect()
                                })
                                .unwrap_or_default();

                            if !commands.is_empty() {
                                // Show explanation as a toast
                                state.overlay_manager.toast(ToastContent::info(explanation.clone()));
                                // Store result for step-by-step confirmation
                                state.natural_result = Some(NaturalLanguageResult {
                                    explanation,
                                    proposed_commands: commands.clone(),
                                });
                                state.natural_command_index = 0;
                                // Show first command for confirmation
                                state.prompt_ai_command();
                            } else {
                                // No commands — just show explanation
                                state.overlay_manager.toast(ToastContent::info(explanation));
                            }
                        }
                    } else {
                        state.overlay_manager.toast(ToastContent::info(text));
                    }
                    state.needs_redraw = true;
                }
                AiResponseMsg::Error(err) => {
                    state.overlay_manager.toast(ToastContent::warning(err));
                    state.needs_redraw = true;
                }
            }
        }

        // Poll for agent commands (non-blocking)
        // Collect first to avoid borrow conflict with handle_agent_command(&mut self)
        let agent_msgs: Vec<_> = state.agent_rx.as_ref()
            .map(|rx| {
                let mut msgs = Vec::new();
                while let Ok(msg) = rx.try_recv() {
                    msgs.push(msg);
                }
                msgs
            })
            .unwrap_or_default();

        for msg in agent_msgs {
            match msg {
                crate::agent::server::AgentMsg::Command(cmd) => {
                    state.handle_agent_command(cmd);
                }
                crate::agent::server::AgentMsg::Disconnected => {
                    tracing::info!("Agent disconnected");
                }
            }
        }

        // Cleanup expired toasts
        state.overlay_manager.cleanup_toasts();

        // Frame pacing
        let now = Instant::now();
        if state.needs_redraw || now - state.last_frame >= FRAME_DURATION {
            state.window.request_redraw();
            state.last_frame = now;
        }
    }
}

impl AppState {
    /// Handle a command received from the SAP agent server.
    fn handle_agent_command(&mut self, cmd: crate::agent::handler::AgentCommand) {
        use crate::agent::handler::AgentCommand;
        let active_pane_id = self.pane_tree.active_pane();

        match cmd {
            AgentCommand::Execute(command) => {
                tracing::info!("Agent execute: {}", command);
                if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                    let input = format!("{}\n", command);
                    let _ = pane.write(input.as_bytes());
                }
                self.needs_redraw = true;
            }
            AgentCommand::Write(text) => {
                if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                    let _ = pane.write(text.as_bytes());
                }
                self.needs_redraw = true;
            }
            AgentCommand::Resize { cols, rows } => {
                tracing::info!("Agent resize: {}x{}", cols, rows);
                for pane in self.panes.values_mut() {
                    pane.screen.resize(cols, rows);
                    let _ = pane.resize(cols as u16, rows as u16);
                }
                self.needs_redraw = true;
            }
            AgentCommand::SetTheme(name) => {
                tracing::info!("Agent set theme: {}", name);
                self.renderer.set_theme(&name);
                self.overlay_manager.toast(ToastContent::info(
                    format!("Theme → {} (via agent)", self.renderer.theme.name),
                ));
                self.needs_redraw = true;
            }
            AgentCommand::Clear => {
                // Send ANSI clear screen + cursor home escape sequence
                if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                    let _ = pane.write(b"\x1b[2J\x1b[H");
                }
                self.needs_redraw = true;
            }
            AgentCommand::Signal(sig) => {
                tracing::info!("Agent signal: {}", sig);
                if sig == "SIGINT" || sig == "INT" {
                    if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                        let _ = pane.write(&[0x03]); // ETX = Ctrl+C
                    }
                }
            }
            AgentCommand::Shutdown => {
                tracing::info!("Agent requested shutdown");
                self.window.request_redraw();
            }
            AgentCommand::RequestState => {
                self.send_state_to_agent(active_pane_id);
            }
        }
    }

    /// Send current terminal state to the agent server.
    fn send_state_to_agent(&self, pane_id: PaneId) {
        if let Some(ref agent_tx) = self.agent_tx {
            if let Some(pane) = self.panes.get(&pane_id) {
                let mut rows = Vec::new();
                for row in 0..pane.screen.height() {
                    let mut line = String::new();
                    for col in 0..pane.screen.width() {
                        let cell = pane.screen.get_cell(col, row);
                        line.push(cell.ch);
                    }
                    rows.push(line.trim_end().to_string());
                }
                let (cur_col, cur_row) = pane.screen.cursor_position();
                let _ = agent_tx.send(
                    crate::agent::server::AgentStateUpdate::ScreenUpdate {
                        rows,
                        cursor_col: cur_col,
                        cursor_row: cur_row,
                        cols: pane.screen.width(),
                        height: pane.screen.height(),
                    },
                );
            }
        }
    }

    fn handle_resize(&mut self, new_size: PhysicalSize<u32>) {
        if new_size.width == 0 || new_size.height == 0 {
            return;
        }

        self.renderer.resize(new_size);

        self.pane_tree.layout(PaneRect {
            x: 0.0,
            y: 0.0,
            width: new_size.width as f32,
            height: new_size.height as f32,
        });

        let cell_w = self.renderer.glyph_atlas.cell_width;
        let cell_h = self.renderer.glyph_atlas.cell_height;

        for (pane_id, pane) in &mut self.panes {
            if let Some(rect) = self.pane_tree.get_rect(*pane_id) {
                let (cols, rows) = rect.grid_size(cell_w, cell_h);
                let _ = pane.resize(cols as u16, rows as u16);
            }
        }

        let (cols, rows) = self.renderer.grid_dimensions();

        self.needs_redraw = true;
    }

    fn handle_key_press(
        &mut self,
        event: &KeyEvent,
        modifiers: ModifiersState,
        event_loop: &ActiveEventLoop,
    ) {
        let ctrl = modifiers.control_key();
        let shift = modifiers.shift_key();

        // --- Handle consequence warning modal (also used by AI Agent) ---
        if self.overlay_manager.has_modal() {
            match &event.logical_key {
                // Enter = confirm execution
                Key::Named(NamedKey::Enter) => {
                    if let Some(cmd) = self.pending_command.take() {
                        self.overlay_manager.dismiss_consequence();
                        // Check if this is an AI agent command
                        if self.natural_result.is_some() {
                            self.execute_ai_command(cmd);
                        } else {
                            self.overlay_manager.toast(ToastContent::warning(
                                format!("Executing: {}", cmd),
                            ));
                        }
                        self.needs_redraw = true;
                    }
                    return;
                }
                // Escape = cancel / skip
                Key::Named(NamedKey::Escape) => {
                    self.pending_command = None;
                    self.overlay_manager.dismiss_consequence();
                    if self.natural_result.is_some() {
                        // Skip this command, move to next
                        self.advance_ai_agent();
                    } else {
                        self.overlay_manager.toast(ToastContent::info("Command cancelled"));
                    }
                    self.needs_redraw = true;
                    return;
                }
                // Tab = execute all remaining AI commands
                Key::Named(NamedKey::Tab) => {
                    if self.natural_result.is_some() {
                        self.pending_command = None;
                        self.overlay_manager.dismiss_consequence();
                        self.execute_all_ai_commands();
                        self.needs_redraw = true;
                        return;
                    }
                }
                _ => return,
            }
        }

        // --- Application keybindings (Ctrl+Shift+…) ---
        if ctrl && shift {
            let matched = match &event.physical_key {
                PhysicalKey::Code(KeyCode::KeyT) => {
                    self.new_tab();
                    true
                }
                PhysicalKey::Code(KeyCode::KeyW) => {
                    self.close_active_pane(event_loop);
                    true
                }
                PhysicalKey::Code(KeyCode::KeyE) => {
                    self.split_pane(Direction::Vertical);
                    true
                }
                PhysicalKey::Code(KeyCode::KeyO) => {
                    self.split_pane(Direction::Horizontal);
                    true
                }
                PhysicalKey::Code(KeyCode::BracketRight) => {
                    // Ctrl+Shift+] = next pane
                    self.focus_next_pane();
                    true
                }
                PhysicalKey::Code(KeyCode::BracketLeft) => {
                    // Ctrl+Shift+[ = previous pane
                    self.focus_prev_pane();
                    true
                }
                PhysicalKey::Code(KeyCode::KeyC) => {
                    // Ctrl+Shift+C = copy selection
                    let text = self.extract_selection_text();
                    if !text.is_empty() {
                        match crate::clipboard::copy_to_clipboard(&text) {
                            Ok(()) => {
                                self.overlay_manager.toast(ToastContent::info(
                                    format!("Copied {} chars", text.len()),
                                ));
                            }
                            Err(e) => {
                                self.overlay_manager.toast(ToastContent::warning(
                                    format!("Copy failed: {}", e),
                                ));
                            }
                        }
                        self.needs_redraw = true;
                    }
                    true
                }
                PhysicalKey::Code(KeyCode::KeyV) => {
                    // Ctrl+Shift+V = paste
                    self.paste_from_clipboard();
                    true
                }
                _ => false,
            };
            if matched {
                return;
            }
        }

        // Ctrl+Tab → cycle tabs
        if ctrl && event.logical_key == Key::Named(NamedKey::Tab) {
            if shift {
                self.tab_manager.prev_tab();
            } else {
                self.tab_manager.next_tab();
            }
            self.needs_redraw = true;
            return;
        }

        // --- Suggestion card navigation (intercept BEFORE PTY) ---
        if self.suggestion_engine.card.visible {
            match &event.logical_key {
                Key::Named(NamedKey::ArrowDown) => {
                    self.suggestion_engine.card.select_next();
                    self.needs_redraw = true;
                    return;
                }
                Key::Named(NamedKey::ArrowUp) => {
                    self.suggestion_engine.card.select_prev();
                    self.needs_redraw = true;
                    return;
                }
                Key::Named(NamedKey::Tab) => {
                    // Accept the selected suggestion
                    if let Some(insert_text) = self.suggestion_engine.card.accept_selected() {
                        // Calculate how much of the current partial to replace
                        let current = self.input_tracker.current_input().to_string();
                        let parts: Vec<&str> = current.split_whitespace().collect();
                        let partial = if parts.len() > 1 && !current.ends_with(' ') {
                            parts.last().copied().unwrap_or("")
                        } else {
                            ""
                        };
                        // Send backspaces to erase the partial, then the insert text
                        let mut bytes_to_send = Vec::new();
                        for _ in 0..partial.len() {
                            bytes_to_send.push(0x7f); // DEL/backspace
                        }
                        bytes_to_send.extend_from_slice(insert_text.as_bytes());
                        // Feed tracker + send to PTY
                        self.input_tracker.feed(&bytes_to_send);
                        let active_pane_id = self.pane_tree.active_pane();
                        if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                            if !pane.exited {
                                let _ = pane.write(&bytes_to_send);
                            }
                        }
                        self.suggestion_engine.card.hide();
                        // Trigger doc update for the new input
                        if self.input_tracker.take_dirty() {
                            self.update_inline_docs();
                            self.update_suggestions();
                        }
                        self.needs_redraw = true;
                        return;
                    }
                }
                Key::Named(NamedKey::Escape) => {
                    self.suggestion_engine.card.hide();
                    self.needs_redraw = true;
                    return;
                }
                _ => {} // Fall through to normal key handling
            }
        }

        // --- Convert key to PTY bytes ---
        let bytes = key_to_bytes(event, modifiers);
        if bytes.is_empty() {
            return;
        }

        // Reset scrollback viewport to bottom on typing
        self.scroll_offset = 0;

        // --- Natural Language Interception ---
        // If Enter is pressed, check if the current input is natural language.
        // If so, intercept it: clear the shell line, don't send Enter, route to AI.
        let is_enter = bytes == b"\r" || bytes == b"\n";
        let current_input = self.input_tracker.current_input().to_string();
        let intercept_natural = is_enter
            && !current_input.is_empty()
            && crate::ai::natural::is_natural_language(&current_input);

        // --- Feed input tracker (for inline docs + suggestions) ---
        let _submitted_command = self.input_tracker.feed(&bytes);

        if self.input_tracker.take_dirty() {
            self.update_inline_docs();
            self.update_suggestions();
        }

        // --- Intercept slash commands and natural language BEFORE sending to PTY ---
        if intercept_natural {
            // Clear the shell line with Ctrl+U (0x15)
            let active_pane_id = self.pane_tree.active_pane();
            if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                let _ = pane.write(b"\x15"); // Ctrl+U clears the current line
            }
            // Dispatch to AI agent
            self.dispatch_natural_language(current_input);
            self.needs_redraw = true;
            return;
        }

        // --- Intercept /slash commands (handle inline instead of shell hook) ---
        if let Some(cmd) = _submitted_command {
            if cmd.starts_with('/') {
                let is_ai = self.on_command_submitted(cmd.clone());
                if is_ai {
                    // Clear the shell line so the /command doesn't get executed
                    let active_pane_id = self.pane_tree.active_pane();
                    if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                        let _ = pane.write(b"\x15"); // Ctrl+U = clear line
                    }
                    self.needs_redraw = true;
                    return;
                }
            }
        }

        // --- Send directly to PTY ---
        let active_pane_id = self.pane_tree.active_pane();
        if let Some(pane) = self.panes.get_mut(&active_pane_id) {
            if !pane.exited {
                if let Err(e) = pane.write(&bytes) {
                    tracing::warn!("PTY write error: {}", e);
                }
            }
        }
    }

    /// Update the suggestion card based on current input.
    fn update_suggestions(&mut self) {
        let input = self.input_tracker.current_input().to_string();
        self.suggestion_engine.update(&input, &self.doc_overlay);

        // Bridge: convert SuggestionCardState → OverlayManager SuggestionCard
        if self.suggestion_engine.card.visible && !self.suggestion_engine.card.items.is_empty() {
            let card = &self.suggestion_engine.card;

            // Get cursor position for card placement
            let (cx, cy) = self
                .panes
                .get(&self.pane_tree.active_pane())
                .map(|p| p.screen.cursor_position())
                .unwrap_or((0, 0));

            use crate::renderer::overlay::{
                SuggestionCardContent, SuggestionCardItem, OverlayPosition,
            };

            let items: Vec<SuggestionCardItem> = card.items.iter().map(|item| {
                SuggestionCardItem {
                    icon: item.kind.icon(),
                    label: item.label.clone(),
                    detail: item.detail.clone(),
                    color: item.kind.color(),
                }
            }).collect();

            self.overlay_manager.show_suggestion_card(SuggestionCardContent {
                command: card.command.clone(),
                items,
                selected_index: card.selected_index,
                position: OverlayPosition::BelowCursor { col: cx, row: cy },
                scroll_offset: card.scroll_offset,
                max_visible: card.max_visible,
            });
        } else {
            self.overlay_manager.hide_suggestion_card();
        }

        self.needs_redraw = true;
    }

    /// Called when the user submits a command (Enter pressed).
    /// Returns `true` if the command was an AI command (should not be sent to PTY).
    fn on_command_submitted(&mut self, command: String) -> bool {
        tracing::debug!("Command submitted: {}", command);

        // --- Theme switching commands ---
        let trimmed = command.trim();
        if trimmed == "/themes" {
            let names = crate::config::theme::theme_names();
            let current = self.renderer.theme.name;
            let msg = names.iter()
                .map(|n| if *n == current { format!("▸ {} (active)", n) } else { n.to_string() })
                .collect::<Vec<_>>()
                .join(" │ ");
            self.overlay_manager.toast(ToastContent::info(format!("Themes: {}", msg)));
            self.needs_redraw = true;
            return true;
        }
        if let Some(name) = trimmed.strip_prefix("/theme ") {
            let name = name.trim();
            self.renderer.set_theme(name);
            self.overlay_manager.toast(ToastContent::info(
                format!("Theme → {}", self.renderer.theme.name),
            ));
            self.needs_redraw = true;
            return true;
        }

        // --- AI Command interception ---
        // If the command starts with '/', check if it's an AI command.
        // AI commands are NOT sent to the PTY — they're handled internally.
        if let Some(ai_cmd) = AiCommand::parse(&command) {
            tracing::info!("AI command intercepted: {:?}", ai_cmd);

            // Handle synchronous commands directly
            match &ai_cmd {
                AiCommand::Config => {
                    let output = self.ai_executor.registry
                        .configured_providers()
                        .len();
                    let mut text = format!("Stratum AI — {}/29 providers configured", output);
                    if let Some(ref name) = self.ai_executor.credentials.active_provider {
                        text.push_str(&format!(" │ Active: {}", name));
                    }
                    self.overlay_manager.toast(ToastContent::info(text));
                    self.needs_redraw = true;
                    return true;
                }
                AiCommand::Providers => {
                    let mut lines = Vec::new();
                    for p in self.ai_executor.registry.all_providers() {
                        let status = if p.is_configured() { "✓" } else { "✗" };
                        lines.push(format!("{} {}", status, p.config.display_name));
                    }
                    // Show as multiple toasts (first 10 + summary)
                    let configured = self.ai_executor.registry.configured_providers().len();
                    self.overlay_manager.toast(ToastContent::info(
                        format!("29 providers available │ {} configured │ Use /ai-set-key <name> <key>", configured)
                    ));
                    self.needs_redraw = true;
                    return true;
                }
                AiCommand::SetKey { provider, key } => {
                    if self.ai_executor.registry.set_api_key(provider, key.clone()) {
                        self.ai_executor.credentials.set_key(provider, key.clone());
                        let _ = self.ai_executor.credentials.save();
                        self.overlay_manager.toast(ToastContent::info(
                            format!("✓ API key set for '{}'", provider)
                        ));
                    } else {
                        self.overlay_manager.toast(ToastContent::warning(
                            format!("✗ Unknown provider: '{}'", provider)
                        ));
                    }
                    self.needs_redraw = true;
                    return true;
                }
                AiCommand::Models(name) => {
                    if let Some(p) = self.ai_executor.registry.get(name) {
                        let model_list = p.config.models.join(", ");
                        self.overlay_manager.toast(ToastContent::info(
                            format!("{}: {}", p.config.display_name, model_list)
                        ));
                    } else {
                        self.overlay_manager.toast(ToastContent::warning(
                            format!("Unknown provider: '{}'", name)
                        ));
                    }
                    self.needs_redraw = true;
                    return true;
                }
                AiCommand::ClearChat => {
                    self.ai_executor.chat_engine.clear();
                    self.overlay_manager.toast(ToastContent::info("Chat history cleared"));
                    self.needs_redraw = true;
                    return true;
                }
                AiCommand::SetProvider(ref name) => {
                    let name = name.clone();
                    if self.ai_executor.registry.get(&name).is_some() {
                        self.ai_executor.credentials.active_provider = Some(name.clone());
                        let _ = self.ai_executor.credentials.save();
                        self.overlay_manager.toast(ToastContent::info(
                            format!("✓ Active provider → {}", name)
                        ));
                    } else {
                        self.overlay_manager.toast(ToastContent::warning(
                            format!("✗ Unknown provider: '{}'", name)
                        ));
                    }
                    self.needs_redraw = true;
                    return true;
                }
                AiCommand::SetModel(ref model) => {
                    let model = model.clone();
                    self.ai_executor.credentials.active_model = Some(model.clone());
                    let _ = self.ai_executor.credentials.save();
                    self.overlay_manager.toast(ToastContent::info(
                        format!("✓ Active model → {}", model)
                    ));
                    self.needs_redraw = true;
                    return true;
                }
                // Async commands — dispatch to background thread
                _ => {
                    self.overlay_manager.toast(ToastContent::info("⏳ Thinking..."));
                    self.needs_redraw = true;
                    self.dispatch_ai_command(ai_cmd);
                    return true;
                }
            }
        }

        // Record in doc overlay history
        self.doc_overlay.record_command(&command);

        // Record in suggestion history provider
        // (HistoryProvider is inside providers list — we access it via the engine)
        // For now we just update CWD — history recording will be added when
        // the HistoryProvider gets mutable access through the engine.
        if let Ok(cwd) = std::env::current_dir() {
            self.suggestion_engine.set_cwd(cwd);
        }

        // Clear inline docs
        self.overlay_manager.hide_doc();

        // --- Consequence scoring ---
        let score = self.consequence_analyzer.analyze(&command);
        self.consequence_analyzer.record_command(&command);

        if score.requires_deliberate_execution() {
            let risk = score.risk_level();
            let risk_color = match risk {
                "CRITICAL" => [1.0, 0.15, 0.15, 1.0],
                "HIGH" => [1.0, 0.5, 0.15, 1.0],
                _ => [1.0, 0.8, 0.2, 1.0],
            };

            self.overlay_manager.show_consequence(ConsequenceWarningContent {
                command: command.clone(),
                risk_level: risk.to_string(),
                risk_color,
                reversibility: format!("{:.0}%", score.reversibility * 100.0),
                blast_radius: format!("{:.0}%", score.blast_radius * 100.0),
                message: format!(
                    "⚠ {} risk command detected. Press Enter to confirm, Escape to cancel.",
                    risk
                ),
                requires_confirmation: true,
            });
            self.pending_command = Some(command.clone());
        }

        // --- Mutation preview ---
        if self.mutation_analyzer.is_mutating(&command) {
            let preview = self.mutation_analyzer.analyze(&command);
            let summary = preview.summary();

            if !preview.changes.is_empty() {
                let change_lines: Vec<MutationChangeLine> = preview
                    .changes
                    .iter()
                    .map(|c| match c {
                        FsChange::Create { .. } => {
                            MutationChangeLine::create(c.description())
                        }
                        FsChange::Delete { .. } => {
                            MutationChangeLine::delete(c.description())
                        }
                        FsChange::Modify { .. } => {
                            MutationChangeLine::modify(c.description())
                        }
                        FsChange::Move { .. } => {
                            MutationChangeLine::r#move(c.description())
                        }
                        _ => MutationChangeLine::modify(c.description()),
                    })
                    .collect();

                self.overlay_manager.show_mutation(MutationPreviewContent {
                    command: command.clone(),
                    summary_line: summary.one_line(),
                    changes: change_lines,
                    has_destructive: summary.has_destructive,
                    position: OverlayPosition::Bottom,
                });
            }
        } else {
            self.overlay_manager.hide_mutation();
        }

        self.needs_redraw = true;
        false
    }

    /// Dispatch an async AI command to a background thread.
    ///
    /// Since the winit event loop is synchronous, we spawn a dedicated thread
    /// with a one-shot tokio runtime to execute the async AI call. The response
    /// is sent back to the main thread via `ai_response_tx`.
    fn dispatch_ai_command(&mut self, command: AiCommand) {
        use crate::ai::chat::ChatEngine;

        // Clone what we need for the background thread
        let tx = self.ai_response_tx.clone();

        // Get the active provider (clone it for the thread)
        let provider = match self.ai_executor.credentials.active_provider.clone() {
            Some(name) => {
                match self.ai_executor.registry.get(&name) {
                    Some(p) if p.is_configured() => p.clone(),
                    _ => {
                        match self.ai_executor.registry.first_configured() {
                            Some(p) => p.clone(),
                            None => {
                                self.overlay_manager.toast(ToastContent::warning(
                                    "No AI provider configured. Use /ai-set-key <provider> <key>"
                                ));
                                self.needs_redraw = true;
                                return;
                            }
                        }
                    }
                }
            }
            None => {
                match self.ai_executor.registry.first_configured() {
                    Some(p) => p.clone(),
                    None => {
                        self.overlay_manager.toast(ToastContent::warning(
                            "No AI provider configured. Use /ai-set-key <provider> <key>"
                        ));
                        self.needs_redraw = true;
                        return;
                    }
                }
            }
        };

        // Build the prompt based on command type
        let (system_prompt, user_prompt) = match &command {
            AiCommand::Ask(query) => (
                "You are Stratum AI, a terminal assistant. Keep responses concise and practical.".to_string(),
                query.clone(),
            ),
            AiCommand::Explain(ctx) => {
                let prompt = match ctx {
                    Some(c) => format!(
                        "Explain this terminal error or output concisely. What went wrong and how to fix it:\n\n{}",
                        c
                    ),
                    None => "Explain the most common terminal errors and how to fix them.".into(),
                };
                (
                    "You are a terminal error expert. Explain errors concisely with actionable fixes.".to_string(),
                    prompt,
                )
            }
            AiCommand::Suggest => {
                let cwd = std::env::current_dir()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|_| "unknown".into());
                (
                    "You are a shell command expert. Suggest practical commands.".to_string(),
                    format!(
                        "Based on the current directory '{}', suggest 3-5 useful shell commands \
                         the user might want to run. Format as a numbered list with brief descriptions.",
                        cwd
                    ),
                )
            }
            AiCommand::Translate(cmd) => (
                "You are a cross-platform command translator. Show equivalent commands.".to_string(),
                format!(
                    "Translate this command to all major platforms. Show Windows (cmd/PowerShell), \
                     macOS, and Linux equivalents:\n\n{}",
                    cmd
                ),
            ),
            AiCommand::Test => (
                "You are a test assistant.".to_string(),
                "Reply with exactly: 'Stratum AI connection successful.' and nothing else.".to_string(),
            ),
            _ => return, // Sync commands already handled above
        };

        // Spawn background thread with a tokio runtime
        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(AiResponseMsg::Error(format!("Failed to create runtime: {}", e)));
                    return;
                }
            };

            rt.block_on(async {
                match ChatEngine::query(&provider, &system_prompt, &user_prompt).await {
                    Ok(response) => {
                        let mut output = response.content.clone();
                        // Add model info footer
                        if let Some(ref usage) = response.usage {
                            output.push_str(&format!(
                                "\n─── {} │ {} tokens",
                                response.model, usage.total_tokens
                            ));
                        }
                        let _ = tx.send(AiResponseMsg::Success(output));
                    }
                    Err(e) => {
                        let _ = tx.send(AiResponseMsg::Error(e.to_string()));
                    }
                }
            });
        });
    }

    /// Dispatch a natural language request to the AI agent.
    fn dispatch_natural_language(&mut self, input: String) {
        use crate::ai::chat::ChatEngine;

        self.natural_input = Some(input.clone());

        // Show processing indicator as toast (AiAgent overlay isn't GPU-rendered yet)
        self.overlay_manager.toast(ToastContent::info("⏳ AI Agent thinking..."));

        // Get the active provider
        let provider = match self.ai_executor.credentials.active_provider.clone() {
            Some(name) => {
                match self.ai_executor.registry.get(&name) {
                    Some(p) if p.is_configured() => p.clone(),
                    _ => {
                        match self.ai_executor.registry.first_configured() {
                            Some(p) => p.clone(),
                            None => {
                                self.overlay_manager.hide_ai_agent();
                                self.overlay_manager.toast(ToastContent::warning(
                                    "No AI provider configured. Use /ai-set-key <provider> <key>"
                                ));
                                self.needs_redraw = true;
                                return;
                            }
                        }
                    }
                }
            }
            None => {
                match self.ai_executor.registry.first_configured() {
                    Some(p) => p.clone(),
                    None => {
                        self.overlay_manager.hide_ai_agent();
                        self.overlay_manager.toast(ToastContent::warning(
                            "No AI provider configured. Use /ai-set-key <provider> <key>"
                        ));
                        self.needs_redraw = true;
                        return;
                    }
                }
            }
        };

        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".into());

        // Clone what we need for the background thread
        let tx = self.ai_response_tx.clone();

        std::thread::spawn(move || {
            let rt = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => {
                    let _ = tx.send(AiResponseMsg::Error(format!("AI runtime error: {}", e)));
                    return;
                }
            };

            rt.block_on(async {
                let mut agent = NaturalLanguageAgent::new();
                match agent.process(&input, &cwd, &provider, false).await {
                    Ok(result) => {
                        if result.proposed_commands.is_empty() {
                            // No commands to execute — show explanation
                            let _ = tx.send(AiResponseMsg::Success(result.explanation));
                        } else {
                            // Commands proposed — send as structured response
                            let json = serde_json::json!({
                                "type": "natural_result",
                                "explanation": result.explanation,
                                "commands": result.proposed_commands.iter().map(|c| {
                                    serde_json::json!({
                                        "command": c.command,
                                        "description": c.description,
                                    })
                                }).collect::<Vec<_>>(),
                            });
                            let _ = tx.send(AiResponseMsg::Success(json.to_string()));
                        }
                    }
                    Err(e) => {
                        let _ = tx.send(AiResponseMsg::Error(format!("AI error: {}", e)));
                    }
                }
            });
        });
    }

    /// Execute a single command proposed by the AI agent.
    fn execute_ai_command(&mut self, command: String) {
        let active_pane_id = self.pane_tree.active_pane();
        if let Some(pane) = self.panes.get_mut(&active_pane_id) {
            if !pane.exited {
                let full_command = format!("{}\n", command);
                if let Err(e) = pane.write(full_command.as_bytes()) {
                    tracing::warn!("PTY write error: {}", e);
                }
                self.overlay_manager.toast(ToastContent::info(
                    format!("▸ {}", command),
                ));
            }
        }
        // Move to next command
        self.advance_ai_agent();
    }

    /// Move to the next proposed command or dismiss.
    fn advance_ai_agent(&mut self) {
        self.natural_command_index += 1;
        self.overlay_manager.dismiss_consequence();
        self.prompt_ai_command();
    }

    /// Execute all remaining AI-proposed commands.
    fn execute_all_ai_commands(&mut self) {
        if let Some(ref result) = self.natural_result.clone() {
            let active_pane_id = self.pane_tree.active_pane();
            // Execute all remaining commands sequentially
            for i in self.natural_command_index..result.proposed_commands.len() {
                let cmd = &result.proposed_commands[i];
                if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                    if !pane.exited {
                        let full_command = format!("{}\n", cmd.command);
                        if let Err(e) = pane.write(full_command.as_bytes()) {
                            tracing::warn!("PTY write error: {}", e);
                        }
                        self.overlay_manager.toast(ToastContent::info(
                            format!("▸ {} — {}", cmd.command, cmd.description),
                        ));
                    }
                }
            }
            self.finish_ai_agent();
        }
    }

    /// Show the first/next proposed AI command for user confirmation.
    fn prompt_ai_command(&mut self) {
        if let Some(ref result) = self.natural_result.clone() {
            if self.natural_command_index < result.proposed_commands.len() {
                let cmd = &result.proposed_commands[self.natural_command_index];
                let is_dangerous = self.natural_agent.check_dangerous(&cmd.command);
                let remaining = result.proposed_commands.len() - self.natural_command_index;

                let message = if is_dangerous {
                    let reason = self.natural_agent.danger_reason(&cmd.command);
                    format!(
                        "Step {}/{} — {}\n\n⚠ {}\n\nEnter=execute  Escape=skip  Tab=all",
                        self.natural_command_index + 1,
                        result.proposed_commands.len(),
                        cmd.description,
                        reason,
                    )
                } else {
                    format!(
                        "Step {}/{} — {}\n\n${}\n\nEnter=execute  Escape=skip  Tab=all",
                        self.natural_command_index + 1,
                        result.proposed_commands.len(),
                        cmd.description,
                        cmd.command,
                    )
                };

                self.overlay_manager.show_consequence(ConsequenceWarningContent {
                    command: cmd.command.clone(),
                    risk_level: if remaining > 1 {
                        format!("Step {}/{}", self.natural_command_index + 1, result.proposed_commands.len())
                    } else {
                        "Final Step".to_string()
                    },
                    risk_color: if is_dangerous { [1.0, 0.5, 0.15, 1.0] } else { [0.3, 0.6, 1.0, 1.0] },
                    reversibility: if is_dangerous { "⚠ DANGEROUS".to_string() } else { "Safe".to_string() },
                    blast_radius: format!("{}/{}", self.natural_command_index + 1, result.proposed_commands.len()),
                    message,
                    requires_confirmation: true,
                });
                // Store the command so modal Enter handler can execute it
                self.pending_command = Some(cmd.command.clone());
            } else {
                // All commands processed
                self.finish_ai_agent();
            }
        }
    }

    /// Finish AI agent execution and show summary.
    fn finish_ai_agent(&mut self) {
        let count = self.natural_command_index;
        self.dismiss_ai_agent();
        self.overlay_manager.toast(ToastContent::info(
            format!("✓ AI Agent — {} commands executed", count),
        ));
    }

    /// Dismiss AI agent state.
    fn dismiss_ai_agent(&mut self) {
        self.overlay_manager.dismiss_consequence();
        self.natural_result = None;
        self.natural_command_index = 0;
        self.natural_input = None;
        self.needs_redraw = true;
    }

    /// Update inline documentation based on current input.
    fn update_inline_docs(&mut self) {
        let input = self.input_tracker.current_input().to_string();

        if input.is_empty() {
            self.overlay_manager.hide_doc();
            self.needs_redraw = true;
            return;
        }

        if let Some(overlay_content) = self.doc_overlay.get_overlay(&input) {
            let (cx, cy) = self
                .panes
                .get(&self.pane_tree.active_pane())
                .map(|p| p.screen.cursor_position())
                .unwrap_or((0, 0));

            self.overlay_manager.show_doc(InlineDocContent {
                command: overlay_content.command,
                synopsis: overlay_content.synopsis.unwrap_or_default(),
                flags: overlay_content
                    .relevant_flags
                    .iter()
                    .map(|f| {
                        let flag = f
                            .long
                            .clone()
                            .or(f.short.clone())
                            .unwrap_or_default();
                        InlineDocFlag {
                            flag,
                            description: f.description.clone(),
                        }
                    })
                    .collect(),
                completions: overlay_content.completions,
                history: overlay_content.history_entries,
                position: OverlayPosition::BelowCursor { col: cx, row: cy },
            });
        } else {
            self.overlay_manager.hide_doc();
        }

        self.needs_redraw = true;
    }

    fn new_tab(&mut self) {
        let _tab_id = self.tab_manager.new_tab();
        let (cols, rows) = self.renderer.grid_dimensions();

        match TerminalPane::new(&self.shell, cols as u16, rows as u16) {
            Ok(pane) => {
                let pane_id = PaneId(self.panes.len() as u32 + 100);
                self.panes.insert(pane_id, pane);
                self.overlay_manager.toast(ToastContent::info("New tab opened"));
                self.needs_redraw = true;
            }
            Err(e) => {
                tracing::error!("Failed to create tab: {}", e);
            }
        }
    }

    fn split_pane(&mut self, direction: Direction) {
        let new_pane_id = self.pane_tree.split(direction);

        self.pane_tree.layout(PaneRect {
            x: 0.0,
            y: 0.0,
            width: self.renderer.size.width as f32,
            height: self.renderer.size.height as f32,
        });

        let cell_w = self.renderer.glyph_atlas.cell_width;
        let cell_h = self.renderer.glyph_atlas.cell_height;

        let (cols, rows) = if let Some(rect) = self.pane_tree.get_rect(new_pane_id) {
            rect.grid_size(cell_w, cell_h)
        } else {
            self.renderer.grid_dimensions()
        };

        match TerminalPane::new(&self.shell, cols as u16, rows as u16) {
            Ok(pane) => {
                self.panes.insert(new_pane_id, pane);
                for (pid, p) in &mut self.panes {
                    if let Some(rect) = self.pane_tree.get_rect(*pid) {
                        let (c, r) = rect.grid_size(cell_w, cell_h);
                        let _ = p.resize(c as u16, r as u16);
                    }
                }
                self.pane_tree.set_active(new_pane_id);

                let dir_name = match direction {
                    Direction::Vertical => "vertical",
                    Direction::Horizontal => "horizontal",
                };
                self.overlay_manager.toast(ToastContent::info(
                    format!("Split {}", dir_name),
                ));
                self.needs_redraw = true;
            }
            Err(e) => {
                tracing::error!("Failed to spawn split pane: {}", e);
            }
        }
    }

    fn close_active_pane(&mut self, event_loop: &ActiveEventLoop) {
        let active = self.pane_tree.active_pane();

        if self.panes.len() <= 1 {
            tracing::info!("Last pane closed — exiting");
            event_loop.exit();
            return;
        }

        self.panes.remove(&active);
        self.pane_tree.close(active);

        self.pane_tree.layout(PaneRect {
            x: 0.0,
            y: 0.0,
            width: self.renderer.size.width as f32,
            height: self.renderer.size.height as f32,
        });

        self.overlay_manager.toast(ToastContent::info("Pane closed"));
        self.needs_redraw = true;
    }

    fn update_and_render(&mut self) {
        let (cols, rows) = self.renderer.grid_dimensions();

        // Build pane render list
        let active_pane_id = self.pane_tree.active_pane();
        let all_pane_ids = self.pane_tree.all_panes();

        // Cleanup expired toasts before rendering
        self.overlay_manager.cleanup_toasts();

        // Convert latest NOS Shell structured block into a table overlay
        if let Some(pane) = self.panes.get(&active_pane_id) {
            if pane.is_nos_shell {
                // Remove any previous structured table overlays
                self.overlay_manager.elements.retain(|e| !matches!(e, crate::renderer::overlay::OverlayElement::StructuredTable(_)));

                if let Some(block) = pane.latest_structured_block() {
                    let (_, cursor_row) = pane.screen.cursor_position();
                    if let Some(table) = crate::renderer::overlay::StructuredTableContent::from_json(&block.json, cursor_row) {
                        self.overlay_manager.elements.push(
                            crate::renderer::overlay::OverlayElement::StructuredTable(table)
                        );
                    }
                }
            }
        }

        if all_pane_ids.len() <= 1 && self.tab_manager.tab_count() <= 1 {
            // Single pane, single tab — fast path
            if let Some(pane) = self.panes.get(&active_pane_id) {
                if let Err(e) = self.renderer.render(&pane.screen, &self.selection, &self.overlay_manager.elements, self.scroll_offset) {
                    tracing::error!("Render error: {}", e);
                }
            }
        } else {
            // Multi-pane or multi-tab — use render_multi
            let mut pane_data: Vec<(PaneRect, &ScreenGrid, bool)> = Vec::new();

            for pid in &all_pane_ids {
                if let Some(pane) = self.panes.get(pid) {
                    let rect = self.pane_tree.get_rect(*pid).cloned().unwrap_or(PaneRect {
                        x: 0.0,
                        y: 0.0,
                        width: self.renderer.size.width as f32,
                        height: self.renderer.size.height as f32,
                    });
                    pane_data.push((rect, &pane.screen, *pid == active_pane_id));
                }
            }

            let tab_titles: Vec<(&str, bool)> = self.tab_manager.tabs()
                .iter()
                .enumerate()
                .map(|(i, tab)| (tab.title.as_str(), i == self.tab_manager.active_index()))
                .collect();

            if let Err(e) = self.renderer.render_multi(&pane_data, &tab_titles, &self.selection, &self.overlay_manager.elements, self.scroll_offset) {
                tracing::error!("Multi-pane render error: {}", e);
            }
        }

        self.needs_redraw = false;
    }

    /// Focus the next pane in the pane tree.
    fn focus_next_pane(&mut self) {
        let all = self.pane_tree.all_panes();
        if all.len() <= 1 { return; }
        let active = self.pane_tree.active_pane();
        if let Some(idx) = all.iter().position(|p| *p == active) {
            let next = (idx + 1) % all.len();
            self.pane_tree.set_active(all[next]);
            self.overlay_manager.toast(ToastContent::info(
                format!("Pane {}/{}", next + 1, all.len()),
            ));
            self.needs_redraw = true;
        }
    }

    /// Focus the previous pane in the pane tree.
    fn focus_prev_pane(&mut self) {
        let all = self.pane_tree.all_panes();
        if all.len() <= 1 { return; }
        let active = self.pane_tree.active_pane();
        if let Some(idx) = all.iter().position(|p| *p == active) {
            let prev = if idx == 0 { all.len() - 1 } else { idx - 1 };
            self.pane_tree.set_active(all[prev]);
            self.overlay_manager.toast(ToastContent::info(
                format!("Pane {}/{}", prev + 1, all.len()),
            ));
            self.needs_redraw = true;
        }
    }

    // =========================================================================
    // Mouse & Clipboard helpers
    // =========================================================================

    /// Convert physical pixel coordinates to a grid position (col, row).
    fn pixel_to_grid(&self, px: f64, py: f64) -> GridPos {
        let cell_w = self.renderer.glyph_atlas.cell_width;
        let cell_h = self.renderer.glyph_atlas.cell_height;

        let col = (px as f32 / cell_w).floor().max(0.0) as usize;
        let row = (py as f32 / cell_h).floor().max(0.0) as usize;

        let (grid_cols, grid_rows) = self.renderer.grid_dimensions();
        GridPos::new(
            col.min(grid_cols.saturating_sub(1)),
            row.min(grid_rows.saturating_sub(1)),
        )
    }

    /// Handle mouse click on the suggestion card overlay.
    /// Returns true if the click was handled/consumed by the card.
    fn handle_mouse_click_suggestion_card(&mut self) -> bool {
        if !self.suggestion_engine.card.visible || self.suggestion_engine.card.items.is_empty() {
            return false;
        }

        let active_pane_id = self.pane_tree.active_pane();
        if let Some(pane) = self.panes.get(&active_pane_id) {
            let (cx, cy) = pane.screen.cursor_position();
            let cell_w = self.renderer.glyph_atlas.cell_width;
            let cell_h = self.renderer.glyph_atlas.cell_height;
            let screen_w = self.renderer.size.width as f32;
            let screen_h = self.renderer.size.height as f32;

            let card = &self.suggestion_engine.card;
            let max_vis = card.max_visible.min(card.items.len());
            let row_h = cell_h + 4.0;
            let header_h = cell_h + 6.0;
            let footer_h = cell_h + 2.0;
            let card_w = 420.0_f32;
            let card_h = header_h + (max_vis as f32) * row_h + footer_h;

            let (card_cx, card_cy) = (
                (cx as f32 * cell_w).min(screen_w - card_w - 8.0).max(4.0),
                (cy as f32 + 1.5) * cell_h,
            );
            let card_x = card_cx;
            let card_y = if card_cy + card_h > screen_h - cell_h * 2.0 {
                (card_cy - card_h - cell_h).max(4.0)
            } else {
                card_cy
            };

            let mx = self.mouse_pos.0 as f32;
            let my = self.mouse_pos.1 as f32;

            if mx >= card_x && mx <= card_x + card_w && my >= card_y && my <= card_y + card_h {
                let item_area_y = card_y + header_h;
                let item_area_h = (max_vis as f32) * row_h;

                if my >= item_area_y && my <= item_area_y + item_area_h {
                    let relative_y = my - item_area_y;
                    let clicked_visible_idx = (relative_y / row_h).floor() as usize;
                    if clicked_visible_idx < max_vis {
                        let actual_idx = card.scroll_offset + clicked_visible_idx;
                        if actual_idx < card.items.len() {
                            let insert_text = card.items[actual_idx].insert_text.clone();

                            // Calculate how much of the current partial to replace
                            let current = self.input_tracker.current_input().to_string();
                            let parts: Vec<&str> = current.split_whitespace().collect();
                            let partial = if parts.len() > 1 && !current.ends_with(' ') {
                                parts.last().copied().unwrap_or("")
                            } else {
                                ""
                            };

                            let mut bytes_to_send = Vec::new();
                            for _ in 0..partial.len() {
                                bytes_to_send.push(0x7f); // Backspace
                            }
                            bytes_to_send.extend_from_slice(insert_text.as_bytes());

                            self.input_tracker.feed(&bytes_to_send);
                            let active_pane_id = self.pane_tree.active_pane();
                            if let Some(pane_mut) = self.panes.get_mut(&active_pane_id) {
                                if !pane_mut.exited {
                                    let _ = pane_mut.write(&bytes_to_send);
                                }
                            }

                            self.suggestion_engine.card.hide();
                            self.overlay_manager.hide_suggestion_card();

                            if self.input_tracker.take_dirty() {
                                self.update_inline_docs();
                                self.update_suggestions();
                            }
                            self.needs_redraw = true;
                        }
                    }
                }
                return true;
            }
        }
        false
    }

    /// Handle a left mouse button click — supports single, double, and triple click.
    fn handle_mouse_click(&mut self) {
        if self.handle_mouse_click_suggestion_card() {
            return;
        }

        let now = Instant::now();
        let pos = self.pixel_to_grid(self.mouse_pos.0, self.mouse_pos.1);

        // Multi-click detection (within 400ms)
        let elapsed = now.duration_since(self.last_click_time);
        if elapsed < Duration::from_millis(400) {
            self.click_count = (self.click_count % 3) + 1;
        } else {
            self.click_count = 1;
        }
        self.last_click_time = now;

        match self.click_count {
            1 => {
                // Single click — start normal selection
                self.selection.start(pos, SelectionMode::Normal);
            }
            2 => {
                // Double click — select word
                let active_pane_id = self.pane_tree.active_pane();
                if let Some(pane) = self.panes.get(&active_pane_id) {
                    let width = pane.screen.width();
                    let row = pos.row;
                    let (start_col, end_col) = crate::screen::selection::word_boundaries(
                        pos.col,
                        width,
                        |col| pane.screen.get_cell(col, row).ch,
                    );
                    self.selection.start(GridPos::new(start_col, row), SelectionMode::Word);
                    self.selection.update(GridPos::new(end_col, row));
                }
            }
            3 => {
                // Triple click — select entire line
                let active_pane_id = self.pane_tree.active_pane();
                if let Some(pane) = self.panes.get(&active_pane_id) {
                    let width = pane.screen.width();
                    self.selection.start(GridPos::new(0, pos.row), SelectionMode::Line);
                    self.selection.update(GridPos::new(width.saturating_sub(1), pos.row));
                }
            }
            _ => {}
        }

        self.needs_redraw = true;
    }

    /// Extract the currently selected text from the active pane.
    fn extract_selection_text(&self) -> String {
        if !self.selection.active {
            return String::new();
        }

        let active_pane_id = self.pane_tree.active_pane();
        if let Some(pane) = self.panes.get(&active_pane_id) {
            let width = pane.screen.width();
            let height = pane.screen.height();
            self.selection.extract_text(width, height, |col, row| {
                pane.screen.get_cell_scrolled(col, row, self.scroll_offset).ch
            })
        } else {
            String::new()
        }
    }

    /// Paste text from the system clipboard into the active PTY.
    fn paste_from_clipboard(&mut self) {
        match crate::clipboard::paste_from_clipboard() {
            Ok(text) if !text.is_empty() => {
                // Bracket paste mode: wrap in \x1b[200~ ... \x1b[201~
                // Most modern shells support this for safe pasting.
                let bracketed = format!("\x1b[200~{}\x1b[201~", text);
                let active_pane_id = self.pane_tree.active_pane();
                if let Some(pane) = self.panes.get_mut(&active_pane_id) {
                    if !pane.exited {
                        if let Err(e) = pane.write(bracketed.as_bytes()) {
                            tracing::warn!("Paste write error: {}", e);
                        }
                    }
                }
                self.overlay_manager.toast(ToastContent::info(
                    format!("Pasted {} chars", text.len()),
                ));
                self.needs_redraw = true;
            }
            Ok(_) => {} // Empty clipboard, ignore
            Err(e) => {
                self.overlay_manager.toast(ToastContent::warning(
                    format!("Paste failed: {}", e),
                ));
                self.needs_redraw = true;
            }
        }
    }

    /// Handle mouse wheel for scrollback navigation.
    fn handle_mouse_wheel(&mut self, delta: winit::event::MouseScrollDelta) {
        let lines = match delta {
            winit::event::MouseScrollDelta::LineDelta(_, y) => {
                // y > 0 = scroll up (into scrollback), y < 0 = scroll down (toward live)
                -(y as i32) * 3
            }
            winit::event::MouseScrollDelta::PixelDelta(pos) => {
                let cell_h = self.renderer.glyph_atlas.cell_height;
                -(pos.y as f32 / cell_h).round() as i32
            }
        };

        if lines < 0 {
            // Scroll up into scrollback
            self.scroll_offset = self.scroll_offset.saturating_add((-lines) as usize);
        } else {
            // Scroll down toward live
            self.scroll_offset = self.scroll_offset.saturating_sub(lines as usize);
        }

        self.needs_redraw = true;
    }
}

/// Convert a winit key event to bytes for the PTY.
fn key_to_bytes(event: &KeyEvent, modifiers: ModifiersState) -> Vec<u8> {
    let ctrl = modifiers.control_key();

    match &event.logical_key {
        Key::Character(c) if ctrl => {
            let ch = c.chars().next().unwrap_or('\0');
            if ch.is_ascii_alphabetic() {
                return vec![(ch.to_ascii_lowercase() as u8) & 0x1f];
            }
            vec![]
        }
        Key::Character(c) => c.as_bytes().to_vec(),
        Key::Named(named) => match named {
            NamedKey::Enter => vec![b'\r'],
            NamedKey::Backspace => vec![0x7f],
            NamedKey::Tab => vec![b'\t'],
            NamedKey::Escape => vec![0x1b],
            NamedKey::ArrowUp => b"\x1b[A".to_vec(),
            NamedKey::ArrowDown => b"\x1b[B".to_vec(),
            NamedKey::ArrowRight => b"\x1b[C".to_vec(),
            NamedKey::ArrowLeft => b"\x1b[D".to_vec(),
            NamedKey::Home => b"\x1b[H".to_vec(),
            NamedKey::End => b"\x1b[F".to_vec(),
            NamedKey::PageUp => b"\x1b[5~".to_vec(),
            NamedKey::PageDown => b"\x1b[6~".to_vec(),
            NamedKey::Insert => b"\x1b[2~".to_vec(),
            NamedKey::Delete => b"\x1b[3~".to_vec(),
            NamedKey::F1 => b"\x1bOP".to_vec(),
            NamedKey::F2 => b"\x1bOQ".to_vec(),
            NamedKey::F3 => b"\x1bOR".to_vec(),
            NamedKey::F4 => b"\x1bOS".to_vec(),
            NamedKey::F5 => b"\x1b[15~".to_vec(),
            NamedKey::F6 => b"\x1b[17~".to_vec(),
            NamedKey::F7 => b"\x1b[18~".to_vec(),
            NamedKey::F8 => b"\x1b[19~".to_vec(),
            NamedKey::F9 => b"\x1b[20~".to_vec(),
            NamedKey::F10 => b"\x1b[21~".to_vec(),
            NamedKey::F11 => b"\x1b[23~".to_vec(),
            NamedKey::F12 => b"\x1b[24~".to_vec(),
            NamedKey::Space => vec![b' '],
            _ => vec![],
        },
        _ => vec![],
    }
}
