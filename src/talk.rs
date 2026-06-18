//! Talk mode — AI agent that runs in your current terminal.
//!
//! Unlike Stratum's GPU mode, this uses only crossterm for I/O,
//! so it works from any terminal (PowerShell, CMD, bash, etc.).
//!
//! Usage: stratum --talk

use crate::ai::commands::AiCommandExecutor;
use crate::ai::natural::{NaturalLanguageAgent, ProposedCommand};
use anyhow::{Context, Result};
use crossterm::cursor;
use crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, EnableMouseCapture, DisableMouseCapture};
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crate::suggestions::SuggestionEngine;
use crate::features::inline_docs::DocOverlay;
use crossterm::terminal::{self, Clear, ClearType};
use crossterm::{execute, queue};
use serde_json::Value;
use std::io::{stdout, Write, Read};
use std::sync::{Arc, Mutex};
use std::time::Duration;

const PROMPT: &str = "\r\n\x1b[1;36m⚡ strat>\x1b[0m ";

fn apply_terminal_colors(stdout: &mut std::io::Stdout, colors: &TalkThemeColors) {
    if let crossterm::style::Color::Rgb { r, g, b } = colors.bg {
        let _ = execute!(stdout, Print(format!("\x1b]11;#{:02x}{:02x}{:02x}\x07", r, g, b)));
    } else {
        let _ = execute!(stdout, Print("\x1b]111\x07")); // Reset default background
    }
    if let crossterm::style::Color::Rgb { r, g, b } = colors.fg {
        let _ = execute!(stdout, Print(format!("\x1b]10;#{:02x}{:02x}{:02x}\x07", r, g, b)));
    } else {
        let _ = execute!(stdout, Print("\x1b]110\x07")); // Reset default foreground
    }
}

fn reset_terminal_colors(stdout: &mut std::io::Stdout) {
    let _ = execute!(stdout, Print("\x1b]110\x07\x1b]111\x07"));
}

