use crate::app::PickerState;
use crate::deps_panel::{render_deps, render_panel};
use ratatui::{layout::Rect, text::Line, Frame};

pub fn render_deps_panel(f: &mut Frame, state: &PickerState, area: Rect) {
    let filtered = state.filtered();
    if let Some((_, beam, _)) = filtered.get(state.selected) {
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
