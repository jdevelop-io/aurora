use aurora_tui::app::{PickerBeam, PickerState};
use aurora_tui::picker::view::render_picker;
use ratatui::{backend::TestBackend, Terminal};

fn beam(name: &str, description: Option<&str>, depends_on: Vec<&str>) -> PickerBeam {
    PickerBeam {
        name: name.to_string(),
        description: description.map(str::to_string),
        depends_on: depends_on.into_iter().map(str::to_string).collect(),
        signature: name.to_string(),
        requires_args: false,
    }
}

fn state() -> PickerState {
    PickerState::new(vec![
        beam("build", Some("compile"), vec!["fmt"]),
        beam("test", None, vec![]),
        beam("fmt", None, vec![]),
    ])
}

#[test]
fn render_picker_normal_size() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let st = state();
    terminal.draw(|f| render_picker(f, &st)).unwrap();
}

#[test]
fn render_picker_small_size() {
    let backend = TestBackend::new(20, 5);
    let mut terminal = Terminal::new(backend).unwrap();
    let st = state();
    terminal.draw(|f| render_picker(f, &st)).unwrap();
}

#[test]
fn render_picker_deps_open() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut st = state();
    st.show_deps = true;
    terminal.draw(|f| render_picker(f, &st)).unwrap();
}

/// A row with a param signature and a pending notice must render without
/// panicking: covers the dimmed/signature row and the footer notice line
/// added for parameterized beams.
#[test]
fn render_picker_with_parameterized_beam_and_notice() {
    let backend = TestBackend::new(80, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let mut st = PickerState::new(vec![
        beam("fmt", None, vec![]),
        PickerBeam {
            name: "deploy".to_string(),
            description: None,
            depends_on: vec![],
            signature: "deploy <version> [env=staging]".to_string(),
            requires_args: true,
        },
    ]);
    st.selected = 1;
    st.notice = Some(
        "'deploy' requires arguments: run `aurora deploy <version> [env=staging]`".to_string(),
    );
    terminal.draw(|f| render_picker(f, &st)).unwrap();
}
