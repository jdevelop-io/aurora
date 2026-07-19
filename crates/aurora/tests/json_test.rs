//! Reporter-level tests: feed a scripted SchedulerEvent sequence into the
//! channel, run JsonReporter over an in-memory buffer, and assert every line
//! is a well-formed event carrying the right fields.

use std::time::Duration;

use aurora::json::JsonReporter;
use aurora::reporter::Reporter;
use aurora_core::events::{BeamStatus, SchedulerEvent, SkipReason};
use serde_json::Value;
use tokio::sync::mpsc;

/// Runs JsonReporter over a scripted event sequence, returns (parsed lines, success).
async fn run_reporter(
    target: &str,
    beams: Vec<String>,
    events: Vec<SchedulerEvent>,
) -> (Vec<Value>, bool) {
    let (tx, rx) = mpsc::channel(64);
    for event in events {
        tx.send(event).await.unwrap();
    }
    drop(tx);
    let mut buf: Vec<u8> = Vec::new();
    let success = {
        let mut reporter = JsonReporter::new(target.to_string(), beams, &mut buf);
        reporter.run(rx).await.unwrap()
    };
    let text = String::from_utf8(buf).unwrap();
    let lines: Vec<Value> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            serde_json::from_str(l).unwrap_or_else(|e| panic!("line is not valid JSON: {l}\n{e}"))
        })
        .collect();
    (lines, success)
}

#[tokio::test]
async fn emits_run_started_first_and_run_completed_last() {
    let (lines, success) = run_reporter(
        "check",
        vec!["fmt".into(), "check".into()],
        vec![
            SchedulerEvent::BeamStarted { name: "fmt".into() },
            SchedulerEvent::BeamCompleted {
                name: "fmt".into(),
                status: BeamStatus::Success {
                    duration: Duration::from_millis(412),
                    cached: false,
                },
            },
            SchedulerEvent::AllDone { success: true },
        ],
    )
    .await;

    assert_eq!(lines.first().unwrap()["event"], "run_started");
    assert_eq!(lines.first().unwrap()["schema"], 1);
    assert_eq!(lines.first().unwrap()["target"], "check");
    assert_eq!(
        lines.first().unwrap()["beams"],
        serde_json::json!(["fmt", "check"])
    );
    assert_eq!(lines.last().unwrap()["event"], "run_completed");
    assert_eq!(lines.last().unwrap()["success"], true);
    assert!(success);
}

#[tokio::test]
async fn beam_completed_success_carries_cached_and_duration() {
    let (lines, _) = run_reporter(
        "b",
        vec!["b".into()],
        vec![
            SchedulerEvent::BeamCompleted {
                name: "b".into(),
                status: BeamStatus::Success {
                    duration: Duration::from_millis(412),
                    cached: false,
                },
            },
            SchedulerEvent::AllDone { success: true },
        ],
    )
    .await;
    let completed = lines
        .iter()
        .find(|l| l["event"] == "beam_completed")
        .unwrap();
    assert_eq!(completed["status"], "success");
    assert_eq!(completed["cached"], false);
    assert_eq!(completed["duration_ms"], 412);
}

#[tokio::test]
async fn cached_beam_omits_duration() {
    let (lines, _) = run_reporter(
        "b",
        vec!["b".into()],
        vec![
            SchedulerEvent::BeamCompleted {
                name: "b".into(),
                status: BeamStatus::Success {
                    duration: Duration::from_millis(0),
                    cached: true,
                },
            },
            SchedulerEvent::AllDone { success: true },
        ],
    )
    .await;
    let completed = lines
        .iter()
        .find(|l| l["event"] == "beam_completed")
        .unwrap();
    assert_eq!(completed["status"], "success");
    assert_eq!(completed["cached"], true);
    assert!(
        completed.get("duration_ms").is_none(),
        "cached beam must omit duration_ms"
    );
}

#[tokio::test]
async fn failed_beam_carries_exit_code() {
    let (lines, success) = run_reporter(
        "b",
        vec!["b".into()],
        vec![
            SchedulerEvent::BeamCompleted {
                name: "b".into(),
                status: BeamStatus::Failed {
                    exit_code: 3,
                    duration: Duration::from_millis(50),
                },
            },
            SchedulerEvent::AllDone { success: false },
        ],
    )
    .await;
    let completed = lines
        .iter()
        .find(|l| l["event"] == "beam_completed")
        .unwrap();
    assert_eq!(completed["status"], "failed");
    assert_eq!(completed["exit_code"], 3);
    assert_eq!(completed["duration_ms"], 50);
    assert!(!success);
}