/// Run the talk mode interactive session.
pub fn run_talk_mode(debug: bool) -> Result<()> {
    // Initialize the AI executor (loads credentials, providers)
    let mut executor = AiCommandExecutor::new();
    let mut agent = NaturalLanguageAgent::new();

    // Check if any AI provider is configured
    let has_provider = executor.registry.first_configured().is_some();

    let mut current_colors = load_talk_theme();

    // Enter raw mode
    terminal::enable_raw_mode().context("Failed to enter raw mode")?;
    let mut stdout = stdout();
    apply_terminal_colors(&mut stdout, &current_colors);
    let bg_esc = match current_colors.bg {
        crossterm::style::Color::Rgb { r, g, b } => format!("\x1b[48;2;{};{};{}m", r, g, b),
        _ => "\x1b[49m".to_string(),
    };
    let fg_esc = match current_colors.fg {
        crossterm::style::Color::Rgb { r, g, b } => format!("\x1b[38;2;{};{};{}m", r, g, b),
        _ => "\x1b[39m".to_string(),
    };
    let theme_reset = format!("\x1b[0m{}{}", bg_esc, fg_esc);

    let _ = execute!(
        stdout,
        EnableMouseCapture,
        crossterm::style::SetBackgroundColor(current_colors.bg),
        crossterm::style::SetForegroundColor(current_colors.fg),
        Clear(ClearType::All),
        cursor::MoveTo(0, 0),
    );

    // Print welcome header
    let header = format!(
        "\r\n\x1b[1;36m╭──────────────────────────────────────────╮{}\
         \r\n\x1b[1;36m│{}\x1b[1;37m  Stratum Talk v{}{}\x1b[1;36m                   │{}\
         \r\n\x1b[1;36m│{}\x1b[1;37m  Type anything — I'll figure it out     \x1b[1;36m│{}\
         \r\n\x1b[1;36m│{}\x1b[2m  Commands go to shell, English → AI agent\x1b[1;36m │{}\
         \r\n\x1b[1;36m│{}\x1b[2m  Type stheme to customize colours        \x1b[1;36m│{}\
         \r\n\x1b[1;36m│{}\x1b[2m  Type exit or Ctrl+C to quit          \x1b[1;36m│{}\
         \r\n\x1b[1;36m╰──────────────────────────────────────────╯{}",
        theme_reset,
        theme_reset,
        env!("CARGO_PKG_VERSION"),
        theme_reset,
        theme_reset,
        theme_reset,
        theme_reset,
        theme_reset,
        theme_reset,
        theme_reset,
        theme_reset,
        theme_reset,
        theme_reset,
        theme_reset,
    );
    execute!(stdout, Print(header))?;

    if !has_provider {
        execute!(
            stdout,
            Print("\r\n\x1b[1;33m⚠ No AI provider configured.\x1b[0m"),
            Print("\r\n  Configure one with: \x1b[1;37m/ai-set-key <provider> <key>\x1b[0m"),
            Print("\r\n  See providers:    \x1b[1;37m/ai-providers\x1b[0m"),
            Print("\r\n  Test connection:  \x1b[1;37m/ai-test\x1b[0m"),
        )?;
    }

    let mut input = String::new();
    let mut cursor_pos: usize = 0;
    let mut history: Vec<String> = Vec::new();
    let mut history_idx: Option<usize> = None;
    let mut ai_mode = true; // Default to AI mode on startup
    let mut needs_redraw = true;
    let mut first_draw = true;
    let mut just_cleared = false;

    let mut suggestion_engine = SuggestionEngine::new();
    let mut doc_overlay = DocOverlay::new();
    let mut last_suggest_input = String::new();
    let mut user_navigated_suggestions = false;
    let mut card_item_count = 0;
    let mut prompt_row: u16 = 0;
    let mut terminal_height: u16 = 24;
    if let Ok((_, h)) = terminal::size() {
        terminal_height = h;
    }

    // Main loop
    loop {
        if needs_redraw {
            // Update suggestions if text changed and not in AI mode
            if input != last_suggest_input {
                if !input.is_empty() && !ai_mode {
                    suggestion_engine.set_cwd(std::env::current_dir().unwrap_or_default());
                    suggestion_engine.update(&input, &doc_overlay);
                } else {
                    suggestion_engine.card.hide();
                }
                last_suggest_input = input.clone();
                user_navigated_suggestions = false;
            }

            // Set the custom colors
            let _ = execute!(
                stdout,
                crossterm::style::SetBackgroundColor(current_colors.bg),
                crossterm::style::SetForegroundColor(current_colors.fg),
            );

            if !first_draw {
                // Move cursor to start of prompt line and clear everything below
                execute!(
                    stdout,
                    cursor::MoveTo(0, prompt_row),
                    Clear(ClearType::FromCursorDown),
                )?;
            } else if just_cleared {
                just_cleared = false;
            } else {
                // For first draw of a new prompt, print a newline first
                execute!(stdout, Print("\r\n"))?;
            }
            let cwd = std::env::current_dir()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "unknown".into());

            // Custom prompt coloring matching the theme card/accent colors!
            let accent_color = if ai_mode {
                crossterm::style::Color::Rgb { r: 189, g: 147, b: 249 } // Dracula Purple fallback
            } else {
                crossterm::style::Color::Rgb { r: 136, g: 192, b: 208 } // Nord Cyan fallback
            };
            let accent_color = match current_colors.card {
                crossterm::style::Color::Reset => accent_color,
                custom => custom,
            };

            execute!(
                stdout,
                crossterm::style::SetBackgroundColor(current_colors.bg),
                crossterm::style::SetForegroundColor(accent_color),
                Print(format!("⚡ stratum [{}] @[{}]> ", if ai_mode { "ai" } else { "shell" }, cwd)),
                crossterm::style::SetForegroundColor(current_colors.fg),
                Print(&input)
            )?;

            // If it's the first draw, get the actual cursor position (prompt row)
            if first_draw {
                if let Ok((_, r)) = cursor::position() {
                    prompt_row = r;
                } else {
                    prompt_row = terminal_height.saturating_sub(1);
                }
            }

            // Move cursor left to the edit position
            let move_left = input.len() - cursor_pos;
            if move_left > 0 {
                execute!(stdout, cursor::MoveLeft(move_left as u16))?;
            }

            // Render suggestion card if visible and not in AI mode
            let card = &suggestion_engine.card;
            if card.visible && !card.items.is_empty() && !ai_mode {
                let max_vis = card.max_visible.min(card.items.len());
                card_item_count = max_vis;
                let visible_items = &card.items[card.scroll_offset..card.scroll_offset + max_vis];

                let card_border_color = match current_colors.card {
                    crossterm::style::Color::Reset => crossterm::style::Color::DarkGrey,
                    other => other,
                };

                execute!(
                    stdout,
                    crossterm::style::SetBackgroundColor(current_colors.bg),
                    crossterm::style::SetForegroundColor(card_border_color),
                    Print("\r\n  ┌── Suggestions (click to select) ─────────────────┐"),
                )?;
                for (idx, item) in visible_items.iter().enumerate() {
                    let actual_idx = card.scroll_offset + idx;
                    let is_selected = actual_idx == card.selected_index as usize;

                    let prefix = if is_selected { " ▸ " } else { "   " };
                    let (item_fg, item_bg) = if is_selected {
                        (current_colors.bg, card_border_color)
                    } else {
                        (current_colors.fg, current_colors.bg)
                    };

                    let detail_str = item.detail.as_ref().map(|d| format!(" ({})", d)).unwrap_or_default();

                    // Truncate label + detail if too long for card
                    let mut label_and_detail = format!("{} {}", item.label, detail_str);
                    if label_and_detail.len() > 42 {
                        label_and_detail.truncate(39);
                        label_and_detail.push_str("...");
                    }

                    execute!(
                        stdout,
                        crossterm::style::SetForegroundColor(card_border_color),
                        crossterm::style::SetBackgroundColor(current_colors.bg),
                        Print("\r\n  │"),
                        crossterm::style::SetForegroundColor(item_fg),
                        crossterm::style::SetBackgroundColor(item_bg),
                        Print(format!("{}{:<45}", prefix, label_and_detail)),
                        crossterm::style::SetForegroundColor(card_border_color),
                        crossterm::style::SetBackgroundColor(current_colors.bg),
                        Print("│"),
                    )?;
                }
                execute!(
                    stdout,
                    crossterm::style::SetForegroundColor(card_border_color),
                    crossterm::style::SetBackgroundColor(current_colors.bg),
                    Print("\r\n  └───────────────────────────────────────────────────┘"),
                    crossterm::style::SetForegroundColor(current_colors.fg),
                )?;
                
                // Adjust prompt_row if printing the card scrolled the terminal
                let lines_printed = max_vis + 2;
                if prompt_row + lines_printed as u16 >= terminal_height {
                    let scroll_amount = (prompt_row + lines_printed as u16) - (terminal_height.saturating_sub(1));
                    prompt_row = prompt_row.saturating_sub(scroll_amount);
                }

                // Restore cursor back to editing position absolutely
                let prompt_plain = format!("⚡ stratum [{}] @[{}]> ", if ai_mode { "ai" } else { "shell" }, cwd);
                let prompt_visual_len = prompt_plain.chars().count();
                let edit_col = prompt_visual_len + cursor_pos;
                execute!(
                    stdout,
                    cursor::MoveTo(edit_col as u16, prompt_row),
                )?;
            } else {
                card_item_count = 0;
            }

            stdout.flush()?;
            needs_redraw = false;
            first_draw = false;
        }

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                match key.code {
                    KeyCode::Enter => {
                        if suggestion_engine.card.visible && user_navigated_suggestions && !ai_mode {
                            if let Some(item) = suggestion_engine.card.selected_item() {
                                let _ = clear_card_from_screen(&mut stdout, &input, cursor_pos, card_item_count);
                                input = get_completed_input(&input, item);
                                cursor_pos = input.len();
                                suggestion_engine.card.hide();
                                user_navigated_suggestions = false;
                                needs_redraw = true;
                                continue;
                            }
                        }

                        let trimmed = input.trim().to_string();

                        // Cleanly clear suggestion card from screen before running anything
                        if suggestion_engine.card.visible && card_item_count > 0 {
                            execute!(
                                stdout,
                                Print("\r\n"),
                                Clear(ClearType::FromCursorDown),
                            )?;
                        }

                        suggestion_engine.card.hide();
                        last_suggest_input.clear();
                        card_item_count = 0;

                        // Add to history before clearing
                        if !trimmed.is_empty()
                            && (history.is_empty() || history.last().map_or(true, |h| h != &trimmed))
                        {
                            history.push(trimmed.clone());
                        }
                        history_idx = None;

                        input.clear();
                        cursor_pos = 0;

                        if trimmed.is_empty() {
                            execute!(stdout, Print("\r\n"))?;
                            first_draw = true;
                            needs_redraw = true;
                            continue;
                        }

                        // Check for theme selector commands
                        if trimmed == "stheme" || trimmed == "/stheme" {
                            let _ = run_theme_selector(&mut current_colors, &mut stdout);
                            let _ = terminal::enable_raw_mode();
                            let _ = execute!(stdout, EnableMouseCapture);
                            first_draw = true;
                            needs_redraw = true;
                            continue;
                        }

                        // Check for exit commands
                        if trimmed == "/exit" || trimmed == "exit" || trimmed == "quit" {
                            let _ = reset_terminal_colors(&mut stdout);
                            execute!(stdout, Print("\r\n\x1b[1;36mGoodbye!\x1b[0m\r\n"))?;
                            break;
                        }

                        // Check for clear screen commands (both shell & AI mode)
                        if trimmed == "clear" || trimmed == "cls" || trimmed == "/clear" {
                            execute!(
                                stdout,
                                Clear(ClearType::All),
                                Clear(ClearType::Purge),
                                cursor::MoveTo(0, 0),
                            )?;
                            first_draw = true;
                            just_cleared = true;
                            needs_redraw = true;
                            continue;
                        }

                        // Handle slash or manual commands (slash-free commands)
                        let is_ai_cmd = trimmed.starts_with('/') || {
                            let first_word = trimmed.split_whitespace().next().unwrap_or("");
                            let manual_cmds = [
                                "ask", "explain", "suggest", "translate", "ai-config", "ai", 
                                "ai-providers", "ai-set-key", "ai-models", "ai-set-provider", 
                                "ai-set-model", "ai-test", "clear-chat", "shelp"
                            ];
                            manual_cmds.contains(&first_word)
                        };

                        if is_ai_cmd {
                            handle_slash_command(&mut executor, &trimmed, &mut stdout)?;
                            let _ = terminal::enable_raw_mode();
                            let _ = execute!(stdout, EnableMouseCapture);
                            first_draw = true;
                            needs_redraw = true;
                            continue;
                        }

                        // Check if in AI mode or if natural language
                        if ai_mode || crate::ai::natural::is_natural_language(&trimmed) {
                            match executor.get_active_provider() {
                                Ok(provider) => {
                                    handle_natural_language(
                                        &mut agent, &provider, &executor, &trimmed, &mut stdout,
                                    )?;
                                    let _ = terminal::enable_raw_mode();
                                    let _ = execute!(stdout, EnableMouseCapture);
                                }
                                Err(e) => {
                                    execute!(
                                        stdout,
                                        Print(format!("\r\n\x1b[1;33m⚠ AI not configured: {}\x1b[0m", e)),
                                        Print("\r\n  Configure: \x1b[1;37mai-set-key <provider> <key>\x1b[0m"),
                                    )?;
                                }
                            }
                        } else {
                            // Execute as shell command
                            execute_shell_command(&trimmed, &mut stdout)?;
                            let _ = terminal::enable_raw_mode();
                            let _ = execute!(stdout, EnableMouseCapture);
                        }
                        first_draw = true;
                    }

                    // Ctrl+C — quit
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let _ = reset_terminal_colors(&mut stdout);
                        execute!(
                            stdout,
                            Print("\r\n\x1b[1;36mGoodbye!\x1b[0m\r\n"),
                        )?;
                        let _ = execute!(stdout, DisableMouseCapture);
                        terminal::disable_raw_mode()?;
                        execute!(stdout, ResetColor)?;
                        return Ok(());
                    }

                    // Ctrl+D — quit (EOF convention)
                    KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        let _ = reset_terminal_colors(&mut stdout);
                        execute!(
                            stdout,
                            Print("\r\n\x1b[1;36mGoodbye!\x1b[0m\r\n"),
                        )?;
                        let _ = execute!(stdout, DisableMouseCapture);
                        terminal::disable_raw_mode()?;
                        execute!(stdout, ResetColor)?;
                        return Ok(());
                    }

                    // Ctrl+U — clear line
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        input.clear();
                        cursor_pos = 0;
                        history_idx = None;
                    }

                    // Ctrl+A — beginning of line
                    KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        cursor_pos = 0;
                    }

                    // Ctrl+E — end of line
                    KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        cursor_pos = input.len();
                    }

                    // Ctrl+left — skip word left
                    KeyCode::Left if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if cursor_pos > 0 {
                            let bytes = input.as_bytes();
                            let mut pos = cursor_pos;
                            // Skip non-word chars
                            while pos > 0 && bytes[pos - 1] == b' ' {
                                pos -= 1;
                            }
                            // Skip word chars
                            while pos > 0 && bytes[pos - 1] != b' ' {
                                pos -= 1;
                            }
                            cursor_pos = pos;
                        }
                    }

                    // Ctrl+right — skip word right
                    KeyCode::Right if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        if cursor_pos < input.len() {
                            let bytes = input.as_bytes();
                            let mut pos = cursor_pos;
                            // Skip word chars
                            while pos < input.len() && bytes[pos] != b' ' {
                                pos += 1;
                            }
                            // Skip non-word chars
                            while pos < input.len() && bytes[pos] == b' ' {
                                pos += 1;
                            }
                            cursor_pos = pos;
                        }
                    }

                    KeyCode::Char(c) => {
                        input.insert(cursor_pos, c);
                        cursor_pos += 1;
                        history_idx = None;
                    }

                    KeyCode::Backspace => {
                        if cursor_pos > 0 {
                            cursor_pos -= 1;
                            input.remove(cursor_pos);
                            history_idx = None;
                        }
                    }

                    KeyCode::Delete => {
                        if cursor_pos < input.len() {
                            input.remove(cursor_pos);
                            history_idx = None;
                        }
                    }

                    KeyCode::Left => {
                        if cursor_pos > 0 {
                            cursor_pos -= 1;
                        }
                    }

                    KeyCode::Right => {
                        if cursor_pos < input.len() {
                            cursor_pos += 1;
                        }
                    }

                    KeyCode::Up => {
                        if suggestion_engine.card.visible {
                            suggestion_engine.card.select_prev();
                            user_navigated_suggestions = true;
                        } else {
                            if history.is_empty() {
                                continue;
                            }
                            let idx = match history_idx {
                                Some(i) if i > 0 => i - 1,
                                Some(_) => continue,
                                None => history.len() - 1,
                            };
                            history_idx = Some(idx);
                            input = history[idx].clone();
                            cursor_pos = input.len();
                        }
                    }

                    KeyCode::Down => {
                        if suggestion_engine.card.visible {
                            suggestion_engine.card.select_next();
                            user_navigated_suggestions = true;
                        } else {
                            if let Some(idx) = history_idx {
                                if idx + 1 < history.len() {
                                    history_idx = Some(idx + 1);
                                    input = history[idx + 1].clone();
                                } else {
                                    history_idx = None;
                                    input.clear();
                                }
                                cursor_pos = input.len();
                            }
                        }
                    }

                    KeyCode::Home => {
                        cursor_pos = 0;
                    }

                    KeyCode::End => {
                        cursor_pos = input.len();
                    }

                    KeyCode::Esc => {
                        if suggestion_engine.card.visible {
                            suggestion_engine.card.hide();
                            user_navigated_suggestions = false;
                        }
                    }

                    // Tab — Accept suggestion if visible, otherwise Toggle Shell vs AI mode
                    KeyCode::Tab => {
                        if suggestion_engine.card.visible && !ai_mode {
                            if let Some(item) = suggestion_engine.card.selected_item() {
                                // Clear card from screen before replacing input
                                let _ = clear_card_from_screen(&mut stdout, &input, cursor_pos, card_item_count);
                                input = get_completed_input(&input, item);
                                cursor_pos = input.len();
                                suggestion_engine.card.hide();
                            }
                        } else {
                            ai_mode = !ai_mode;
                            suggestion_engine.card.hide();
                            let mode_str = if ai_mode { "\x1b[1;35mAI Agent\x1b[0m" } else { "\x1b[1;36mShell\x1b[0m" };
                            execute!(
                                stdout,
                                Print(format!("\r\n\x1b[2m  Mode switched to {}\x1b[0m", mode_str)),
                            )?;
                            first_draw = true;
                        }
                    }

                    _ => {}
                }

                needs_redraw = true;
            }
            Event::Mouse(mouse_event) => {
                if mouse_event.kind == crossterm::event::MouseEventKind::Down(crossterm::event::MouseButton::Left) {
                    let _click_col = mouse_event.column;
                    let click_row = mouse_event.row;

                    if suggestion_engine.card.visible && card_item_count > 0 && !ai_mode {
                        let card_start = prompt_row + 2; // Border is at prompt_row + 1, first item is at prompt_row + 2
                        let card_item_count_u16 = card_item_count as u16;
                        if click_row >= card_start && click_row < card_start + card_item_count_u16 {
                            let item_idx = (click_row - card_start) as usize;
                            let card = &suggestion_engine.card;
                            if card.scroll_offset + item_idx < card.items.len() {
                                let item = &card.items[card.scroll_offset + item_idx];
                                // Clear card from screen before replacing input
                                let _ = clear_card_from_screen(&mut stdout, &input, cursor_pos, card_item_count);
                                input = get_completed_input(&input, item);
                                cursor_pos = input.len();
                                suggestion_engine.card.hide();
                                needs_redraw = true;
                            }
                        }
                    }
                }
            }
            Event::Resize(_, h) => {
                terminal_height = h;
                first_draw = true;
                needs_redraw = true;
            }
            _ => {}
        }
    }

    let _ = execute!(stdout, DisableMouseCapture);
    terminal::disable_raw_mode()?;
    execute!(stdout, ResetColor)?;
    Ok(())
}

