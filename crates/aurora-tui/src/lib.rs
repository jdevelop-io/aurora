pub mod app;
pub mod execution;
pub mod picker;
pub mod widgets;

use anyhow::Result;
use app::{
    ExecutionAction, ExecutionState, FocusPanel, LogSearch, LogViewState, PickerAction, PickerState,
};
use aurora_core::scheduler::{BeamStatus, SchedulerEvent};
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

                // Dimensions du panneau logs et hauteur visuelle totale du beam
                // sélectionné, pour piloter scroll et auto-scroll en lignes
                // visuelles.
                let size = terminal.size()?;
                let (log_w, log_h) = log_panel_dims(size.width, size.height, exec.show_deps);
                let total_visual = exec.beams[log_state.beam_index].total_visual_rows(log_w);

                // Recherche active : recalculer les correspondances au fil des
                // nouvelles sorties, sans déplacer la vue (non intrusif). La
                // position courante de n/N est préservée.
                if search.is_active() {
                    search.recompute_preserving(&exec.beams[log_state.beam_index]);
                }

                // Auto-scroll si pas verrouillé
                log_state.auto_scroll(total_visual, log_h);

                terminal.draw(|f| {
                    split_layout::render_execution(f, &exec, &log_state, &search, tick, show_help);
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

                        // Mode saisie du filtre de beams (`/` sur le panneau
                        // beams) : la frappe filtre la liste. Entrée verrouille,
                        // Échap efface. La sélection suit le filtre et les logs
                        // affichés suivent la sélection.
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

                        // Mode saisie de recherche : capture tout
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

/// Dimensions intérieures (largeur, hauteur) du panneau logs pour un terminal
/// donné. Réplique le découpage de `render_execution` (vertical [Min, Length(1)]
/// puis horizontal 30/70, ou 25/25/50 quand le panneau de dépendances est
/// affiché) afin de convertir précisément les index logiques en offset visuel et
/// de borner le scroll.
fn log_panel_dims(width: u16, height: u16, show_deps: bool) -> (u16, u16) {
    use ratatui::layout::{Constraint, Direction, Layout, Rect};
    let area = Rect::new(0, 0, width, height);
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(area);
    let split = if show_deps {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([
                Constraint::Percentage(25),
                Constraint::Percentage(25),
                Constraint::Percentage(50),
            ])
            .split(outer[0])
    } else {
        Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
            .split(outer[0])
    };
    let log_area = split[split.len() - 1];
    (
        log_area.width.saturating_sub(2),
        log_area.height.saturating_sub(2),
    )
}

/// Recalcule les correspondances pour le beam sélectionné et saute au match
/// courant. Utilisé quand la requête ou le beam change.
/// Effets du dispatch clavier runner qui nécessitent des ressources de la boucle
/// (canal d'annulation, relance, sortie), que `handle_execution_key` ne peut pas
/// réaliser lui-même. Tout le reste (sélection, scroll, recherche, aide, panneau
/// deps) est muté directement sur l'état passé en argument.
#[derive(Debug, PartialEq, Eq)]
enum ExecKeyOutcome {
    Continue,
    Quit,
    CancelSelected(String),
    Rerun {
        root: String,
        pre_success: Vec<String>,
    },
}

/// Métriques du panneau logs nécessaires au pilotage du scroll : hauteur
/// visuelle totale du beam courant et dimensions intérieures du panneau.
#[derive(Clone, Copy)]
struct LogMetrics {
    total_visual: u16,
    width: u16,
    height: u16,
}

/// Dispatch clavier de la vue runner (hors popup d'aide et saisie de recherche,
/// gérés en amont). Mute l'état local et renvoie l'effet à réaliser par la
/// boucle. Extrait de `run_execution_tui` pour être testable sans terminal.
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
            // `/` contextuel : filtre la liste de beams si le focus est sur les
            // beams, cherche dans les logs si le focus est sur les logs.
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

/// Positionne le scroll sur la ligne visuelle du match courant, s'il y en a un.
/// Convertit l'index de ligne logique en offset visuel (les lignes longues
/// occupent plusieurs lignes à l'écran), borné au dernier écran complet.
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
            // Clipboard non disponible (SSH sans X11) — silencieux
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

    /// État runner minimal pour exercer le dispatch clavier sans terminal.
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

    // Régression : un « d » simple dans la boucle runner doit basculer le panneau
    // de dépendances. Le bug initial venait du dispatch de `run_execution_tui`
    // qui ne routait pas la touche vers `ExecutionState::handle_key`.
    #[test]
    fn d_toggles_deps_panel_through_dispatch() {
        let (mut exec, mut ls, mut s, mut help) = fixture();
        assert!(!exec.show_deps);

        let out = press(KeyCode::Char('d'), &mut exec, &mut ls, &mut s, &mut help);
        assert_eq!(out, ExecKeyOutcome::Continue);
        assert!(exec.show_deps, "d affiche les dépendances");

        press(KeyCode::Char('d'), &mut exec, &mut ls, &mut s, &mut help);
        assert!(!exec.show_deps, "d masque les dépendances");
    }

    // Ctrl+d reste le demi-page-bas et ne doit jamais toucher au panneau deps.
    #[test]
    fn ctrl_d_scrolls_and_keeps_deps_hidden() {
        let (mut exec, mut ls, mut s, mut help) = fixture();
        let out = handle_execution_key(ctrl('d'), &mut exec, &mut ls, &mut s, &mut help, METRICS);
        assert_eq!(out, ExecKeyOutcome::Continue);
        assert!(!exec.show_deps, "Ctrl+d ne bascule pas les dépendances");
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
        assert!(
            exec.filter_input,
            "/ sur les beams ouvre le filtre de beams"
        );
        assert!(!s.input_active, "et n'ouvre pas la recherche de logs");
    }

    #[test]
    fn slash_on_logs_starts_log_search() {
        let (mut exec, mut ls, mut s, mut help) = fixture();
        exec.focus = FocusPanel::Logs;
        let out = press(KeyCode::Char('/'), &mut exec, &mut ls, &mut s, &mut help);
        assert_eq!(out, ExecKeyOutcome::Continue);
        assert!(s.input_active, "/ sur les logs ouvre la recherche de logs");
        assert!(!exec.filter_input, "et n'ouvre pas le filtre de beams");
    }

    #[test]
    fn tab_switches_focus_without_side_effect() {
        let (mut exec, mut ls, mut s, mut help) = fixture();
        assert_eq!(exec.focus, FocusPanel::Beams);
        let out = press(KeyCode::Tab, &mut exec, &mut ls, &mut s, &mut help);
        assert_eq!(out, ExecKeyOutcome::Continue);
        assert_eq!(exec.focus, FocusPanel::Logs, "Tab bascule le focus");
        assert!(!exec.show_deps, "Tab ne touche pas les dépendances");
    }
}
