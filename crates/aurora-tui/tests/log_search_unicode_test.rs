//! Search highlighting must stay on char boundaries even when lowercasing
//! shifts byte offsets, so rendering never panics on Unicode log lines.

use aurora_tui::app::{BeamView, LogSearch, LogViewState};
use aurora_tui::execution::log_panel::render_log_panel;
use ratatui::{backend::TestBackend, Terminal};

#[test]
fn render_does_not_panic_on_unicode_search_highlight() {
    let mut beam = BeamView::new("b".to_string(), vec![]);
    // The Kelvin sign (3 bytes, lowercases to 1) and İ (2 bytes, lowercases
    // to 3) compensate so the lowercased line keeps the same byte length as
    // the original while its internal offsets shift. A byte range computed on
    // the lowercased text then falls inside a multibyte char of the original.
    beam.stdout = vec!["\u{212A}error \u{130}\u{130}".to_string()];

    let mut search = LogSearch::new();
    search.query = "error".to_string();
    search.recompute(&beam);

    let log_state = LogViewState::new(0);
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();

    terminal
        .draw(|f| render_log_panel(f, &beam, &log_state, Some(&search), f.area(), true))
        .expect("rendering a Unicode log line with a search match must not panic");
}