/// Handle a slash command (/ask, /ai-set-key, etc.)
fn handle_slash_command(
    executor: &mut AiCommandExecutor,
    command: &str,
    stdout: &mut std::io::Stdout,
) -> Result<()> {
    use crate::ai::commands::AiCommand;

    if let Some(ai_cmd) = AiCommand::parse(command) {
        // Build a tokio runtime for async execution
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        execute!(stdout, Print("\r\n\x1b[2m⏳ Thinking...\x1b[0m"))?;
        stdout.flush()?;

        match rt.block_on(executor.execute(&ai_cmd)) {
            Ok(output) => {
                execute!(
                    stdout,
                    cursor::MoveToPreviousLine(1),
                    Clear(ClearType::FromCursorDown),
                    Print(format!("\r\n{}", output)),
                )?;
            }
            Err(e) => {
                execute!(
                    stdout,
                    cursor::MoveToPreviousLine(1),
                    Clear(ClearType::FromCursorDown),
                    Print(format!("\r\n\x1b[1;31m✗ {}\x1b[0m", e)),
                )?;
            }
        }
    } else {
        execute!(
            stdout,
            Print(format!("\r\n\x1b[1;33mUnknown slash command: {}\x1b[0m", command)),
            Print("\r\n  Try: shelp, ask, explain, suggest, ai-set-key, ai-providers, ai-test"),
        )?;
    }

    Ok(())
}

/// Handle a natural language request via the AI agent.
fn handle_natural_language(
    agent: &mut NaturalLanguageAgent,
    provider: &crate::ai::provider::AiProvider,
    executor: &AiCommandExecutor,
    initial_input: &str,
    stdout: &mut std::io::Stdout,
) -> Result<()> {
    let theme_colors = load_talk_theme();
    let mut current_input = initial_input.to_string();
    let mut executed_commands_log: Vec<String> = Vec::new();
    let mut last_output = String::new();
    let mut is_follow_up = false;

    // Clear conversation history for a fresh interaction
    agent.chat_engine.clear();

    loop {
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".into());

        print_themed(stdout, "\r\n\x1b[2m⏳ AI Agent thinking (Press ESC or Ctrl+C to abort)...\x1b[0m", &theme_colors)?;
        stdout.flush()?;

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;

        let result = match rt.block_on(async {
            tokio::select! {
                res = agent.process(&current_input, &cwd, provider, is_follow_up) => {
                    res.map(Some)
                }
                _ = async {
                    loop {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        if let Ok(true) = event::poll(Duration::from_millis(0)) {
                            if let Ok(Event::Key(key)) = event::read() {
                                if key.code == KeyCode::Esc || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)) {
                                    return;
                                }
                            }
                        }
                    }
                } => {
                    Ok(None)
                }
            }
        }) {
            Ok(Some(res)) => res,
            Ok(None) => {
                execute!(
                    stdout,
                    cursor::MoveToPreviousLine(1),
                    Clear(ClearType::FromCursorDown),
                )?;
                print_themed(stdout, "\r\n\x1b[1;31m✗ AI thinking cancelled by user\x1b[0m\r\n", &theme_colors)?;
                break;
            }
            Err(e) => {
                execute!(
                    stdout,
                    cursor::MoveToPreviousLine(1),
                    Clear(ClearType::FromCursorDown),
                )?;
                print_themed(stdout, &format!("\r\n\x1b[1;31m✗ AI Error: {}\x1b[0m\r\n", e), &theme_colors)?;
                break;
            }
        };

        // Clear "Thinking..." line
        execute!(
            stdout,
            cursor::MoveToPreviousLine(1),
            Clear(ClearType::FromCursorDown),
        )?;

        // Show explanation
        if !result.explanation.is_empty() {
            print_themed(stdout, &format!("\r\n\x1b[1;37m  {}\x1b[0m", result.explanation), &theme_colors)?;
        }

        if result.proposed_commands.is_empty() {
            print_themed(stdout, "\r\n", &theme_colors)?;
            // If we executed commands in this session, save learnings
            if !executed_commands_log.is_empty() {
                let mut memory = agent.memory.clone();
                let _ = rt.block_on(memory.extract_learnings(provider, initial_input, &executed_commands_log, &last_output));
                agent.memory = memory;
                agent.memory.add_interaction(cwd, initial_input.to_string(), executed_commands_log);
            }
            break;
        }

        let total = result.proposed_commands.len();
        let mut next_system_input = String::new();
        let mut aborted = false;

        // Execute commands one by one with confirmation
        for (i, cmd) in result.proposed_commands.iter().enumerate() {
            let cmd_trimmed = cmd.command.trim();
            let is_web_cmd = cmd_trimmed.starts_with("websearch ")
                || cmd_trimmed.starts_with("web-search ")
                || cmd_trimmed.starts_with("webread ")
                || cmd_trimmed.starts_with("web-read ");

            let confirmed = if is_web_cmd {
                let display_msg = if cmd_trimmed.starts_with("webread ") || cmd_trimmed.starts_with("web-read ") {
                    format!("\r\n\x1b[1;36m📖 Reading web page: {}\x1b[0m", cmd_trimmed)
                } else {
                    format!("\r\n\x1b[1;36m🔍 Searching web: {}\x1b[0m", cmd_trimmed)
                };
                print_themed(stdout, &display_msg, &theme_colors)?;
                stdout.flush()?;
                true
            } else {
                let is_dangerous = agent.check_dangerous(&cmd.command);
                let danger_msg = if is_dangerous {
                    agent.danger_reason(&cmd.command)
                } else {
                    String::new()
                };

                let confirm_prompt = if is_dangerous {
                    format!(
                        "\r\n\x1b[1;33m⚠ Step {}/{} — {}\x1b[0m\
                         \r\n  \x1b[1;37m${}\x1b[0m\
                         \r\n  \x1b[1;31m⚠ ⚠ ⚠ {}\x1b[0m\
                         \r\n  \x1b[1;33m[Enter] execute  [Escape] skip/cancel\x1b[0m ",
                        i + 1,
                        total,
                        cmd.description,
                        cmd.command,
                        danger_msg,
                    )
                } else {
                    format!(
                        "\r\n\x1b[1;36m  Step {}/{} — {}\x1b[0m\
                         \r\n  \x1b[1;37m  ${}\x1b[0m\
                         \r\n  \x1b[2m  [Enter] execute  [Escape] skip/cancel\x1b[0m ",
                        i + 1,
                        total,
                        cmd.description,
                        cmd.command,
                    )
                };

                print_themed(stdout, &confirm_prompt, &theme_colors)?;
                stdout.flush()?;

                match get_confirmation()? {
                    Confirmation::Execute => true,
                    Confirmation::Skip | Confirmation::ExecuteAll => {
                        print_themed(stdout, "\r\n  \x1b[2mSkipped remaining steps\x1b[0m", &theme_colors)?;
                        next_system_input.push_str(&format!(
                            "[Command '{}' skipped/aborted by user]\n",
                            cmd.command
                        ));
                        aborted = true;
                        false
                    }
                }
            };

            if aborted {
                break;
            }

            if confirmed {
                print_themed(stdout, "\r\n\x1b[2m  Executing...\x1b[0m", &theme_colors)?;
                stdout.flush()?;

                // Load active theme/colors to print output correctly
                let output_color = match theme_colors.out {
                    crossterm::style::Color::Reset => theme_colors.fg,
                    other => other,
                };
                let _ = execute!(
                    stdout,
                    crossterm::style::SetBackgroundColor(theme_colors.bg),
                    crossterm::style::SetForegroundColor(output_color),
                );

                match execute_command(&cmd.command) {
                    Ok((output, streamed)) => {
                        executed_commands_log.push(cmd.command.clone());
                        last_output = output.clone();

                        let is_web_cmd = {
                            let cmd_trimmed = cmd.command.trim();
                            cmd_trimmed.starts_with("websearch ")
                                || cmd_trimmed.starts_with("web-search ")
                                || cmd_trimmed.starts_with("webread ")
                                || cmd_trimmed.starts_with("web-read ")
                        };

                        if is_web_cmd {
                            let cmd_trimmed = cmd.command.trim();
                            if cmd_trimmed.starts_with("websearch ") || cmd_trimmed.starts_with("web-search ") {
                                let count = output.split("\n---\n\n").filter(|s| !s.trim().is_empty()).count();
                                print_themed(stdout, &format!("\r\n  \x1b[1;32m✓ Search completed ({} results found)\x1b[0m", count), &theme_colors)?;
                            } else {
                                print_themed(stdout, &format!("\r\n  \x1b[1;32m✓ Page read completed ({} characters loaded)\x1b[0m", output.len()), &theme_colors)?;
                            }
                            let _ = execute!(
                                stdout,
                                crossterm::style::SetBackgroundColor(theme_colors.bg),
                                crossterm::style::SetForegroundColor(theme_colors.fg),
                            );
                        } else {
                            let formatted_output = if let Some(table) = format_as_table(&output) {
                                Some(table)
                            } else if !streamed {
                                Some(output.clone())
                            } else {
                                None
                            };

                            if let Some(fmt_out) = formatted_output {
                                let lines: Vec<&str> = fmt_out.lines().collect();
                                let show_lines = if lines.len() > 20 {
                                    &lines[..20]
                                } else {
                                    &lines[..]
                                };
                                for line in show_lines {
                                    print_themed(stdout, &format!("\r\n  \x1b[37m{}\x1b[0m", line), &theme_colors)?;
                                }
                                if lines.len() > 20 {
                                    print_themed(stdout, &format!("\r\n  \x1b[37m... {} more lines\x1b[0m", lines.len() - 20), &theme_colors)?;
                                }
                            }
                            
                            // Restore colors back to standard text input colors
                            let _ = execute!(
                                stdout,
                                crossterm::style::SetBackgroundColor(theme_colors.bg),
                                crossterm::style::SetForegroundColor(theme_colors.fg),
                            );
                            print_themed(stdout, "\r\n  \x1b[1;32m✓ Done\x1b[0m", &theme_colors)?;
                        }

                        next_system_input.push_str(&format!(
                            "[Command '{}' succeeded with output:\n{}]\n",
                            cmd.command, output
                        ));
                    }
                    Err((e, streamed)) => {
                        executed_commands_log.push(cmd.command.clone());
                        last_output = e.clone();

                        if e == "Command cancelled by user" {
                            let _ = execute!(
                                stdout,
                                crossterm::style::SetBackgroundColor(theme_colors.bg),
                                crossterm::style::SetForegroundColor(theme_colors.fg),
                            );
                            print_themed(stdout, "\r\n  \x1b[1;31m✗ Command cancelled by user\x1b[0m\r\n", &theme_colors)?;
                            aborted = true;
                            break;
                        }

                        if !streamed {
                            print_themed(stdout, &format!("\r\n  \x1b[1;31m✗ {}\x1b[0m", e), &theme_colors)?;
                        } else {
                            print_themed(stdout, "\r\n  \x1b[1;31m✗ Command failed\x1b[0m", &theme_colors)?;
                        }
                        
                        // Restore colors back to standard text input colors
                        let _ = execute!(
                            stdout,
                            crossterm::style::SetBackgroundColor(theme_colors.bg),
                            crossterm::style::SetForegroundColor(theme_colors.fg),
                        );

                        next_system_input.push_str(&format!(
                            "[Command '{}' failed with output:\n{}]\n",
                            cmd.command, e
                        ));
                    }
                }
            }
        }

        if aborted || next_system_input.is_empty() {
            // Save what we completed
            if !executed_commands_log.is_empty() {
                let mut memory = agent.memory.clone();
                let _ = rt.block_on(memory.extract_learnings(provider, initial_input, &executed_commands_log, &last_output));
                agent.memory = memory;
                agent.memory.add_interaction(cwd, initial_input.to_string(), executed_commands_log);
            }
            break;
        }

        current_input = next_system_input;
        is_follow_up = true;
    }

    print_themed(stdout, "\r\n", &theme_colors)?;
    Ok(())
}

