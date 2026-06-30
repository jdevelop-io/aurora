use aurora_tui::app::{ExecutionAction, ExecutionState, FocusPanel, PickerAction, PickerState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn picker_enter_with_selection_returns_beam_name() {
    let mut picker = PickerState::new(vec![
        ("build".to_string(), None, vec![]),
        ("test".to_string(), None, vec![]),
    ]);
    let result = picker.handle_key(key(KeyCode::Enter));
    assert_eq!(
        result,
        Some(PickerAction::Launch(vec!["build".to_string()]))
    );
}

#[test]
fn picker_esc_returns_quit() {
    let mut picker = PickerState::new(vec![("build".to_string(), None, vec![])]);
    let result = picker.handle_key(key(KeyCode::Esc));
    assert_eq!(result, Some(PickerAction::Quit));
}

#[test]
fn execution_q_returns_quit() {
    let mut exec = ExecutionState::new(vec![
        ("build".to_string(), vec![]),
        ("test".to_string(), vec![]),
    ]);
    let result = exec.handle_key(key(KeyCode::Char('q')));
    assert_eq!(result, Some(ExecutionAction::Quit));
}

#[test]
fn execution_enter_opens_log_view() {
    let mut exec = ExecutionState::new(vec![("build".to_string(), vec![])]);
    let result = exec.handle_key(key(KeyCode::Enter));
    assert_eq!(result, Some(ExecutionAction::OpenLogView { beam_index: 0 }));
}

#[test]
fn default_focus_is_beams() {
    let state = ExecutionState::new(vec![("a".to_string(), vec![]), ("b".to_string(), vec![])]);
    assert_eq!(state.focus, FocusPanel::Beams);
}

#[test]
fn tab_switches_focus_from_beams_to_logs() {
    let mut state = ExecutionState::new(vec![("a".to_string(), vec![])]);
    state.handle_key(key(KeyCode::Tab));
    assert_eq!(state.focus, FocusPanel::Logs);
}

#[test]
fn tab_switches_focus_from_logs_to_beams() {
    let mut state = ExecutionState::new(vec![("a".to_string(), vec![])]);
    state.focus = FocusPanel::Logs;
    state.handle_key(key(KeyCode::Tab));
    assert_eq!(state.focus, FocusPanel::Beams);
}

#[test]
fn beam_view_stores_depends_on() {
    use aurora_tui::app::BeamView;
    let beam = BeamView::new("deploy".to_string(), vec!["build".to_string()]);
    assert_eq!(beam.depends_on, vec!["build".to_string()]);
}
