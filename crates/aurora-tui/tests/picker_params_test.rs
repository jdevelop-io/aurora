use aurora_tui::app::{PickerAction, PickerBeam, PickerState};
use crossterm::event::{KeyCode, KeyEvent};

fn beams() -> Vec<PickerBeam> {
    vec![
        PickerBeam {
            name: "fmt".to_string(),
            description: None,
            depends_on: vec![],
            signature: "fmt".to_string(),
            requires_args: false,
        },
        PickerBeam {
            name: "deploy".to_string(),
            description: None,
            depends_on: vec![],
            signature: "deploy <version> [env=staging]".to_string(),
            requires_args: true,
        },
    ]
}

#[test]
fn enter_on_a_parameterized_beam_sets_a_notice_instead_of_launching() {
    let mut state = PickerState::new(beams());
    state.selected = 1;
    let action = state.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(action, None);
    let notice = state.notice.as_deref().unwrap();
    assert!(
        notice.contains("aurora deploy <version> [env=staging]"),
        "got: {notice}"
    );
}

#[test]
fn space_cannot_check_a_parameterized_beam() {
    let mut state = PickerState::new(beams());
    state.selected = 1;
    state.handle_key(KeyEvent::from(KeyCode::Char(' ')));
    assert!(state.checked.iter().all(|&c| !c));
    assert!(state.notice.is_some());
}

#[test]
fn plain_beams_still_launch() {
    let mut state = PickerState::new(beams());
    state.selected = 0;
    let action = state.handle_key(KeyEvent::from(KeyCode::Enter));
    assert_eq!(action, Some(PickerAction::Launch(vec!["fmt".to_string()])));
}
