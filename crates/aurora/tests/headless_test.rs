use aurora::headless::run_headless;
use aurora_core::scheduler::{BeamStatus, SchedulerEvent, SkipReason};
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::test]
async fn streams_prefixed_output_routes_stderr_and_builds_recap() {
    let (tx, rx) = mpsc::channel(16);
    let beams = vec!["build".to_string(), "test".to_string()]; // largeur = 5

    tx.send(SchedulerEvent::BeamStarted {
        name: "build".into(),
    })
    .await
    .unwrap();
    tx.send(SchedulerEvent::BeamOutput {
        name: "build".into(),
        line: "compiling".into(),
        is_stderr: false,
    })
    .await
    .unwrap();
    tx.send(SchedulerEvent::BeamOutput {
        name: "test".into(),
        line: "boom".into(),
        is_stderr: true,
    })
    .await
    .unwrap();
    tx.send(SchedulerEvent::BeamCompleted {
        name: "build".into(),
        status: BeamStatus::Success {
            duration: Duration::from_millis(4200),
            cached: false,
        },
    })
    .await
    .unwrap();
    tx.send(SchedulerEvent::BeamCompleted {
        name: "test".into(),
        status: BeamStatus::Failed {
            exit_code: 1,
            duration: Duration::from_millis(1800),
        },
    })
    .await
    .unwrap();
    tx.send(SchedulerEvent::AllDone { success: false })
        .await
        .unwrap();
    drop(tx);

    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let success = run_headless(&beams, false, rx, &mut out, &mut err)
        .await
        .unwrap();
    let out = String::from_utf8(out).unwrap();
    let err = String::from_utf8(err).unwrap();

    assert!(!success);
    assert!(out.contains("[build] compiling"), "stdout prefix:\n{out}");
    // "test" est complété à la largeur de "build" (5) → "[test ]"
    assert!(
        err.contains("[test ] boom"),
        "stderr prefix/padding:\n{err}"
    );
    assert!(!out.contains("boom"), "stderr line must not leak to stdout");
    assert!(out.contains("[OK]"), "recap ok marker:\n{out}");
    assert!(out.contains("4.2s"), "recap duration:\n{out}");
    assert!(out.contains("[FAIL]"), "recap fail marker:\n{out}");
    assert!(out.contains("exit 1"), "recap exit code:\n{out}");
    assert!(out.contains("Done: 1 ok, 1 failed"), "summary:\n{out}");
}

#[tokio::test]
async fn allow_failure_counts_as_ok_and_overall_can_be_true() {
    let (tx, rx) = mpsc::channel(16);
    let beams = vec!["deploy".to_string()];

    tx.send(SchedulerEvent::BeamCompleted {
        name: "deploy".into(),
        status: BeamStatus::FailedAllowed {
            exit_code: 2,
            duration: Duration::from_millis(300),
        },
    })
    .await
    .unwrap();
    tx.send(SchedulerEvent::AllDone { success: true })
        .await
        .unwrap();
    drop(tx);

    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    let success = run_headless(&beams, false, rx, &mut out, &mut err)
        .await
        .unwrap();
    let out = String::from_utf8(out).unwrap();

    assert!(success);
    assert!(out.contains("[WARN]"), "warn marker:\n{out}");
    assert!(out.contains("(allowed)"), "allowed note:\n{out}");
    assert!(out.contains("Done: 1 ok, 0 failed"), "summary:\n{out}");
}

#[tokio::test]
async fn skipped_and_cancelled_markers_and_color_toggle() {
    let (tx, rx) = mpsc::channel(16);
    let beams = vec!["lint".to_string(), "deploy".to_string()];

    tx.send(SchedulerEvent::BeamCompleted {
        name: "lint".into(),
        status: BeamStatus::Skipped {
            reason: SkipReason::Cached,
        },
    })
    .await
    .unwrap();
    tx.send(SchedulerEvent::BeamCompleted {
        name: "deploy".into(),
        status: BeamStatus::Cancelled,
    })
    .await
    .unwrap();
    tx.send(SchedulerEvent::AllDone { success: true })
        .await
        .unwrap();
    drop(tx);

    let mut out: Vec<u8> = Vec::new();
    let mut err: Vec<u8> = Vec::new();
    run_headless(&beams, true, rx, &mut out, &mut err)
        .await
        .unwrap();
    let out = String::from_utf8(out).unwrap();

    assert!(out.contains("[SKIP]"), "skip marker:\n{out}");
    assert!(out.contains("cached"), "skip reason:\n{out}");
    assert!(out.contains("[CANC]"), "cancelled marker:\n{out}");
    assert!(out.contains("cancelled"), "cancelled reason:\n{out}");
    // use_color = true encadre les marqueurs avec des séquences ANSI
    assert!(out.contains("\u{1b}["), "ansi escape present:\n{out:?}");
    assert!(out.contains("\u{1b}[0m"), "ansi reset present:\n{out:?}");
}
