use aurora_core::scheduler::{BeamStatus, SchedulerEvent};
use aurora_tui::app::ExecutionState;
use std::time::Duration;

fn make_state() -> ExecutionState {
    ExecutionState::new(vec![
        ("test".to_string(), vec![]),
        ("build".to_string(), vec!["test".to_string()]),
        ("deploy".to_string(), vec!["build".to_string()]),
    ])
}

fn complete(state: &mut ExecutionState, name: &str, status: BeamStatus) {
    state.apply_event(SchedulerEvent::BeamCompleted {
        name: name.to_string(),
        status,
    });
}

#[test]
fn selects_first_failed_beam() {
    let mut state = make_state();
    complete(
        &mut state,
        "test",
        BeamStatus::Success {
            duration: Duration::from_secs(1),
            cached: false,
        },
    );
    complete(
        &mut state,
        "build",
        BeamStatus::Failed {
            exit_code: 1,
            duration: Duration::from_secs(1),
        },
    );
    complete(
        &mut state,
        "deploy",
        BeamStatus::Failed {
            exit_code: 1,
            duration: Duration::from_secs(1),
        },
    );
    state.selected = 0;

    let found = state.select_first_failed();

    assert!(found);
    assert_eq!(state.selected, 1); // build, le premier Failed
}

#[test]
fn ignores_cancelled_and_success() {
    let mut state = make_state();
    complete(
        &mut state,
        "test",
        BeamStatus::Success {
            duration: Duration::from_secs(1),
            cached: false,
        },
    );
    complete(&mut state, "build", BeamStatus::Cancelled);
    complete(
        &mut state,
        "deploy",
        BeamStatus::Failed {
            exit_code: 1,
            duration: Duration::from_secs(1),
        },
    );
    state.selected = 0;

    let found = state.select_first_failed();

    assert!(found);
    assert_eq!(state.selected, 2); // deploy, premier Failed (build est Cancelled)
}

#[test]
fn does_not_move_when_no_failed() {
    let mut state = make_state();
    complete(
        &mut state,
        "test",
        BeamStatus::Success {
            duration: Duration::from_secs(1),
            cached: false,
        },
    );
    complete(&mut state, "build", BeamStatus::Cancelled);
    complete(&mut state, "deploy", BeamStatus::Cancelled);
    state.selected = 1;

    let found = state.select_first_failed();

    assert!(!found);
    assert_eq!(state.selected, 1); // inchangé
}
