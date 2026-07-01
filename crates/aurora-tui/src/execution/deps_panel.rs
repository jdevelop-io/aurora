use crate::app::ExecutionState;
use crate::deps_panel::{render_deps, render_panel};
use ratatui::{layout::Rect, text::Line, Frame};

/// Dependency panel for the beam selected in the runner. Delegates to the
/// shared `crate::deps_panel` renderer, like the picker; only the selection
/// source differs.
pub fn render_deps_panel(f: &mut Frame, state: &ExecutionState, area: Rect) {
    if let Some(beam) = state.beams.get(state.selected) {
        let dependents: Vec<&str> = state
            .beams
            .iter()
            .filter(|b| b.depends_on.iter().any(|d| d == &beam.name))
            .map(|b| b.name.as_str())
            .collect();
        render_deps(f, area, &beam.name, &beam.depends_on, &dependents);
    } else {
        render_panel(f, area, vec![Line::from("")]);
    }
}