/// User's confirmation choice.
enum Confirmation {
    Execute,
    Skip,
    ExecuteAll,
}

/// Wait for user to press Enter, Escape, or Tab.
fn get_confirmation() -> Result<Confirmation> {
    loop {
        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                match key.code {
                    KeyCode::Enter => return Ok(Confirmation::Execute),
                    KeyCode::Esc => return Ok(Confirmation::Skip),
                    KeyCode::Tab => return Ok(Confirmation::ExecuteAll),
                    _ => {} // Ignore other keys
                }
            }
            _ => {}
        }
    }
}

/// Execute a single shell command via std::process::Command and return output.
/// Execute a single shell command via std::process::Command and return output and whether it was streamed in real-time.
fn execute_command(command: &str) -> Result<(String, bool), (String, bool)> {
    let trimmed = command.trim();
    
    // Intercept built-in web search
    if trimmed.starts_with("websearch ") || trimmed.starts_with("web-search ") {
        let query = if trimmed.starts_with("websearch ") {
            &trimmed["websearch ".len()..]
        } else {
            &trimmed["web-search ".len()..]
        };
        let query = query.trim().trim_matches(|c| c == '\'' || c == '"');
        
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build() {
                Ok(r) => r,
                Err(e) => return Err((e.to_string(), false)),
            };
        
        let result = rt.block_on(async {
            tokio::select! {
                res = web_search_helper_async(query) => {
                    res.map(Some)
                }
                _ = async {
                    loop {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        if let Ok(true) = event::poll(Duration::from_millis(0)) {
                            if let Ok(Event::Key(key)) = event::read() {
                                if key.code == KeyCode::Esc || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)) {
                                    return;
                                }
                            }
                        }
                    }
                } => {
                    Ok(None)
                }
            }
        });

        match result {
            Ok(Some(res)) => return Ok((res, false)),
            Ok(None) => return Err(("Command cancelled by user".to_string(), false)),
            Err(e) => return Err((format!("Web search failed: {}", e), false)),
        }
    }
    
    // Intercept built-in web read
    if trimmed.starts_with("webread ") || trimmed.starts_with("web-read ") {
        let url = if trimmed.starts_with("webread ") {
            &trimmed["webread ".len()..]
        } else {
            &trimmed["web-read ".len()..]
        };
        let url = url.trim().trim_matches(|c| c == '\'' || c == '"');
        
        let rt = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build() {
                Ok(r) => r,
                Err(e) => return Err((e.to_string(), false)),
            };
            
        let result = rt.block_on(async {
            tokio::select! {
                res = web_read_helper_async(url) => {
                    res.map(Some)
                }
                _ = async {
                    loop {
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        if let Ok(true) = event::poll(Duration::from_millis(0)) {
                            if let Ok(Event::Key(key)) = event::read() {
                                if key.code == KeyCode::Esc || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)) {
                                    return;
                                }
                            }
                        }
                    }
                } => {
                    Ok(None)
                }
            }
        });

        match result {
            Ok(Some(res)) => return Ok((res, false)),
            Ok(None) => return Err(("Command cancelled by user".to_string(), false)),
            Err(e) => return Err((format!("Web read failed: {}", e), false)),
        }
    }

    if trimmed == "cd" {
        let home = if cfg!(windows) {
            std::env::var("USERPROFILE").ok()
        } else {
            std::env::var("HOME").ok()
        };
        if let Some(home_path) = home {
            if let Err(e) = std::env::set_current_dir(&home_path) {
                return Err((format!("cd failed: {}", e), false));
            }
            return Ok((format!("Changed directory to {}", home_path), false));
        } else {
            return Err(("Could not find home directory".to_string(), false));
        }
    } else if trimmed.starts_with("cd ") {
        let path = trimmed["cd ".len()..].trim();
        let path = path.trim_matches(|c| c == '\'' || c == '"');
        if let Err(e) = std::env::set_current_dir(path) {
            return Err((format!("cd failed: {}", e), false));
        }
        let new_cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| path.to_string());
        return Ok((format!("Changed directory to {}", new_cwd), false));
    }

    let mut child = if cfg!(windows) {
        match std::process::Command::new("cmd")
            .args(["/C", command])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn() {
                Ok(c) => c,
                Err(e) => return Err((format!("Failed to execute command: {}", e), false)),
            }
    } else {
        match std::process::Command::new("sh")
            .args(["-c", command])
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped())
            .spawn() {
                Ok(c) => c,
                Err(e) => return Err((format!("Failed to execute command: {}", e), false)),
            }
    };

    let _ = execute!(
        std::io::stdout(),
        Print("\r\n\x1b[2m[Press ESC or Ctrl+C to abort command]\x1b[0m\r\n")
    );
    let _ = std::io::stdout().flush();

    let stdout_buf = Arc::new(Mutex::new(Vec::new()));
    let stdout_buf_clone = stdout_buf.clone();
    let mut child_stdout = child.stdout.take().unwrap();
    std::thread::spawn(move || {
        let mut buffer = [0; 512];
        while let Ok(n) = child_stdout.read(&mut buffer) {
            if n == 0 { break; }
            if let Ok(mut lock) = stdout_buf_clone.lock() {
                lock.extend_from_slice(&buffer[..n]);
            }
            let text = String::from_utf8_lossy(&buffer[..n]).replace('\n', "\r\n");
            let mut out = std::io::stdout();
            let _ = out.write_all(text.as_bytes());
            let _ = out.flush();
        }
    });

    let stderr_buf = Arc::new(Mutex::new(Vec::new()));
    let stderr_buf_clone = stderr_buf.clone();
    let mut child_stderr = child.stderr.take().unwrap();
    std::thread::spawn(move || {
        let mut buffer = [0; 512];
        while let Ok(n) = child_stderr.read(&mut buffer) {
            if n == 0 { break; }
            if let Ok(mut lock) = stderr_buf_clone.lock() {
                lock.extend_from_slice(&buffer[..n]);
            }
            let text = String::from_utf8_lossy(&buffer[..n]).replace('\n', "\r\n");
            let mut out = std::io::stdout();
            let _ = out.write_all(text.as_bytes());
            let _ = out.flush();
        }
    });

    let mut aborted = false;
    loop {
        match child.try_wait() {
            Ok(Some(_status)) => {
                break;
            }
            Ok(None) => {}
            Err(e) => {
                let _ = terminal::enable_raw_mode();
                let _ = execute!(std::io::stdout(), EnableMouseCapture);
                return Err((format!("Error waiting for command: {}", e), true));
            }
        }

        if let Ok(true) = event::poll(Duration::from_millis(50)) {
            if let Ok(Event::Key(key)) = event::read() {
                if key.code == KeyCode::Esc || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL)) {
                    let _ = child.kill();
                    aborted = true;
                    break;
                }
            }
        }
    }

    let _ = terminal::enable_raw_mode();
    let _ = execute!(std::io::stdout(), EnableMouseCapture);

    if aborted {
        return Err(("Command cancelled by user".to_string(), true));
    }

    // Wait a brief moment for pipes to finish flushing
    std::thread::sleep(Duration::from_millis(50));

    let stdout_bytes = stdout_buf.lock().map(|l| l.clone()).unwrap_or_default();
    let stderr_bytes = stderr_buf.lock().map(|l| l.clone()).unwrap_or_default();
    let mut result = String::new();
    if !stdout_bytes.is_empty() {
        result.push_str(&String::from_utf8_lossy(&stdout_bytes));
    }
    if !stderr_bytes.is_empty() {
        if !result.is_empty() {
            result.push('\n');
        }
        result.push_str(&String::from_utf8_lossy(&stderr_bytes));
    }

    let exit_success = match child.wait() {
        Ok(s) => s.success(),
        Err(_) => false,
    };

    if !exit_success {
        let err_msg = if result.is_empty() {
            "Command failed with non-zero exit code".to_string()
        } else {
            result.trim().to_string()
        };
        return Err((err_msg, true));
    }

    Ok((result.trim().to_string(), true))
}

