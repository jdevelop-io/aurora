use aurora_core::scheduler::{BeamStatus, SchedulerEvent};
use aurora_tui::app::{ExecutionAction, ExecutionState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn make_state() -> ExecutionState {
    // test   (Success)
    //   └── build (Failed)
    //         └── deploy (Failed/Cancelled)  ← selected
    ExecutionState::new(vec![
        ("test".to_string(), vec![]),
        ("build".to_string(), vec!["test".to_string()]),
        ("deploy".to_string(), vec!["build".to_string()]),
    ])
}

fn set_done(state: &mut ExecutionState) {
    state.done = Some(false);
    // test = Success
    state.apply_event(SchedulerEvent::BeamStarted {
        name: "test".to_string(),
    });
    state.apply_event(SchedulerEvent::BeamCompleted {
        name: "test".to_string(),
        status: BeamStatus::Success {
            duration: Duration::from_secs(1),
            cached: false,
        },
    });
    // build = Failed
    state.apply_event(SchedulerEvent::BeamStarted {
        name: "build".to_string(),
    });
    state.apply_event(SchedulerEvent::BeamCompleted {
        name: "build".to_string(),
        status: BeamStatus::Failed {
            exit_code: 1,
            duration: Duration::from_secs(1),
        },
    });
    // deploy = Cancelled
    state.apply_event(SchedulerEvent::BeamCompleted {
        name: "deploy".to_string(),
        status: BeamStatus::Cancelled,
    });
}

#[test]
fn compute_rerun_returns_failed_and_cancelled_deps() {
    let mut state = make_state();
    set_done(&mut state);
    state.selected = 2; // deploy

    let (root, to_rerun, pre_success) = state.compute_rerun(2);

    assert_eq!(root, "deploy");
    // build (Failed) and deploy (Cancelled) must be in to_rerun
    assert!(to_rerun.contains(&"build".to_string()));
    assert!(to_rerun.contains(&"deploy".to_string()));
    // test (Success) must be in pre_success
    assert!(pre_success.contains(&"test".to_string()));
    // test must NOT be in to_rerun
    assert!(!to_rerun.contains(&"test".to_string()));
}

#[test]
fn reset_for_rerun_clears_beam_state() {
    let mut state = make_state();
    set_done(&mut state);

    // Add a few logs to build
    state.beams[1].stdout.push("some output".to_string());

    state.reset_for_rerun(&["build".to_string(), "deploy".to_string()]);

    // build and deploy must be Pending
    assert!(matches!(state.beams[1].status, BeamStatus::Pending));
    assert!(matches!(state.beams[2].status, BeamStatus::Pending));
    // stdout cleared
    assert!(state.beams[1].stdout.is_empty());
    // done reset
    assert!(state.done.is_none());
    // test unchanged (Success)
    assert!(matches!(state.beams[0].status, BeamStatus::Success { .. }));
}

#[test]
fn r_key_returns_rerun_action_when_done_and_failed() {
    let mut state = make_state();
    set_done(&mut state);
    state.selected = 2; // deploy (Cancelled)

    let action = state.handle_key(key(KeyCode::Char('r')));

    assert!(matches!(action, Some(ExecutionAction::Rerun { .. })));
    if let Some(ExecutionAction::Rerun { root, pre_success }) = action {
        assert_eq!(root, "deploy");
        assert!(pre_success.contains(&"test".to_string()));
    }
}

#[test]
fn r_key_ignored_when_exec_still_running() {
    let mut state = make_state();
    // done = None → still running
    state.selected = 1; // build

    let action = state.handle_key(key(KeyCode::Char('r')));
    assert!(action.is_none());
}

#[test]
fn r_key_works_on_success_beam() {
    let mut state = make_state();
    set_done(&mut state);
    state.selected = 0; // test (Success)

    let action = state.handle_key(key(KeyCode::Char('r')));
    assert!(matches!(action, Some(ExecutionAction::Rerun { .. })));
    if let Some(ExecutionAction::Rerun { root, pre_success }) = action {
        assert_eq!(root, "test");
        // test is the root → in to_rerun, not in pre_success
        assert!(!pre_success.contains(&"test".to_string()));
    }
}
