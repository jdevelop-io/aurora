//! The execution sidebar lists every declared beam, including those the run
//! could not instantiate (a beam with a required param has no default
//! instance). Such beams are shown but cannot be launched from the sidebar:
//! pressing `r` on one surfaces a notice instead of spawning an empty run.

use aurora_core::scheduler::{BeamStatus, SchedulerEvent};
use aurora_tui::app::{ExecutionAction, ExecutionState};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use std::time::Duration;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn make_state() -> ExecutionState {
    ExecutionState::new(vec![
        ("lint".to_string(), vec![]),
        ("deploy".to_string(), vec![]),
    ])
}

#[test]
fn beams_are_launchable_by_default() {
    let state = make_state();
    assert!(state.is_launchable("lint"));
    assert!(state.is_launchable("deploy"));
}

#[test]
fn set_non_launchable_marks_the_listed_beams() {
    let mut state = make_state();
    state.set_non_launchable(["deploy".to_string()]);
    assert!(state.is_launchable("lint"));
    assert!(!state.is_launchable("deploy"));
}

#[test]
fn rerun_on_a_non_launchable_beam_is_refused_with_a_notice() {
    let mut state = make_state();
    state.set_non_launchable(["deploy".to_string()]);
    // Give `deploy` a rerun-eligible status so that, without the guard, `r`
    // would return a Rerun action.
    state.done = Some(true);
    state.apply_event(SchedulerEvent::BeamCompleted {
        name: "deploy".to_string(),
        status: BeamStatus::Success {
            duration: Duration::from_secs(1),
            cached: false,
        },
    });
    state.selected = 1; // deploy

    let action = state.handle_key(key(KeyCode::Char('r')));

    assert_eq!(action, None, "a non-launchable beam must not rerun");
    assert!(
        state.notice.is_some(),
        "the refusal must be explained by a notice"
    );
}

#[test]
fn rerun_on_a_launchable_beam_still_works() {
    let mut state = make_state();
    state.set_non_launchable(["deploy".to_string()]);
    state.done = Some(true);
    state.apply_event(SchedulerEvent::BeamCompleted {
        name: "lint".to_string(),
        status: BeamStatus::Success {
            duration: Duration::from_secs(1),
            cached: false,
        },
    });
    state.selected = 0; // lint (launchable)

    let action = state.handle_key(key(KeyCode::Char('r')));

    assert!(matches!(action, Some(ExecutionAction::Rerun { .. })));
    assert!(state.notice.is_none());
}
