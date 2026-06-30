use aurora_tui::app::PickerState;
use aurora_tui::picker::view::render_picker;
use ratatui::{backend::TestBackend, Terminal};

fn state() -> PickerState {
    PickerState::new(vec![
        (
            "build".to_string(),
            Some("compile".to_string()),
            vec!["fmt".to_string()],
        ),
        ("test".to_string(), None, vec![]),
        ("fmt".to_string(), None, vec![]),
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
