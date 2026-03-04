pub mod app;
pub mod execution;
pub mod picker;
pub mod widgets;

use anyhow::Result;
use app::{ExecutionState, FocusPanel, LogViewState, PickerAction, PickerState};
use aurora_core::scheduler::SchedulerEvent;
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

pub async fn run_execution_tui(
    beam_names: Vec<String>,
    mut rx: mpsc::Receiver<SchedulerEvent>,
) -> Result<()> {
    tokio::task::block_in_place(move || {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut exec = ExecutionState::new(beam_names);
        let mut log_state = LogViewState::new(0, 0);
        let mut show_help = false;
        let mut tick: u64 = 0;

        let result = (|| -> Result<()> {
            loop {
                // Drainer les events scheduler
                while let Ok(evt) = rx.try_recv() {
                    let is_done = matches!(evt, SchedulerEvent::AllDone { .. });
                    exec.apply_event(evt);
                    if is_done {
                        break;
                    }
                }

                // Auto-scroll si pas verrouillé
                let total_lines = {
                    let beam = &exec.beams[log_state.beam_index];
                    beam.stdout.len()
                        + beam.stderr.len()
                        + if beam.stderr.is_empty() { 0 } else { 1 }
                };
                log_state.auto_scroll(total_lines);

                terminal.draw(|f| {
                    split_layout::render_execution(f, &exec, &log_state, tick, show_help);
                })?;
                tick += 1;

                if event::poll(Duration::from_millis(50))? {
                    if let Event::Key(key) = event::read()? {
                        // Help popup capture tout
                        if show_help {
                            match key.code {
                                KeyCode::Char('?') | KeyCode::Esc => show_help = false,
                                _ => {}
                            }
                            continue;
                        }

                        match key.code {
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Char('c')
                                if key.modifiers.contains(KeyModifiers::CONTROL) =>
                            {
                                return Ok(());
                            }
                            KeyCode::Char('?') => show_help = true,
                            KeyCode::Down | KeyCode::Char('j') => {
                                match exec.focus {
                                    FocusPanel::Beams => {
                                        exec.select_next();
                                        log_state.beam_index = exec.selected;
                                        log_state.scroll_locked = false;
                                    }
                                    FocusPanel::Logs => {
                                        let current_total = {
                                            let beam = &exec.beams[log_state.beam_index];
                                            beam.stdout.len() + beam.stderr.len() + if beam.stderr.is_empty() { 0 } else { 1 }
                                        };
                                        let height = terminal.size()?.height;
                                        log_state.handle_key(key, current_total, height);
                                    }
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                match exec.focus {
                                    FocusPanel::Beams => {
                                        exec.select_prev();
                                        log_state.beam_index = exec.selected;
                                        log_state.scroll_locked = false;
                                    }
                                    FocusPanel::Logs => {
                                        let current_total = {
                                            let beam = &exec.beams[log_state.beam_index];
                                            beam.stdout.len() + beam.stderr.len() + if beam.stderr.is_empty() { 0 } else { 1 }
                                        };
                                        let height = terminal.size()?.height;
                                        log_state.handle_key(key, current_total, height);
                                    }
                                }
                            }
                            KeyCode::Tab => {
                                let _ = exec.handle_key(key);
                            }
                            KeyCode::Char('G') => {
                                log_state.scroll_locked = false;
                            }
                            KeyCode::PageUp | KeyCode::PageDown => {
                                let current_total = {
                                    let beam = &exec.beams[log_state.beam_index];
                                    beam.stdout.len() + beam.stderr.len() + if beam.stderr.is_empty() { 0 } else { 1 }
                                };
                                let height = terminal.size()?.height;
                                log_state.handle_key(key, current_total, height);
                            }
                            KeyCode::Char('y') => {
                                copy_logs_to_clipboard(&exec.beams[exec.selected]);
                            }
                            _ => {}
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

fn copy_logs_to_clipboard(beam: &app::BeamView) {
    let content = beam.all_logs().join("\n");
    match arboard::Clipboard::new() {
        Ok(mut cb) => {
            let _ = cb.set_text(content);
        }
        Err(_) => {
            // Clipboard non disponible (SSH sans X11) — silencieux
        }
    }
}
