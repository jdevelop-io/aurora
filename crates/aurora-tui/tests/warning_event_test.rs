use aurora_core::scheduler::SchedulerEvent;
use aurora_tui::app::ExecutionState;

/// A `Warning` event is routed to its beam's stderr log so it is visible in the
/// log pane, without affecting the beam's status.
#[test]
fn warning_event_is_appended_to_the_beam_stderr_log() {
    let mut state = ExecutionState::new(vec![("build".to_string(), vec![])]);

    state.apply_event(SchedulerEvent::Warning {
        name: "build".to_string(),
        message: "input pattern matched no files: missing/*.rs".to_string(),
    });

    let beam = state.beams.iter().find(|b| b.name == "build").unwrap();
    assert_eq!(
        beam.stderr,
        vec!["warning: input pattern matched no files: missing/*.rs".to_string()]
    );
}
