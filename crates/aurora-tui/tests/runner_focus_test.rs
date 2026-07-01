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
    assert_eq!(exec.focus, FocusPanel::Logs, "Right -> logs");

    exec.handle_key(key(KeyCode::Left));
    assert_eq!(exec.focus, FocusPanel::Beams, "Left -> beams");

    // Idempotent: staying on the same panel does not break anything.
    exec.handle_key(key(KeyCode::Left));
    assert_eq!(exec.focus, FocusPanel::Beams);
    exec.handle_key(key(KeyCode::Right));
    exec.handle_key(key(KeyCode::Right));
    assert_eq!(exec.focus, FocusPanel::Logs);
}

#[test]
fn d_toggles_deps_panel() {
    let mut exec = ExecutionState::new(vec![
        ("build".to_string(), vec!["lint".to_string()]),
        ("lint".to_string(), vec![]),
    ]);
    // Hidden by default to preserve the usual beams/logs layout.
    assert!(!exec.show_deps);

    exec.handle_key(key(KeyCode::Char('d')));
    assert!(exec.show_deps, "d -> shows the dependencies");

    exec.handle_key(key(KeyCode::Char('d')));
    assert!(!exec.show_deps, "d -> hides the dependencies");
}

#[test]
fn d_does_not_change_focus() {
    let mut exec = ExecutionState::new(vec![("a".to_string(), vec![])]);
    assert_eq!(exec.focus, FocusPanel::Beams);
    exec.handle_key(key(KeyCode::Char('d')));
    assert_eq!(
        exec.focus,
        FocusPanel::Beams,
        "toggling deps does not touch the focus"
    );
}
