pub mod app;
pub mod execution;
pub mod picker;
pub mod widgets;

use anyhow::Result;
use app::{ExecutionAction, ExecutionState, FocusPanel, LogSearch, LogViewState, PickerAction, PickerState};
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
    beam_info: Vec<(String, Vec<String>)>,
    mut rx: mpsc::Receiver<SchedulerEvent>,
    rerun: impl Fn(String, Vec<String>) -> mpsc::Receiver<SchedulerEvent>,
) -> Result<()> {
    tokio::task::block_in_place(move || {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let mut exec = ExecutionState::new(beam_info);
        let mut log_state = LogViewState::new(0, 0);
        let mut search = LogSearch::new();
        let mut show_help = false;
        let mut tick: u64 = 0;

        let result = (|| -> Result<()> {
            loop {
                // Drainer les events scheduler
                while let Ok(evt) = rx.try_recv() {
                    let done_failed = matches!(evt, SchedulerEvent::AllDone { success: false });
                    let is_done = matches!(evt, SchedulerEvent::AllDone { .. });
                    exec.apply_event(evt);
                    if is_done {
                        // En cas d'échec, sauter sur le premier beam Failed
                        // pour montrer ses logs sans navigation manuelle.
                        if done_failed && exec.select_first_failed() {
                            log_state.beam_index = exec.selected;
                            log_state.scroll_locked = false;
                        }
                        break;
                    }
                }

                // Recherche active : recalculer les correspondances au fil des
                // nouvelles sorties, sans déplacer la vue (non intrusif). La
                // position courante de n/N est préservée.
                if search.is_active() {
                    search.recompute_preserving(&exec.beams[log_state.beam_index]);
                }

                // Auto-scroll si pas verrouillé
                log_state.auto_scroll(exec.beams[log_state.beam_index].log_line_count());

                terminal.draw(|f| {
                    split_layout::render_execution(f, &exec, &log_state, &search, tick, show_help);
                })?;
                tick += 1;

                if event::poll(Duration::from_millis(50))? {
                    if let Event::Key(key) = event::read()? {
                        let size = terminal.size()?;
                        let log_w = log_panel_width(size.width, size.height);

                        // Help popup capture tout
                        if show_help {
                            match key.code {
                                KeyCode::Char('?') | KeyCode::Esc => show_help = false,
                                _ => {}
                            }
                            continue;
                        }

                        // Mode saisie de recherche : capture tout
                        if search.input_active {
                            match key.code {
                                KeyCode::Esc => search.clear(),
                                KeyCode::Enter => search.input_active = false,
                                KeyCode::Backspace => {
                                    search.query.pop();
                                    refresh_search(&mut search, &exec, &mut log_state, log_w);
                                }
                                KeyCode::Char(c) => {
                                    search.query.push(c);
                                    refresh_search(&mut search, &exec, &mut log_state, log_w);
                                }
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
                                        if search.is_active() {
                                            refresh_search(&mut search, &exec, &mut log_state, log_w);
                                        }
                                    }
                                    FocusPanel::Logs => {
                                        let total = exec.beams[log_state.beam_index].log_line_count();
                                        let height = terminal.size()?.height;
                                        log_state.handle_key(key, total, height);
                                    }
                                }
                            }
                            KeyCode::Up | KeyCode::Char('k') => {
                                match exec.focus {
                                    FocusPanel::Beams => {
                                        exec.select_prev();
                                        log_state.beam_index = exec.selected;
                                        log_state.scroll_locked = false;
                                        if search.is_active() {
                                            refresh_search(&mut search, &exec, &mut log_state, log_w);
                                        }
                                    }
                                    FocusPanel::Logs => {
                                        let total = exec.beams[log_state.beam_index].log_line_count();
                                        let height = terminal.size()?.height;
                                        log_state.handle_key(key, total, height);
                                    }
                                }
                            }
                            KeyCode::Tab => {
                                let _ = exec.handle_key(key);
                            }
                            KeyCode::Char('/') => {
                                search.clear();
                                search.input_active = true;
                            }
                            KeyCode::Char('n') if search.is_active() => {
                                search.next();
                                apply_search_jump(&search, &exec.beams[log_state.beam_index], log_w, &mut log_state);
                            }
                            KeyCode::Char('N') if search.is_active() => {
                                search.prev();
                                apply_search_jump(&search, &exec.beams[log_state.beam_index], log_w, &mut log_state);
                            }
                            KeyCode::Esc if search.is_active() => {
                                search.clear();
                            }
                            KeyCode::Char('G') => {
                                let total = exec.beams[log_state.beam_index].log_line_count();
                                log_state.scroll = total.saturating_sub(1) as u16;
                                log_state.scroll_locked = false;
                            }
                            KeyCode::PageUp | KeyCode::PageDown => {
                                let total = exec.beams[log_state.beam_index].log_line_count();
                                let height = terminal.size()?.height;
                                log_state.handle_key(key, total, height);
                            }
                            KeyCode::Char('y') => {
                                copy_logs_to_clipboard(&exec.beams[exec.selected]);
                            }
                            KeyCode::Char('r') => {
                                if let Some(ExecutionAction::Rerun { root, pre_success }) = exec.handle_key(key) {
                                    log_state = LogViewState::new(exec.selected, 0);
                                    search.clear();
                                    rx = rerun(root, pre_success);
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

/// Largeur intérieure du panneau logs pour un terminal donné. Réplique le
/// découpage de `render_execution` (vertical [Min, Length(1)] puis horizontal
/// 30/70) afin de convertir précisément les index logiques en offset visuel.
fn log_panel_width(width: u16, height: u16) -> u16 {
    use ratatui::layout::{Constraint, Direction, Layout, Rect};
    let area = Rect::new(0, 0, width, height);
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(1)])
        .split(area);
    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(outer[0]);
    split[1].width.saturating_sub(2)
}

/// Recalcule les correspondances pour le beam sélectionné et saute au match
/// courant. Utilisé quand la requête ou le beam change.
fn refresh_search(
    search: &mut LogSearch,
    exec: &ExecutionState,
    log_state: &mut LogViewState,
    width: u16,
) {
    let beam = &exec.beams[log_state.beam_index];
    search.recompute(beam);
    apply_search_jump(search, beam, width, log_state);
}

/// Positionne le scroll sur la ligne visuelle du match courant, s'il y en a un.
/// Convertit l'index de ligne logique en offset visuel (les lignes longues
/// occupent plusieurs lignes à l'écran).
fn apply_search_jump(
    search: &LogSearch,
    beam: &app::BeamView,
    width: u16,
    log_state: &mut LogViewState,
) {
    if let Some(line) = search.current_line() {
        log_state.scroll = beam.visual_offset(line, width);
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
            // Clipboard non disponible (SSH sans X11) — silencieux
        }
    }
}
