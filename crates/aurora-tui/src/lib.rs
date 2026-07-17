pub mod app;
mod deps_panel;
pub mod execution;
pub mod picker;
pub mod widgets;

use anyhow::Result;
use app::{
    ExecutionAction, ExecutionState, FocusPanel, LogSearch, LogViewState, PickerAction, PickerState,
};
use aurora_core::events::{BeamStatus, SchedulerEvent};
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use execution::split_layout;
use picker::view as picker_view;
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

/// Installs a panic hook that restores the terminal (leaves raw mode and the
/// alternate screen) before delegating to the previously installed hook.
///
/// Without it, a panic while the TUI holds the terminal unwinds past the
/// normal restore path and leaves the user's shell in raw mode inside the
/// alternate screen, with the panic message invisible or mangled. Chaining to
/// the previous hook keeps the panic message printing.
pub fn install_terminal_panic_hook() {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = disable_raw_mode();
        let _ = execute!(io::stdout(), LeaveAlternateScreen);
        previous(info);
    }));
}

pub async fn run_execution_tui(
    beam_info: Vec<(String, Vec<String>)>,
    mut rx: mpsc::Receiver<SchedulerEvent>,
    mut cancel_tx: mpsc::UnboundedSender<String>,
    rerun: impl Fn(
        String,
        Vec<String>,
    ) -> (
        mpsc::Receiver<SchedulerEvent>,
        mpsc::UnboundedSender<String>,
    ),
) -> Result<()> {
    tokio::task::block_in_place(move || {
        install_terminal_panic_hook();
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut exec = ExecutionState::new(beam_info);
        let mut log_state = LogViewState::new(0);
        let mut search = LogSearch::new();
        let mut show_help = false;
        let mut tick: u64 = 0;

        let result = (|| -> Result<()> {
            loop {
                // Drain scheduler events
                while let Ok(evt) = rx.try_recv() {
                    let done_failed = matches!(evt, SchedulerEvent::AllDone { success: false });
                    let is_done = matches!(evt, SchedulerEvent::AllDone { .. });
                    exec.apply_event(evt);
                    if is_done {
                        // On failure, jump to the first Failed beam
                        // to show its logs without manual navigation.
                        if done_failed && exec.select_first_failed() {
                            log_state.beam_index = exec.selected;
                            log_state.scroll_locked = false;
                        }
                        break;
                    }
                }

                // Dimensions of the log panel and total visual height of the
                // selected beam, used to drive scroll and auto-scroll in
                // visual lines.
                let size = terminal.size()?;
                let (log_w, log_h) = log_panel_dims(size.width, size.height, exec.show_deps);
                let total_visual = exec.beams[log_state.beam_index].total_visual_rows(log_w);

                // Active search: recompute matches as new output arrives,
                // without moving the view (non-intrusive). The current
                // n/N position is preserved.
                if search.is_active() {
                    search.recompute_preserving(&exec.beams[log_state.beam_index]);
                }

                // Auto-scroll if not locked
                log_state.auto_scroll(total_visual, log_h);

                terminal.draw(|f| {
                    split_layout::render_execution(f, &exec, &log_state, &search, tick, show_help);
                })?;
                tick += 1;

                if event::poll(Duration::from_millis(50))? {
                    if let Event::Key(key) = event::read()? {
                        // Help popup captures everything
                        if show_help {
                            match key.code {
                                KeyCode::Char('?') | KeyCode::Esc => show_help = false,
                                _ => {}
                            }
                            continue;
                        }

                        // Beam filter input mode (`/` on the beams panel):
                        // keystrokes filter the list. Enter locks it in,
                        // Esc clears it. The selection follows the filter and the
                        // displayed logs follow the selection.
                        if exec.filter_input {
                            let mut filter_changed = true;
                            match key.code {
                                KeyCode::Esc => {
                                    exec.beam_filter.clear();
                                    exec.filter_input = false;
                                }
                                KeyCode::Enter => {
                                    exec.filter_input = false;
                                    filter_changed = false;
                                }
                                KeyCode::Backspace => {
                                    exec.beam_filter.pop();
                                }
                                KeyCode::Char(c) => {
                                    exec.beam_filter.push(c);
                                }
                                _ => filter_changed = false,
                            }
                            if filter_changed {
                                exec.clamp_selection_to_visible();
                                if log_state.beam_index != exec.selected {
                                    log_state.beam_index = exec.selected;
                                    log_state.scroll_locked = false;
                                }
                            }
                            continue;
                        }

                        // Search input mode: captures everything
                        if search.input_active {
                            match key.code {
                                KeyCode::Esc => search.clear(),
                                KeyCode::Enter => search.input_active = false,
                                KeyCode::Backspace => {
                                    search.query.pop();
                                    refresh_search(
                                        &mut search,
                                        &exec,
                                        &mut log_state,
                                        log_w,
                                        log_h,
                                    );
                                }
                                KeyCode::Char(c) => {
                                    search.query.push(c);
                                    refresh_search(
                                        &mut search,
                                        &exec,
                                        &mut log_state,
                                        log_w,
                                        log_h,
                                    );
                                }
                                _ => {}
                            }
                            continue;
                        }

                        match handle_execution_key(
                            key,
                            &mut exec,
                            &mut log_state,
                            &mut search,
                            &mut show_help,
                            LogMetrics {
                                total_visual,
                                width: log_w,
                                height: log_h,
                            },
                        ) {
                            ExecKeyOutcome::Continue => {}
                            ExecKeyOutcome::Quit => return Ok(()),
                            ExecKeyOutcome::CancelSelected(name) => {
                                let _ = cancel_tx.send(name);
                            }
                            ExecKeyOutcome::Rerun { root, pre_success } => {
                                log_state = LogViewState::new(exec.selected);
                                search.clear();
                                let (new_rx, new_cancel) = rerun(root, pre_success);
                                rx = new_rx;
                                cancel_tx = new_cancel;
                            }
                            // TODO(task 11): wire the watch toggle into the render loop
                            // (arm/disarm WatchUiState, start/stop the notify watcher).
                            ExecKeyOutcome::ToggleWatch => {}
                        }
                    }
                }
            }
        })();

        let _ = disable_raw_mode();
        let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
        result
    })
}

