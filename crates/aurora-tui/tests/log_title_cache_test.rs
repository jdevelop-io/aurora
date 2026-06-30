use aurora_core::scheduler::{BeamStatus, SkipReason};
use aurora_tui::app::{BeamView, LogViewState};
use aurora_tui::execution::log_panel::render_log_panel;
use ratatui::{backend::TestBackend, Terminal};

/// Concatène tout le contenu textuel du buffer rendu, pour asserter sur ce qui
/// est réellement affiché (titre du panneau inclus).
fn rendered_text(beam: &BeamView) -> String {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let log_state = LogViewState::new(0);
    terminal
        .draw(|f| render_log_panel(f, beam, &log_state, None, f.area(), false))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let area = buf.area;
    let mut text = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            text.push_str(buf[(x, y)].symbol());
        }
    }
    text
}

#[test]
fn cached_beam_log_title_mentions_cache() {
    let mut beam = BeamView::new("build".to_string(), vec![]);
    beam.status = BeamStatus::Skipped {
        reason: SkipReason::Cached,
    };
    beam.stdout = vec!["compiled ok".to_string()];

    let text = rendered_text(&beam);
    assert!(
        text.contains("(cache)"),
        "le titre du panneau de logs devrait signaler les logs en cache, rendu :\n{text}"
    );
}

#[test]
fn running_beam_log_title_has_no_cache_marker() {
    let mut beam = BeamView::new("build".to_string(), vec![]);
    beam.status = BeamStatus::Running;
    beam.stdout = vec!["compiling...".to_string()];

    let text = rendered_text(&beam);
    assert!(
        !text.contains("(cache)"),
        "un beam en cours ne devrait pas être marqué comme en cache, rendu :\n{text}"
    );
}
