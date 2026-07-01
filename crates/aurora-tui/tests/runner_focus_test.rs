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

#[test]
fn d_toggles_deps_panel() {
    let mut exec = ExecutionState::new(vec![
        ("build".to_string(), vec!["lint".to_string()]),
        ("lint".to_string(), vec![]),
    ]);
    // Masqué par défaut pour préserver le layout beams/logs habituel.
    assert!(!exec.show_deps);

    exec.handle_key(key(KeyCode::Char('d')));
    assert!(exec.show_deps, "d -> affiche les dépendances");

    exec.handle_key(key(KeyCode::Char('d')));
    assert!(!exec.show_deps, "d -> masque les dépendances");
}

#[test]
fn d_does_not_change_focus() {
    let mut exec = ExecutionState::new(vec![("a".to_string(), vec![])]);
    assert_eq!(exec.focus, FocusPanel::Beams);
    exec.handle_key(key(KeyCode::Char('d')));
    assert_eq!(exec.focus, FocusPanel::Beams, "le toggle deps ne touche pas au focus");
}