pub fn run_picker(
    beam_info: Vec<(String, Option<String>, Vec<String>)>,
) -> Result<Option<Vec<String>>> {
    tokio::task::block_in_place(|| {
        install_terminal_panic_hook();
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut state = PickerState::new(beam_info);

        let result = (|| -> Result<Option<Vec<String>>> {
            loop {
                terminal.draw(|f| picker_view::render_picker(f, &state))?;
                if event::poll(Duration::from_millis(100))? {
                    if let Event::Key(key) = event::read()? {
                        match state.handle_key(key) {
                            Some(PickerAction::Launch(names)) => return Ok(Some(names)),
                            Some(PickerAction::Quit) => return Ok(None),
                            None => {}
                        }
                    }
                }
            }
        })();

        let _ = disable_raw_mode();
        let _ = execute!(terminal.backend_mut(), LeaveAlternateScreen);
        result
    })
}

/// Inner dimensions (width, height) of the log panel for a given terminal.
/// Derived from the same `execution_layout` split used by `render_execution`,
/// so logical indices convert to visual offsets and the scroll is bounded
/// against the exact panel the user sees.
fn log_panel_dims(width: u16, height: u16, show_deps: bool) -> (u16, u16) {
    let area = ratatui::layout::Rect::new(0, 0, width, height);
    let logs = split_layout::execution_layout(area, show_deps).logs;
    (logs.width.saturating_sub(2), logs.height.saturating_sub(2))
}

/// Effects of the runner's keyboard dispatch that need resources owned by the
/// loop (cancellation channel, rerun, exit) that `handle_execution_key` cannot
/// produce itself. Everything else (selection, scroll, search, help, deps
/// panel) is mutated directly on the state passed as argument.
#[derive(Debug, PartialEq, Eq)]
enum ExecKeyOutcome {
    Continue,
    Quit,
    CancelSelected(String),
    Rerun {
        root: String,
        pre_success: Vec<String>,
    },
    ToggleWatch,
}

/// Log panel metrics needed to drive scrolling: total visual height of the
/// current beam and inner dimensions of the panel.
#[derive(Clone, Copy)]
struct LogMetrics {
    total_visual: u16,
    width: u16,
    height: u16,
}