#[tokio::test]
async fn skipped_beam_carries_reason() {
    let (lines, _) = run_reporter(
        "b",
        vec!["b".into()],
        vec![
            SchedulerEvent::BeamCompleted {
                name: "b".into(),
                status: BeamStatus::Skipped {
                    reason: SkipReason::ConditionNotMet,
                },
            },
            SchedulerEvent::AllDone { success: true },
        ],
    )
    .await;
    let completed = lines
        .iter()
        .find(|l| l["event"] == "beam_completed")
        .unwrap();
    assert_eq!(completed["status"], "skipped");
    assert_eq!(completed["reason"], "condition_not_met");
}

#[tokio::test]
async fn beam_output_carries_stream_and_line() {
    let (lines, _) = run_reporter(
        "b",
        vec!["b".into()],
        vec![
            SchedulerEvent::BeamOutput {
                name: "b".into(),
                line: "hello".into(),
                is_stderr: false,
            },
            SchedulerEvent::BeamOutput {
                name: "b".into(),
                line: "oops".into(),
                is_stderr: true,
            },
            SchedulerEvent::AllDone { success: true },
        ],
    )
    .await;
    let outs: Vec<_> = lines
        .iter()
        .filter(|l| l["event"] == "beam_output")
        .collect();
    assert_eq!(outs[0]["stream"], "stdout");
    assert_eq!(outs[0]["line"], "hello");
    assert_eq!(outs[1]["stream"], "stderr");
    assert_eq!(outs[1]["line"], "oops");
}

#[tokio::test]
async fn failed_allowed_beam_carries_exit_code_and_duration() {
    let (lines, success) = run_reporter(
        "b",
        vec!["b".into()],
        vec![
            SchedulerEvent::BeamCompleted {
                name: "b".into(),
                status: BeamStatus::FailedAllowed {
                    exit_code: 2,
                    duration: Duration::from_millis(70),
                },
            },
            SchedulerEvent::AllDone { success: true },
        ],
    )
    .await;
    let completed = lines
        .iter()
        .find(|l| l["event"] == "beam_completed")
        .unwrap();
    assert_eq!(completed["status"], "failed_allowed");
    assert_eq!(completed["exit_code"], 2);
    assert_eq!(completed["duration_ms"], 70);
    // allow_failure counts as success for the run outcome.
    assert!(success);
}

#[tokio::test]
async fn cancelled_beam_omits_extra_fields() {
    let (lines, _) = run_reporter(
        "b",
        vec!["b".into()],
        vec![
            SchedulerEvent::BeamCompleted {
                name: "b".into(),
                status: BeamStatus::Cancelled,
            },
            SchedulerEvent::AllDone { success: false },
        ],
    )
    .await;
    let completed = lines
        .iter()
        .find(|l| l["event"] == "beam_completed")
        .unwrap();
    assert_eq!(completed["status"], "cancelled");
    // A cancelled beam carries no exit_code, reason, cached, or duration.
    assert!(
        completed.get("exit_code").is_none(),
        "cancelled has no exit_code"
    );
    assert!(completed.get("reason").is_none(), "cancelled has no reason");
    assert!(completed.get("cached").is_none(), "cancelled has no cached");
    assert!(
        completed.get("duration_ms").is_none(),
        "cancelled has no duration_ms"
    );
}

#[tokio::test]
async fn emits_a_warning_event_for_a_dead_input_pattern() {
    let (lines, _success) = run_reporter(
        "build",
        vec!["build".into()],
        vec![
            SchedulerEvent::Warning {
                name: "build".into(),
                message: "input pattern matched no files: missing/*.rs".into(),
            },
            SchedulerEvent::AllDone { success: true },
        ],
    )
    .await;

    let warning = lines.iter().find(|l| l["event"] == "warning").unwrap();
    assert_eq!(warning["beam"], "build");
    assert_eq!(
        warning["message"],
        "input pattern matched no files: missing/*.rs"
    );
}