/// Execute a command and print output to the terminal.
fn execute_shell_command(command: &str, stdout: &mut std::io::Stdout) -> Result<()> {
    match execute_command(command) {
        Ok((output, streamed)) => {
            if output.is_empty() {
                execute!(stdout, Print("\r\n"))?;
            } else {
                if let Some(table) = format_as_table(&output) {
                    for line in table.lines() {
                        execute!(stdout, Print(format!("\r\n  \x1b[37m{}\x1b[0m", line)))?;
                    }
                } else if !streamed {
                    for line in output.lines() {
                        execute!(stdout, Print(format!("\r\n  \x1b[37m{}\x1b[0m", line)))?;
                    }
                }
                execute!(stdout, Print("\r\n"))?;
            }
        }
        Err((e, streamed)) => {
            if e == "Command cancelled by user" {
                execute!(
                    stdout,
                    Print("\r\n\x1b[1;31m✗ Command cancelled by user\x1b[0m\r\n"),
                )?;
            } else if streamed {
                execute!(
                    stdout,
                    Print("\r\n\x1b[1;31m✗ Command failed\x1b[0m\r\n"),
                )?;
            } else {
                execute!(
                    stdout,
                    Print(format!("\r\n\x1b[1;31m✗ {}\x1b[0m\r\n", e)),
                )?;
            }
        }
    }
    Ok(())
}

/// Formats structured output as an ASCII table if possible.
fn format_as_table(output: &str) -> Option<String> {
    let trimmed = output.trim();
    if trimmed.is_empty() {
        return None;
    }

    // Try parsing as JSON first
    if (trimmed.starts_with('[') && trimmed.ends_with(']')) || (trimmed.starts_with('{') && trimmed.ends_with('}')) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(trimmed) {
            match value {
                serde_json::Value::Array(arr) => {
                    if arr.is_empty() {
                        return None;
                    }
                    // Case 1: Array of objects
                    if arr.iter().all(|v| v.is_object()) {
                        let mut keys = std::collections::BTreeSet::new();
                        for item in &arr {
                            if let Some(obj) = item.as_object() {
                                for k in obj.keys() {
                                    keys.insert(k.clone());
                                }
                            }
                        }
                        let headers: Vec<String> = keys.into_iter().collect();
                        if headers.is_empty() {
                            return None;
                        }
                        let mut rows = Vec::new();
                        for item in &arr {
                            if let Some(obj) = item.as_object() {
                                let row: Vec<String> = headers
                                    .iter()
                                    .map(|h| {
                                        obj.get(h)
                                            .map(|v| match v {
                                                serde_json::Value::String(s) => s.clone(),
                                                _ => v.to_string(),
                                            })
                                            .unwrap_or_default()
                                    })
                                    .collect();
                                rows.push(row);
                            }
                        }
                        return Some(render_ascii_table(&headers, &rows));
                    }
                    // Case 2: Array of arrays (first one is headers)
                    if arr.iter().all(|v| v.is_array()) {
                        let mut headers = Vec::new();
                        let mut rows = Vec::new();
                        for (i, item) in arr.iter().enumerate() {
                            if let Some(row_arr) = item.as_array() {
                                let row_vals: Vec<String> = row_arr
                                    .iter()
                                    .map(|v| match v {
                                        serde_json::Value::String(s) => s.clone(),
                                        _ => v.to_string(),
                                    })
                                    .collect();
                                if i == 0 {
                                    headers = row_vals;
                                } else {
                                    rows.push(row_vals);
                                }
                            }
                        }
                        if !headers.is_empty() {
                            return Some(render_ascii_table(&headers, &rows));
                        }
                    }
                }
                serde_json::Value::Object(obj) => {
                    // Single object -> render as Key-Value table
                    let headers = vec!["Key".to_string(), "Value".to_string()];
                    let mut rows = Vec::new();
                    for (k, v) in obj {
                        let val_str = match v {
                            serde_json::Value::String(s) => s.clone(),
                            _ => v.to_string(),
                        };
                        rows.push(vec![k, val_str]);
                    }
                    return Some(render_ascii_table(&headers, &rows));
                }
                _ => {}
            }
        }
    }

    // Fallback: Check if the raw text is space-aligned or CSV-like
    let lines: Vec<&str> = trimmed.lines().map(|l| l.trim_end()).filter(|l| !l.is_empty()).collect();
    if lines.len() >= 2 {
        let (headers, rows) = split_columns(&lines);
        if !headers.is_empty() && !rows.is_empty() {
            return Some(render_ascii_table(&headers, &rows));
        }
    }

    None
}

/// Splits space-aligned, CSV, or TSV lines into columns.
fn split_columns(lines: &[&str]) -> (Vec<String>, Vec<Vec<String>>) {
    if lines.is_empty() {
        return (Vec::new(), Vec::new());
    }

    let first = lines[0];
    let delimiter = if first.contains('\t') {
        Some('\t')
    } else if first.contains("  ") {
        None
    } else if first.contains(',') {
        Some(',')
    } else {
        None
    };

    let mut headers;
    let mut rows = Vec::new();

    if let Some(delim) = delimiter {
        headers = first.split(delim).map(|s| s.trim().to_string()).collect();
        for line in &lines[1..] {
            let row: Vec<String> = line.split(delim).map(|s| s.trim().to_string()).collect();
            rows.push(row);
        }
    } else {
        // Split by 2+ spaces
        let re_split = |s: &str| {
            let mut parts = Vec::new();
            let mut current = String::new();
            let mut space_count = 0;
            for c in s.chars() {
                if c == ' ' {
                    space_count += 1;
                    if space_count >= 2 {
                        let trimmed = current.trim();
                        if !trimmed.is_empty() {
                            parts.push(trimmed.to_string());
                            current.clear();
                        }
                    } else {
                        current.push(c);
                    }
                } else {
                    current.push(c);
                    space_count = 0;
                }
            }
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                parts.push(trimmed.to_string());
            }
            parts
        };

        headers = re_split(first);
        if headers.len() > 1 {
            for line in &lines[1..] {
                let row = re_split(line);
                rows.push(row);
            }
        } else {
            headers.clear();
        }
    }

    (headers, rows)
}

/// Renders headers and rows into a clean ASCII table.
fn render_ascii_table(headers: &[String], rows: &[Vec<String>]) -> String {
    if headers.is_empty() {
        return String::new();
    }

    let num_cols = headers.len();
    let mut widths = vec![0; num_cols];

    for (i, h) in headers.iter().enumerate() {
        widths[i] = h.len();
    }

    for row in rows {
        for (i, cell) in row.iter().enumerate() {
            if i < num_cols {
                widths[i] = widths[i].max(cell.len());
            }
        }
    }

    let mut border = String::new();
    border.push('+');
    for w in &widths {
        border.push_str(&"-".repeat(*w + 2));
        border.push('+');
    }

    let mut result = String::new();
    result.push_str(&border);
    result.push_str("\r\n");

    result.push('|');
    for (i, h) in headers.iter().enumerate() {
        result.push_str(&format!(" {:<width$} |", h, width = widths[i]));
    }
    result.push_str("\r\n");

    result.push_str(&border);
    result.push_str("\r\n");

    for row in rows {
        result.push('|');
        for i in 0..num_cols {
            let val = row.get(i).cloned().unwrap_or_default();
            result.push_str(&format!(" {:<width$} |", val, width = widths[i]));
        }
        result.push_str("\r\n");
    }

    result.push_str(&border);
    result
}

/// URL-encode a string query for web request query parameter.
fn url_encode(s: &str) -> String {
    let mut encoded = String::new();
    for b in s.bytes() {
        match b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(b as char);
            }
            b' ' => {
                encoded.push('+');
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", b));
            }
        }
    }
    encoded
}