/// Keyboard dispatch for the runner view (excluding the help popup and search
/// input, handled upstream). Mutates local state and returns the effect for
/// the loop to perform. Extracted from `run_execution_tui` to be testable
/// without a terminal.
fn handle_execution_key(
    key: crossterm::event::KeyEvent,
    exec: &mut ExecutionState,
    log_state: &mut LogViewState,
    search: &mut LogSearch,
    show_help: &mut bool,
    metrics: LogMetrics,
) -> ExecKeyOutcome {
    let LogMetrics {
        total_visual,
        width: log_w,
        height: log_h,
    } = metrics;
    match key.code {
        KeyCode::Char('q') => {
            let beam = &exec.beams[exec.selected];
            if matches!(beam.status, BeamStatus::Running) {
                return ExecKeyOutcome::CancelSelected(beam.name.clone());
            }
        }
        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            return ExecKeyOutcome::Quit;
        }
        KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let half = (log_h / 2).max(1) as i32;
            log_state.scroll_lines(-half, total_visual, log_h);
        }
        KeyCode::Char('d') if key.modifiers.contains(KeyModifiers::CONTROL) => {
            let half = (log_h / 2).max(1) as i32;
            log_state.scroll_lines(half, total_visual, log_h);
        }
        KeyCode::Char('d') => {
            let _ = exec.handle_key(key);
        }
        KeyCode::Char('?') => *show_help = true,
        KeyCode::Down | KeyCode::Char('j') => match exec.focus {
            FocusPanel::Beams => {
                exec.select_next();
                log_state.beam_index = exec.selected;
                log_state.scroll_locked = false;
                if search.is_active() {
                    refresh_search(search, exec, log_state, log_w, log_h);
                }
            }
            FocusPanel::Logs => {
                log_state.handle_key(key, total_visual, log_h);
            }
        },
        KeyCode::Up | KeyCode::Char('k') => match exec.focus {
            FocusPanel::Beams => {
                exec.select_prev();
                log_state.beam_index = exec.selected;
                log_state.scroll_locked = false;
                if search.is_active() {
                    refresh_search(search, exec, log_state, log_w, log_h);
                }
            }
            FocusPanel::Logs => {
                log_state.handle_key(key, total_visual, log_h);
            }
        },
        KeyCode::Home => match exec.focus {
            FocusPanel::Beams => {
                exec.select_first();
                log_state.beam_index = exec.selected;
                log_state.scroll_locked = false;
                if search.is_active() {
                    refresh_search(search, exec, log_state, log_w, log_h);
                }
            }
            FocusPanel::Logs => {
                log_state.scroll_to_top();
            }
        },
        KeyCode::End => match exec.focus {
            FocusPanel::Beams => {
                exec.select_last();
                log_state.beam_index = exec.selected;
                log_state.scroll_locked = false;
                if search.is_active() {
                    refresh_search(search, exec, log_state, log_w, log_h);
                }
            }
            FocusPanel::Logs => {
                log_state.scroll_to_bottom(total_visual, log_h);
            }
        },
        KeyCode::Tab | KeyCode::Left | KeyCode::Right => {
            let _ = exec.handle_key(key);
        }
        KeyCode::Char('/') => match exec.focus {
            // Contextual `/`: filters the beam list if focus is on the
            // beams, searches the logs if focus is on the logs.
            FocusPanel::Beams => {
                exec.beam_filter.clear();
                exec.filter_input = true;
                exec.selected = exec.visible_indices().first().copied().unwrap_or(0);
            }
            FocusPanel::Logs => {
                search.clear();
                search.input_active = true;
            }
        },
        KeyCode::Char('n') if search.is_active() => {
            search.next();
            apply_search_jump(
                search,
                &exec.beams[log_state.beam_index],
                log_w,
                log_h,
                log_state,
            );
        }
        KeyCode::Char('N') if search.is_active() => {
            search.prev();
            apply_search_jump(
                search,
                &exec.beams[log_state.beam_index],
                log_w,
                log_h,
                log_state,
            );
        }
        KeyCode::Esc if search.is_active() => {
            search.clear();
        }
        KeyCode::Esc => return ExecKeyOutcome::Quit,
        KeyCode::Char('g') => {
            log_state.scroll_to_top();
        }
        KeyCode::Char('G') => {
            log_state.scroll_to_bottom(total_visual, log_h);
        }
        KeyCode::PageUp | KeyCode::PageDown => {
            log_state.handle_key(key, total_visual, log_h);
        }
        KeyCode::Char('y') => {
            copy_logs_to_clipboard(&exec.beams[exec.selected]);
        }
        KeyCode::Char('r') => {
            if let Some(ExecutionAction::Rerun { root, pre_success }) = exec.handle_key(key) {
                return ExecKeyOutcome::Rerun { root, pre_success };
            }
        }
        KeyCode::Char('w') => return ExecKeyOutcome::ToggleWatch,
        _ => {}
    }
    ExecKeyOutcome::Continue
}

fn refresh_search(
    search: &mut LogSearch,
    exec: &ExecutionState,
    log_state: &mut LogViewState,
    width: u16,
    height: u16,
) {
    let beam = &exec.beams[log_state.beam_index];
    search.recompute(beam);
    apply_search_jump(search, beam, width, height, log_state);
}

/// Positions the scroll on the visual line of the current match, if there is one.
/// Converts the logical line index into a visual offset (long lines
/// occupy several lines on screen), bounded to the last full screen.
fn apply_search_jump(
    search: &LogSearch,
    beam: &app::BeamView,
    width: u16,
    height: u16,
    log_state: &mut LogViewState,
) {
    if let Some(line) = search.current_line() {
        let max = beam.total_visual_rows(width).saturating_sub(height);
        log_state.scroll = beam.visual_offset(line, width).min(max);
        log_state.scroll_locked = true;
    }
}

