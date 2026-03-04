pub mod app;
pub mod execution;
pub mod execution_view;
pub mod picker;
pub mod picker_view;
pub mod widgets;

use anyhow::Result;
use app::{App, AppMode};
use aurora_core::scheduler::SchedulerEvent;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use picker_view::{PickerBeam, PickerState};
use ratatui::{backend::CrosstermBackend, Terminal};
use std::io;
use std::time::Duration;
use tokio::sync::mpsc;

pub async fn run_execution_tui(
    beam_names: Vec<String>,
    mut rx: mpsc::Receiver<SchedulerEvent>,
) -> Result<()> {
    // block_in_place : crossterm utilise epoll en bloquant — incompatible avec le
    // runtime tokio sans cette directive. Permet aux autres tâches tokio (scheduler)
    // de continuer sur d'autres threads pendant que la TUI bloque ce thread.
    tokio::task::block_in_place(move || {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        let mut app = App::new(beam_names);
        let mut tick: u64 = 0;

        let result = (|| -> Result<()> {
            loop {
                while let Ok(evt) = rx.try_recv() {
                    let is_done = matches!(evt, SchedulerEvent::AllDone { .. });
                    app.apply_event(evt);
                    if is_done {
                        break;
                    }
                }

                terminal.draw(|f| execution_view::render_execution(f, &app, tick))?;
                tick += 1;

                if event::poll(Duration::from_millis(50))? {
                    if let Event::Key(key) = event::read()? {
                        match key.code {
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Down | KeyCode::Char('j') => app.select_next(),
                            KeyCode::Up | KeyCode::Char('k') => app.select_prev(),
                            KeyCode::Enter => {
                                app.mode = if app.mode == AppMode::LogView {
                                    AppMode::Running
                                } else {
                                    AppMode::LogView
                                };
                            }
                            KeyCode::Esc => {
                                if app.mode == AppMode::LogView {
                                    app.mode = AppMode::Running;
                                }
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
) -> Result<Option<String>> {
    // Même raison : appelé depuis un contexte async (main), doit déclarer le blocage.
    tokio::task::block_in_place(|| {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut state = PickerState {
            beams: beam_info
                .into_iter()
                .map(|(name, desc, deps)| PickerBeam {
                    name,
                    description: desc,
                    depends_on: deps,
                })
                .collect(),
            selected: 0,
            search: String::new(),
            show_deps: false,
        };

        let result = (|| -> Result<Option<String>> {
            loop {
                terminal.draw(|f| picker_view::render_picker(f, &state))?;

                if event::poll(Duration::from_millis(100))? {
                    if let Event::Key(key) = event::read()? {
                        let filtered_count = state.filtered().len();
                        match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => return Ok(None),
                            KeyCode::Enter => {
                                return Ok(state.selected_beam().map(|b| b.name.clone()));
                            }
                            KeyCode::Down | KeyCode::Char('j') => {
                                state.selected =
                                    (state.selected + 1).min(filtered_count.saturating_sub(1));
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                state.selected = state.selected.saturating_sub(1);
                            }
                            KeyCode::Char('/') => {
                                state.search.clear();
                            }
                            KeyCode::Backspace => {
                                state.search.pop();
                                state.selected = 0;
                            }
                            KeyCode::Char(c) => {
                                state.search.push(c);
                                state.selected = 0;
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