/// URL-decode a string value.
fn url_decode(s: &str) -> String {
    let mut bytes = Vec::new();
    let mut s_bytes = s.bytes();
    while let Some(b) = s_bytes.next() {
        if b == b'%' {
            let mut hex = String::new();
            if let Some(h1) = s_bytes.next() { hex.push(h1 as char); }
            if let Some(h2) = s_bytes.next() { hex.push(h2 as char); }
            if let Ok(val) = u8::from_str_radix(&hex, 16) {
                bytes.push(val);
            }
        } else if b == b'+' {
            bytes.push(b' ');
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Helper function to strip HTML tags from raw web page source.
fn strip_html_tags(html: &str) -> String {
    let mut clean = String::new();
    let mut in_tag = false;
    let mut chars = html.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '<' {
            in_tag = true;
        } else if c == '>' {
            in_tag = false;
        } else if !in_tag {
            clean.push(c);
        }
    }
    clean
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#x27;", "'")
        .replace("&#39;", "'")
}

async fn fetch_url_with_curl_async(url: &str) -> Result<String, anyhow::Error> {
    let output = tokio::process::Command::new("curl")
        .arg("-sL")
        .arg("--compressed")
        .arg("-H")
        .arg("User-Agent: Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36")
        .arg(url)
        .output()
        .await?;
        
    if !output.status.success() {
        return Err(anyhow::anyhow!("curl failed with status: {:?}", output.status));
    }
    
    let html = String::from_utf8_lossy(&output.stdout).into_owned();
    Ok(html)
}

/// Helper to query Ecosia search as a fallback when DuckDuckGo is blocked.
async fn ecosia_search_helper_async(query: &str) -> Result<String, anyhow::Error> {
    let url = format!("https://www.ecosia.org/search?q={}", url_encode(query));
    let response = fetch_url_with_curl_async(&url).await?;
    
    let mut results = Vec::new();
    
    // Attempt 1: Parse server-rendered HTML (modern layout)
    let mut cursor = 0;
    while let Some(start_idx) = response[cursor..].find("data-test-id=\"organic-result\"") {
        let absolute_start = cursor + start_idx;
        cursor = absolute_start + "data-test-id=\"organic-result\"".len();
        
        let end_idx = response.len().min(absolute_start + 8000);
        let article_chunk = &response[absolute_start..end_idx];
        
        let mut href = String::new();
        if let Some(href_pos) = article_chunk.find("data-test-id=\"result-link\"") {
            let chunk_after_link = &article_chunk[href_pos..];
            if let Some(h_start) = chunk_after_link.find("href=\"") {
                let h_val_start = href_pos + h_start + 6;
                if let Some(h_end) = article_chunk[h_val_start..].find("\"") {
                    href = article_chunk[h_val_start..h_val_start + h_end].to_string();
                }
            }
        }
        
        let mut title = String::new();
        if let Some(title_pos) = article_chunk.find("data-test-id=\"result-title\"") {
            let chunk_after_title = &article_chunk[title_pos..];
            if let Some(t_start) = chunk_after_title.find(">") {
                let t_val_start = title_pos + t_start + 1;
                if let Some(t_end) = article_chunk[t_val_start..].find("</h2>") {
                    title = strip_html_tags(&article_chunk[t_val_start..t_val_start + t_end]).trim().to_string();
                }
            }
        }
        
        let mut snippet = String::new();
        if let Some(snip_pos) = article_chunk.find("data-test-id=\"web-result-description\"") {
            let chunk_after_snip = &article_chunk[snip_pos..];
            if let Some(s_start) = chunk_after_snip.find(">") {
                let s_val_start = snip_pos + s_start + 1;
                if let Some(s_end) = article_chunk[s_val_start..].find("</p>") {
                    snippet = strip_html_tags(&article_chunk[s_val_start..s_val_start + s_end]).trim().to_string();
                }
            }
        }
        
        if !title.is_empty() && !href.is_empty() {
            results.push(format!("Title: {}\nLink: {}\nSnippet: {}\n", title, href, snippet));
        }
        
        if results.len() >= 8 {
            break;
        }
    }
    
    // Attempt 2: Fallback to pageContextInit JSON (legacy layout)
    if results.is_empty() {
        if let Some(start_idx) = response.find("window.__pageContextInit = ") {
            let json_start = start_idx + "window.__pageContextInit = ".len();
            if let Some(end_idx) = response[json_start..].find("</script>") {
                let json_str = response[json_start..json_start + end_idx].trim().trim_end_matches(';');
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(json_str) {
                    if let Some(items) = val.pointer("/data/results/results").and_then(|v| v.as_array()) {
                        for item in items {
                            if item.get("type").and_then(|v| v.as_str()) == Some("result") {
                                let title = item.get("title").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let url = item.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let description = item.get("description").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                if !title.is_empty() && !url.is_empty() {
                                    results.push(format!("Title: {}\nLink: {}\nSnippet: {}\n", title, url, description));
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    
    if results.is_empty() {
        #[cfg(test)]
        {
            let preview = if response.len() > 1000 { &response[..1000] } else { &response };
            println!("Ecosia debug: HTML length is {}, start of HTML: {}", response.len(), preview);
        }
        Ok("No results found.".to_string())
    } else {
        Ok(results.join("\n---\n\n"))
    }
}


/// Async helper to query DuckDuckGo search and extract links and snippets.
async fn web_search_ddg_helper_async(query: &str) -> Result<String, anyhow::Error> {
    let url = format!("https://html.duckduckgo.com/html/?q={}", url_encode(query));
    let response = fetch_url_with_curl_async(&url).await?;
    
    let mut results = Vec::new();
    let mut cursor = 0;
    
    while let Some(start_idx) = response[cursor..].find("class=\"result__body\"") {
        let absolute_start = cursor + start_idx;
        cursor = absolute_start + "class=\"result__body\"".len();
        
        let mut end_idx = response.len().min(absolute_start + 4000);
        while end_idx > absolute_start && !response.is_char_boundary(end_idx) {
            end_idx -= 1;
        }
        let link_search = &response[absolute_start..end_idx];
        if let Some(a_start) = link_search.find("class=\"result__a\"") {
            let abs_a_start = absolute_start + a_start;
            if let Some(href_start) = response[abs_a_start..].find("href=\"") {
                let abs_href_start = abs_a_start + href_start + 6;
                if let Some(href_end) = response[abs_href_start..].find("\"") {
                    let mut href = response[abs_href_start..abs_href_start + href_end].to_string();
                    if href.contains("uddg=") {
                        if let Some(uddg_idx) = href.find("uddg=") {
                            let uddg_val = &href[uddg_idx + 5..];
                            let end_val = uddg_val.find('&').unwrap_or(uddg_val.len());
                            let decoded = url_decode(&uddg_val[..end_val]);
                            if !decoded.is_empty() {
                                href = decoded;
                            }
                        }
                    }

                    if let Some(title_start) = response[abs_a_start..].find(">") {
                        let abs_title_start = abs_a_start + title_start + 1;
                        if let Some(title_end) = response[abs_title_start..].find("</a>") {
                            let title = strip_html_tags(&response[abs_title_start..abs_title_start + title_end]);
                            
                            let mut snippet = String::new();
                            if let Some(snippet_start) = response[abs_a_start..].find("class=\"result__snippet\"") {
                                let abs_snippet_start = abs_a_start + snippet_start;
                                if let Some(text_start) = response[abs_snippet_start..].find(">") {
                                    let abs_text_start = abs_snippet_start + text_start + 1;
                                    if let Some(text_end) = response[abs_text_start..].find("</a>") {
                                        snippet = strip_html_tags(&response[abs_text_start..abs_text_start + text_end]);
                                    }
                                }
                            }
                            
                            results.push(format!("Title: {}\nLink: {}\nSnippet: {}\n", title.trim(), href.trim(), snippet.trim()));
                        }
                    }
                }
            }
        }
        
        if results.len() >= 8 {
            break;
        }
    }
    
    if results.is_empty() {
        Ok("No results found.".to_string())
    } else {
        Ok(results.join("\n---\n\n"))
    }
}

async fn web_search_helper_async(query: &str) -> Result<String, anyhow::Error> {
    match web_search_ddg_helper_async(query).await {
        Ok(res) if res != "No results found." => Ok(res),
        _ => ecosia_search_helper_async(query).await,
    }
}

/// Helper for case-insensitive substring search.
fn find_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let needle_lower = needle.to_lowercase();
    let needle_len = needle.len();
    haystack.char_indices().find_map(|(i, _)| {
        if let Some(chunk) = haystack.get(i..i + needle_len) {
            if chunk.eq_ignore_ascii_case(&needle_lower) {
                return Some(i);
            }
        }
        None
    })
}

/// Helper to print styled text while maintaining custom theme background and foreground.
fn print_themed(stdout: &mut std::io::Stdout, text: &str, colors: &TalkThemeColors) -> Result<()> {
    let bg_esc = match colors.bg {
        crossterm::style::Color::Rgb { r, g, b } => format!("\x1b[48;2;{};{};{}m", r, g, b),
        _ => "\x1b[49m".to_string(),
    };
    let fg_esc = match colors.fg {
        crossterm::style::Color::Rgb { r, g, b } => format!("\x1b[38;2;{};{};{}m", r, g, b),
        _ => "\x1b[39m".to_string(),
    };
    let theme_reset = format!("\x1b[0m{}{}", bg_esc, fg_esc);
    let formatted = text.replace("\x1b[0m", &theme_reset);
    execute!(
        stdout,
        crossterm::style::SetBackgroundColor(colors.bg),
        crossterm::style::SetForegroundColor(colors.fg),
        Print(formatted)
    )?;
    Ok(())
}

/// Async helper to read a URL, strip CSS, JS, and HTML, and format to clean text.
async fn web_read_helper_async(url: &str) -> Result<String, anyhow::Error> {
    let response = fetch_url_with_curl_async(url).await?;
    
    let mut clean_html = String::new();
    let mut cursor = 0;
    
    while cursor < response.len() {
        if let Some(tag_start) = response[cursor..].find('<') {
            let abs_tag_start = cursor + tag_start;
            clean_html.push_str(&response[cursor..abs_tag_start]);
            
            let tag_content = &response[abs_tag_start..];
            if tag_content.get(..7).map(|s| s.eq_ignore_ascii_case("<script")).unwrap_or(false) {
                if let Some(tag_end) = find_case_insensitive(tag_content, "</script>") {
                    cursor = abs_tag_start + tag_end + 9;
                } else {
                    cursor = abs_tag_start + 1;
                }
            } else if tag_content.get(..6).map(|s| s.eq_ignore_ascii_case("<style")).unwrap_or(false) {
                if let Some(tag_end) = find_case_insensitive(tag_content, "</style>") {
                    cursor = abs_tag_start + tag_end + 8;
                } else {
                    cursor = abs_tag_start + 1;
                }
            } else {
                if let Some(closing_bracket) = tag_content.find('>') {
                    cursor = abs_tag_start + closing_bracket + 1;
                } else {
                    cursor = abs_tag_start + 1;
                }
            }
        } else {
            clean_html.push_str(&response[cursor..]);
            break;
        }
    }
    
    let mut final_text = strip_html_tags(&clean_html);
    
    let mut formatted = String::new();
    let mut last_was_space = false;
    let mut newline_count = 0;
    
    for c in final_text.chars() {
        if c == '\n' || c == '\r' {
            if newline_count < 2 {
                formatted.push('\n');
                newline_count += 1;
            }
            last_was_space = false;
        } else if c.is_whitespace() {
            if !last_was_space {
                formatted.push(' ');
                last_was_space = true;
            }
            newline_count = 0;
        } else {
            formatted.push(c);
            last_was_space = false;
            newline_count = 0;
        }
    }
    
    let mut trimmed = formatted.trim().to_string();
    if trimmed.len() > 6000 {
        let mut truncate_len = 5900;
        while truncate_len > 0 && !trimmed.is_char_boundary(truncate_len) {
            truncate_len -= 1;
        }
        trimmed.truncate(truncate_len);
        trimmed.push_str("\n\n[Content truncated due to length...]");
    }
    
    Ok(trimmed)
}

/// Helper function to strip ANSI escape sequences from a string to measure visual length.
fn strip_ansi_escapes(s: &str) -> String {
    let mut result = String::new();
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next(); // Consume '['
                while let Some(next_char) = chars.next() {
                    if next_char >= '@' && next_char <= '~' {
                        break;
                    }
                }
            } else {
                chars.next(); // Consume single escape trailing char
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Accept completion suggestion and merge with current input based on spaces and command type.
fn get_completed_input(input: &str, item: &crate::suggestions::types::CompletionItem) -> String {
    let mut insert_text = item.insert_text.clone();
    
    // Automatically append space for commands/scripts if they don't have it
    if item.kind == crate::suggestions::types::CompletionKind::Subcommand
        || item.kind == crate::suggestions::types::CompletionKind::Script
    {
        if !insert_text.ends_with(' ') {
            insert_text.push(' ');
        }
    }

    if input.is_empty() {
        return insert_text;
    }

    let input_trimmed = input.trim_end();
    if !input_trimmed.contains(' ') {
        // Single word input (e.g., "c" or "cd")
        if item.kind == crate::suggestions::types::CompletionKind::Subcommand
            && item.insert_text.starts_with(input_trimmed)
        {
            // Completing the command name itself (e.g., "c" -> "cd ")
            insert_text
        } else {
            // Completing an argument for the command (e.g., "cd" -> "cd stratum/")
            format!("{} {}", input_trimmed, insert_text)
        }
    } else if input.ends_with(' ') {
        // Ends with space, just append
        format!("{}{}", input, insert_text)
    } else {
        // Replace the last word (partial argument)
        if let Some(last_space_idx) = input.rfind(' ') {
            let base = &input[..=last_space_idx];
            format!("{}{}", base, insert_text)
        } else {
            insert_text
        }
    }
}

/// Clear the suggestions card visually from the screen and restore the cursor.
fn clear_card_from_screen(
    stdout: &mut std::io::Stdout,
    input: &str,
    cursor_pos: usize,
    card_item_count: usize,
) -> Result<()> {
    if card_item_count > 0 {
        let cwd = std::env::current_dir()
            .map(|p| p.display().to_string())
            .unwrap_or_else(|_| "unknown".into());
        let prompt_plain = format!("⚡ stratum [shell] @[{}]> ", cwd);
        let prompt_visual_len = prompt_plain.chars().count();
        let edit_col = prompt_visual_len + cursor_pos;
        
        execute!(
            stdout,
            Print("\r\n"),
            Clear(ClearType::FromCursorDown),
            cursor::MoveUp(1),
            cursor::MoveRight(edit_col as u16),
        )?;
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct TalkThemeColors {
    pub bg: crossterm::style::Color,
    pub fg: crossterm::style::Color,
    pub out: crossterm::style::Color,
    pub card: crossterm::style::Color,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct TalkThemeConfig {
    bg: String,
    fg: String,
    out: String,
    card: String,
}

struct PredefinedTheme {
    name: &'static str,
    bg: crossterm::style::Color,
    fg: crossterm::style::Color,
    out: crossterm::style::Color,
    card: crossterm::style::Color,
}

const PREDEFINED_THEMES: &[PredefinedTheme] = &[
    PredefinedTheme {
        name: "Dracula",
        bg: crossterm::style::Color::Rgb { r: 40, g: 42, b: 54 },
        fg: crossterm::style::Color::Rgb { r: 248, g: 248, b: 242 },
        out: crossterm::style::Color::Rgb { r: 80, g: 250, b: 123 },
        card: crossterm::style::Color::Rgb { r: 189, g: 147, b: 249 },
    },
    PredefinedTheme {
        name: "Gruvbox Dark",
        bg: crossterm::style::Color::Rgb { r: 40, g: 40, b: 40 },
        fg: crossterm::style::Color::Rgb { r: 235, g: 219, b: 178 },
        out: crossterm::style::Color::Rgb { r: 184, g: 187, b: 38 },
        card: crossterm::style::Color::Rgb { r: 215, g: 153, b: 33 },
    },
    PredefinedTheme {
        name: "Tokyo Night",
        bg: crossterm::style::Color::Rgb { r: 26, g: 27, b: 38 },
        fg: crossterm::style::Color::Rgb { r: 169, g: 177, b: 214 },
        out: crossterm::style::Color::Rgb { r: 158, g: 206, b: 106 },
        card: crossterm::style::Color::Rgb { r: 122, g: 162, b: 247 },
    },
    PredefinedTheme {
        name: "One Dark",
        bg: crossterm::style::Color::Rgb { r: 40, g: 44, b: 52 },
        fg: crossterm::style::Color::Rgb { r: 171, g: 178, b: 191 },
        out: crossterm::style::Color::Rgb { r: 152, g: 195, b: 121 },
        card: crossterm::style::Color::Rgb { r: 97, g: 175, b: 239 },
    },
    PredefinedTheme {
        name: "Monokai Pro",
        bg: crossterm::style::Color::Rgb { r: 45, g: 42, b: 46 },
        fg: crossterm::style::Color::Rgb { r: 252, g: 252, b: 250 },
        out: crossterm::style::Color::Rgb { r: 169, g: 220, b: 118 },
        card: crossterm::style::Color::Rgb { r: 255, g: 216, b: 102 },
    },
    PredefinedTheme {
        name: "Nord",
        bg: crossterm::style::Color::Rgb { r: 46, g: 52, b: 64 },
        fg: crossterm::style::Color::Rgb { r: 216, g: 222, b: 233 },
        out: crossterm::style::Color::Rgb { r: 163, g: 190, b: 140 },
        card: crossterm::style::Color::Rgb { r: 136, g: 192, b: 208 },
    },
    PredefinedTheme {
        name: "Solarized Dark",
        bg: crossterm::style::Color::Rgb { r: 0, g: 43, b: 54 },
        fg: crossterm::style::Color::Rgb { r: 131, g: 148, b: 150 },
        out: crossterm::style::Color::Rgb { r: 133, g: 153, b: 0 },
        card: crossterm::style::Color::Rgb { r: 38, g: 139, b: 210 },
    },
    PredefinedTheme {
        name: "Cyberpunk",
        bg: crossterm::style::Color::Rgb { r: 15, g: 15, b: 26 },
        fg: crossterm::style::Color::Rgb { r: 0, g: 255, b: 204 },
        out: crossterm::style::Color::Rgb { r: 255, g: 0, b: 127 },
        card: crossterm::style::Color::Rgb { r: 158, g: 0, b: 255 },
    },
    PredefinedTheme {
        name: "Rose Pine",
        bg: crossterm::style::Color::Rgb { r: 25, g: 23, b: 36 },
        fg: crossterm::style::Color::Rgb { r: 224, g: 222, b: 244 },
        out: crossterm::style::Color::Rgb { r: 156, g: 207, b: 216 },
        card: crossterm::style::Color::Rgb { r: 196, g: 167, b: 231 },
    },
    PredefinedTheme {
        name: "Aura",
        bg: crossterm::style::Color::Rgb { r: 21, g: 20, b: 27 },
        fg: crossterm::style::Color::Rgb { r: 237, g: 236, b: 238 },
        out: crossterm::style::Color::Rgb { r: 97, g: 255, b: 202 },
        card: crossterm::style::Color::Rgb { r: 162, g: 119, b: 255 },
    },
];

fn parse_hex_color(s: &str) -> Option<crossterm::style::Color> {
    let s = s.trim().trim_start_matches('#');
    if s.len() == 3 {
        let r = u8::from_str_radix(&s[0..1], 16).ok()? * 17;
        let g = u8::from_str_radix(&s[1..2], 16).ok()? * 17;
        let b = u8::from_str_radix(&s[2..3], 16).ok()? * 17;
        Some(crossterm::style::Color::Rgb { r, g, b })
    } else if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some(crossterm::style::Color::Rgb { r, g, b })
    } else {
        None
    }
}

fn format_color_hex(c: &crossterm::style::Color) -> String {
    match c {
        crossterm::style::Color::Rgb { r, g, b } => format!("#{:02x}{:02x}{:02x}", r, g, b),
        _ => "reset".to_string(),
    }
}

fn get_talk_theme_path() -> std::path::PathBuf {
    if let Some(config_dir) = dirs::config_dir() {
        config_dir.join("stratum").join("talk_theme.json")
    } else {
        std::path::PathBuf::from("talk_theme.json")
    }
}

pub fn load_talk_theme() -> TalkThemeColors {
    let path = get_talk_theme_path();
    if path.exists() {
        if let Ok(content) = std::fs::read_to_string(path) {
            if let Ok(config) = serde_json::from_str::<TalkThemeConfig>(&content) {
                let parse = |s: &str| {
                    if s == "reset" {
                        crossterm::style::Color::Reset
                    } else {
                        parse_hex_color(s).unwrap_or(crossterm::style::Color::Reset)
                    }
                };
                return TalkThemeColors {
                    bg: parse(&config.bg),
                    fg: parse(&config.fg),
                    out: parse(&config.out),
                    card: parse(&config.card),
                };
            }
        }
    }
    TalkThemeColors {
        bg: crossterm::style::Color::Reset,
        fg: crossterm::style::Color::Reset,
        out: crossterm::style::Color::Reset,
        card: crossterm::style::Color::Reset,
    }
}

pub fn save_talk_theme(colors: &TalkThemeColors) {
    let path = get_talk_theme_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let to_str = |c: &crossterm::style::Color| {
        match c {
            crossterm::style::Color::Reset => "reset".to_string(),
            other => format_color_hex(other),
        }
    };
    let config = TalkThemeConfig {
        bg: to_str(&colors.bg),
        fg: to_str(&colors.fg),
        out: to_str(&colors.out),
        card: to_str(&colors.card),
    };
    if let Ok(content) = serde_json::to_string_pretty(&config) {
        let _ = std::fs::write(path, content);
    }
}

fn run_theme_selector(
    current_colors: &mut TalkThemeColors,
    stdout: &mut std::io::Stdout,
) -> Result<()> {
    let original_colors = *current_colors;
    let mut selected_idx = 0;
    let total_options = PREDEFINED_THEMES.len() + 1;

    let _ = execute!(stdout, cursor::Hide);

    loop {
        let preview_colors = if selected_idx < PREDEFINED_THEMES.len() {
            let t = &PREDEFINED_THEMES[selected_idx];
            TalkThemeColors {
                bg: t.bg,
                fg: t.fg,
                out: t.out,
                card: t.card,
            }
        } else {
            original_colors
        };

        apply_terminal_colors(stdout, &preview_colors);

        let _ = execute!(
            stdout,
            crossterm::style::SetBackgroundColor(preview_colors.bg),
            crossterm::style::SetForegroundColor(preview_colors.fg),
            Clear(ClearType::All),
            cursor::MoveTo(0, 0),
        );

        let _ = execute!(
            stdout,
            Print("\r\n  === Stratum Theme Customizer ==="),
            Print("\r\n  Use Up/Down Arrow keys to preview themes. Enter to select, Esc to cancel.\r\n\r\n"),
        );

        for i in 0..total_options {
            let is_selected = i == selected_idx;
            let prefix = if is_selected { " ▸ " } else { "   " };
            let style_color = if is_selected {
                preview_colors.card
            } else {
                preview_colors.fg
            };

            let name = if i < PREDEFINED_THEMES.len() {
                PREDEFINED_THEMES[i].name
            } else {
                "Custom Hex Theme"
            };

            let _ = execute!(
                stdout,
                crossterm::style::SetForegroundColor(style_color),
                Print(format!("{}{}\r\n", prefix, name)),
            );
        }

        let _ = execute!(
            stdout,
            crossterm::style::SetForegroundColor(preview_colors.fg),
            Print("\r\n  Theme Colors Preview:\r\n"),
        );

        let _ = execute!(
            stdout,
            Print("  - Background: "),
            crossterm::style::SetBackgroundColor(preview_colors.bg),
            Print("██████████\r\n"),
        );

        let _ = execute!(
            stdout,
            Print("  - Normal Text: "),
            crossterm::style::SetForegroundColor(preview_colors.fg),
            Print("This is normal terminal text.\r\n"),
        );

        let out_c = match preview_colors.out {
            crossterm::style::Color::Reset => preview_colors.fg,
            other => other,
        };
        let _ = execute!(
            stdout,
            Print("  - Output Text: "),
            crossterm::style::SetForegroundColor(out_c),
            Print("This is how command/AI outputs look.\r\n"),
        );

        let card_c = match preview_colors.card {
            crossterm::style::Color::Reset => preview_colors.fg,
            other => other,
        };
        let _ = execute!(
            stdout,
            Print("  - Card Theme:  "),
            crossterm::style::SetForegroundColor(card_c),
            Print("┌── Suggestion Border ──┐\r\n"),
            Print("                 │ ▸ Selected Item       │\r\n"),
            Print("                 └───────────────────────┘\r\n"),
        );

        let _ = stdout.flush();

        match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => {
                match key.code {
                    KeyCode::Up => {
                        if selected_idx > 0 {
                            selected_idx -= 1;
                        } else {
                            selected_idx = total_options - 1;
                        }
                    }
                    KeyCode::Down => {
                        if selected_idx + 1 < total_options {
                            selected_idx += 1;
                        } else {
                            selected_idx = 0;
                        }
                    }
                    KeyCode::Esc => {
                        *current_colors = original_colors;
                        apply_terminal_colors(stdout, current_colors);
                        let _ = execute!(
                            stdout,
                            crossterm::style::SetBackgroundColor(current_colors.bg),
                            crossterm::style::SetForegroundColor(current_colors.fg),
                            Clear(ClearType::All),
                            cursor::MoveTo(0, 0),
                            cursor::Show,
                        );
                        return Ok(());
                    }
                    KeyCode::Enter => {
                        if selected_idx < PREDEFINED_THEMES.len() {
                            let t = &PREDEFINED_THEMES[selected_idx];
                            *current_colors = TalkThemeColors {
                                bg: t.bg,
                                fg: t.fg,
                                out: t.out,
                                card: t.card,
                            };
                            save_talk_theme(current_colors);
                            apply_terminal_colors(stdout, current_colors);
                            let _ = execute!(
                                stdout,
                                crossterm::style::SetBackgroundColor(current_colors.bg),
                                crossterm::style::SetForegroundColor(current_colors.fg),
                                Clear(ClearType::All),
                                cursor::MoveTo(0, 0),
                                cursor::Show,
                                Print(format!("\r\n  ✓ Theme '{}' applied successfully!\r\n", t.name)),
                            );
                            return Ok(());
                        } else {
                            let _ = execute!(
                                stdout,
                                cursor::Show,
                                Print("\r\n  === Customize Theme ==="),
                                Print("\r\n  Enter hex colors in order: background, text, output, card theme"),
                                Print("\r\n  Format: #bg, #fg, #out, #card  (e.g., #1a1b26, #c0caf5, #7aa2f7, #bb9af0)\r\n  > "),
                            );
                            let _ = stdout.flush();

                            let _ = terminal::disable_raw_mode();
                            let mut hex_input = String::new();
                            let _ = std::io::stdin().read_line(&mut hex_input);
                            let _ = terminal::enable_raw_mode();
                            let _ = execute!(stdout, cursor::Hide);

                            let parts: Vec<&str> = hex_input.split(',').collect();
                            if parts.len() == 4 {
                                let bg_p = parse_hex_color(parts[0]);
                                let fg_p = parse_hex_color(parts[1]);
                                let out_p = parse_hex_color(parts[2]);
                                let card_p = parse_hex_color(parts[3]);

                                if let (Some(bg), Some(fg), Some(out), Some(card)) = (bg_p, fg_p, out_p, card_p) {
                                    *current_colors = TalkThemeColors { bg, fg, out, card };
                                    save_talk_theme(current_colors);
                                    apply_terminal_colors(stdout, current_colors);
                                    let _ = execute!(
                                        stdout,
                                        crossterm::style::SetBackgroundColor(current_colors.bg),
                                        crossterm::style::SetForegroundColor(current_colors.fg),
                                        Clear(ClearType::All),
                                        cursor::MoveTo(0, 0),
                                        cursor::Show,
                                        Print("\r\n  ✓ Custom theme applied successfully!\r\n"),
                                    );
                                    return Ok(());
                                }
                            }

                            let _ = execute!(
                                stdout,
                                Print("\r\n  ✗ Invalid format or hex code! Reverting to selector...\r\n"),
                            );
                            let _ = stdout.flush();
                            std::thread::sleep(Duration::from_secs(2));
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hex_color_parsing() {
        // 3-digit shorthand
        let color = parse_hex_color("#fff").unwrap();
        assert_eq!(color, crossterm::style::Color::Rgb { r: 255, g: 255, b: 255 });

        let color = parse_hex_color("#000").unwrap();
        assert_eq!(color, crossterm::style::Color::Rgb { r: 0, g: 0, b: 0 });

        // 6-digit hex
        let color = parse_hex_color("#1a1b26").unwrap();
        assert_eq!(color, crossterm::style::Color::Rgb { r: 26, g: 27, b: 38 });

        // No leading hash
        let color = parse_hex_color("abb2bf").unwrap();
        assert_eq!(color, crossterm::style::Color::Rgb { r: 171, g: 178, b: 191 });

        // Invalid hex
        assert!(parse_hex_color("invalid").is_none());
        assert!(parse_hex_color("#12").is_none());
    }

    #[test]
    fn test_format_color_hex() {
        let color = crossterm::style::Color::Rgb { r: 40, g: 42, b: 54 };
        assert_eq!(format_color_hex(&color), "#282a36");

        let color = crossterm::style::Color::Reset;
        assert_eq!(format_color_hex(&color), "reset");
    }

    #[test]
    fn test_strip_ansi_escapes() {
        let raw = "\x1b[1;35m⚡ stratum [ai] @[E:\\foo]>\x1b[0m ";
        let clean = strip_ansi_escapes(raw);
        assert_eq!(clean, "⚡ stratum [ai] @[E:\\foo]> ");
    }

    #[test]
    fn test_get_completed_input() {
        use crate::suggestions::types::CompletionItem;
        
        // 1. Single word command completion
        let item = CompletionItem::subcommand("cd", "Change directory");
        assert_eq!(get_completed_input("c", &item), "cd ");

        // 2. Folder completion when input is just the command
        let item = CompletionItem::directory("stratum");
        assert_eq!(get_completed_input("cd", &item), "cd stratum/");

        // 3. Folder completion when input ends with space
        let item = CompletionItem::directory("stratum");
        assert_eq!(get_completed_input("cd ", &item), "cd stratum/");

        // 4. Folder completion when input ends with a partial folder
        let item = CompletionItem::directory("stratum");
        assert_eq!(get_completed_input("cd stra", &item), "cd stratum/");
    }

    #[test]
    fn test_execute_command_success() {
        let cmd = if cfg!(windows) {
            "echo hello"
        } else {
            "echo hello"
        };
        let res = execute_command(cmd);
        assert!(res.is_ok());
        let (output, streamed) = res.unwrap();
        assert_eq!(output, "hello");
    }

    #[test]
    fn test_execute_command_failure() {
        let cmd = if cfg!(windows) {
            "dir_non_existent_folder_abc_123"
        } else {
            "ls -la /non_existent_folder_abc_123"
        };
        let res = execute_command(cmd);
        assert!(res.is_err());
    }

    #[tokio::test]
    async fn test_web_search_integration() {
        let res = web_search_helper_async("latest python version 2024").await;
        println!("web_search_helper_async result: {:?}", res);
        let output = res.unwrap();
        println!("Output length: {}", output.len());
        println!("Output preview: {}", if output.len() > 500 { &output[..500] } else { &output });
        assert!(!output.is_empty());
        assert!(output.contains("Link: "));
    }
}
