use aurora_tui::app::{ExecutionState, FocusPanel};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn left_right_switch_focus() {
    let mut exec = ExecutionState::new(vec![("a".to_string(), vec![])]);
    assert_eq!(exec.focus, FocusPanel::Beams);

    exec.handle_key(key(KeyCode::Right));
    assert_eq!(exec.focus, FocusPanel::Logs, "Droite -> logs");

    exec.handle_key(key(KeyCode::Left));
    assert_eq!(exec.focus, FocusPanel::Beams, "Gauche -> beams");

    // Idempotent : rester sur le même panneau ne casse rien.
    exec.handle_key(key(KeyCode::Left));
    assert_eq!(exec.focus, FocusPanel::Beams);
    exec.handle_key(key(KeyCode::Right));
    exec.handle_key(key(KeyCode::Right));
    assert_eq!(exec.focus, FocusPanel::Logs);
}