fn copy_logs_to_clipboard(beam: &app::BeamView) {
    let content = beam.all_logs().join("\n");
    match arboard::Clipboard::new() {
        Ok(mut cb) => {
            let _ = cb.set_text(content);
        }
        Err(_) => {
            // Clipboard unavailable (SSH without X11): silent
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crossterm::event::{KeyEvent, KeyModifiers};

    fn key(code: KeyCode) -> KeyEvent {
        KeyEvent::new(code, KeyModifiers::NONE)
    }

    fn ctrl(c: char) -> KeyEvent {
        KeyEvent::new(KeyCode::Char(c), KeyModifiers::CONTROL)
    }

    /// Minimal runner state to exercise the keyboard dispatch without a terminal.
    fn fixture() -> (ExecutionState, LogViewState, LogSearch, bool) {
        let exec = ExecutionState::new(vec![
            ("build".to_string(), vec!["lint".to_string()]),
            ("lint".to_string(), vec![]),
        ]);
        (exec, LogViewState::new(0), LogSearch::new(), false)
    }

    const METRICS: LogMetrics = LogMetrics {
        total_visual: 0,
        width: 80,
        height: 24,
    };

    fn press(
        code: KeyCode,
        exec: &mut ExecutionState,
        log_state: &mut LogViewState,
        search: &mut LogSearch,
        show_help: &mut bool,
    ) -> ExecKeyOutcome {
        handle_execution_key(key(code), exec, log_state, search, show_help, METRICS)
    }

    // Regression: a plain « d » in the runner loop must toggle the dependency
    // panel. The original bug was that `run_execution_tui`'s dispatch did not
    // route the key to `ExecutionState::handle_key`.
    #[test]
    fn d_toggles_deps_panel_through_dispatch() {
        let (mut exec, mut ls, mut s, mut help) = fixture();
        assert!(!exec.show_deps);

        let out = press(KeyCode::Char('d'), &mut exec, &mut ls, &mut s, &mut help);
        assert_eq!(out, ExecKeyOutcome::Continue);
        assert!(exec.show_deps, "d shows the dependencies");

        press(KeyCode::Char('d'), &mut exec, &mut ls, &mut s, &mut help);
        assert!(!exec.show_deps, "d hides the dependencies");
    }

    // Ctrl+d stays half-page-down and must never touch the deps panel.
    #[test]
    fn ctrl_d_scrolls_and_keeps_deps_hidden() {
        let (mut exec, mut ls, mut s, mut help) = fixture();
        let out = handle_execution_key(ctrl('d'), &mut exec, &mut ls, &mut s, &mut help, METRICS);
        assert_eq!(out, ExecKeyOutcome::Continue);
        assert!(!exec.show_deps, "Ctrl+d does not toggle the dependencies");
    }

    #[test]
    fn esc_yields_quit() {
        let (mut exec, mut ls, mut s, mut help) = fixture();
        let out = press(KeyCode::Esc, &mut exec, &mut ls, &mut s, &mut help);
        assert_eq!(out, ExecKeyOutcome::Quit);
    }

    #[test]
    fn slash_on_beams_starts_beam_filter() {
        let (mut exec, mut ls, mut s, mut help) = fixture();
        assert_eq!(exec.focus, FocusPanel::Beams);
        let out = press(KeyCode::Char('/'), &mut exec, &mut ls, &mut s, &mut help);
        assert_eq!(out, ExecKeyOutcome::Continue);
        assert!(exec.filter_input, "/ on the beams opens the beam filter");
        assert!(!s.input_active, "and does not open the log search");
    }

    #[test]
    fn slash_on_logs_starts_log_search() {
        let (mut exec, mut ls, mut s, mut help) = fixture();
        exec.focus = FocusPanel::Logs;
        let out = press(KeyCode::Char('/'), &mut exec, &mut ls, &mut s, &mut help);
        assert_eq!(out, ExecKeyOutcome::Continue);
        assert!(s.input_active, "/ on the logs opens the log search");
        assert!(!exec.filter_input, "and does not open the beam filter");
    }

    #[test]
    fn tab_switches_focus_without_side_effect() {
        let (mut exec, mut ls, mut s, mut help) = fixture();
        assert_eq!(exec.focus, FocusPanel::Beams);
        let out = press(KeyCode::Tab, &mut exec, &mut ls, &mut s, &mut help);
        assert_eq!(out, ExecKeyOutcome::Continue);
        assert_eq!(exec.focus, FocusPanel::Logs, "Tab switches focus");
        assert!(!exec.show_deps, "Tab does not touch the dependencies");
    }

    #[test]
    fn w_toggles_watch() {
        let (mut exec, mut ls, mut s, mut help) = fixture();
        let out = press(KeyCode::Char('w'), &mut exec, &mut ls, &mut s, &mut help);
        assert_eq!(out, ExecKeyOutcome::ToggleWatch);
    }
}
