use aurora_core::scheduler::BeamStatus;
use crate::app::{ExecutionState, FocusPanel, LogSearch, LogViewState};
use crate::execution::{beam_list, log_panel};
use ratatui::{
    layout::{Constraint, Direction, Layout},
    style::{Color, Style},
    widgets::Paragraph,
    Frame,
};

pub fn render_execution(
    f: &mut Frame,
    exec: &ExecutionState,
    log_state: &LogViewState,
    search: &LogSearch,
    tick: u64,
    show_help: bool,
) {
    let area = f.area();
    // Footer sur 2 lignes : état + barre, puis raccourcis (ou recherche).
    let outer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(0), Constraint::Length(2)])
        .split(area);

    // Split 30/70
    let split = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([Constraint::Percentage(30), Constraint::Percentage(70)])
        .split(outer[0]);

    let beams_focused = exec.focus == FocusPanel::Beams;
    beam_list::render_beam_list(f, exec, tick, split[0], beams_focused);

    let beam = &exec.beams[log_state.beam_index];
    log_panel::render_log_panel(f, beam, log_state, Some(search), split[1], !beams_focused);

    let footer = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(1), Constraint::Length(1)])
        .split(outer[1]);

    let total = exec.beams.len();
    let count_status = |pred: fn(&BeamStatus) -> bool| exec.beams.iter().filter(|b| pred(&b.status)).count();
    // Succès (cache inclus), avertissements (échecs tolérés), échecs (annulés inclus), skipped.
    let success = count_status(|s| matches!(s, BeamStatus::Success { .. }));
    let warning = count_status(|s| matches!(s, BeamStatus::FailedAllowed { .. }));
    let failed = count_status(|s| matches!(s, BeamStatus::Failed { .. } | BeamStatus::Cancelled));
    let skipped = count_status(|s| matches!(s, BeamStatus::Skipped { .. }));
    let done_count = success + warning + failed + skipped;
    let breakdown = crate::widgets::status_bar::StatusBreakdown { success, warning, failed, skipped };

    // Ligne 1 : toujours l'état + la barre (reste visible même en recherche).
    crate::widgets::status_bar::render_progress_line(f, footer[0], exec.done, done_count, total, &breakdown);

    // Ligne 2 : raccourcis, ou invite de recherche si une recherche est active.
    if search.is_active() {
        f.render_widget(search_bar(search), footer[1]);
    } else {
        crate::widgets::status_bar::render_hints_line(f, footer[1], exec.done);
    }

    if show_help {
        crate::widgets::help_popup::render_help_popup(
            f,
            area,
            crate::widgets::help_popup::HelpContext::Execution,
        );
    }
}

/// Barre de statut affichée pendant une recherche dans les logs.
fn search_bar(search: &LogSearch) -> Paragraph<'static> {
    let count = if search.query.is_empty() {
        String::new()
    } else if search.match_count() == 0 {
        "  [aucun résultat]".to_string()
    } else {
        format!("  [{}/{}]", search.current + 1, search.match_count())
    };
    let hint = if search.input_active {
        "   (Entrée valider, Esc annuler)"
    } else {
        "   [n/N] suivant/précédent  [Esc] effacer"
    };
    let text = format!(" /{}{}{} ", search.query, count, hint);
    Paragraph::new(text).style(Style::default().fg(Color::Yellow))
}
