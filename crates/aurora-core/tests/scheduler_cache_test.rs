use aurora_core::ast::{Beam, Run};
use aurora_core::scheduler::{BeamStatus, Scheduler, SchedulerEvent, SkipReason};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

fn local_executors() -> HashMap<String, Arc<dyn Executor>> {
    let mut m: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    m.insert("local".into(), Arc::new(LocalExecutor::new()));
    m
}

/// A beam that hashes `in.txt` and produces `out.txt`. The output is declared
/// relative to the Beamfile directory (resolved against `working_dir` by the
/// cache), so the existence check does not depend on the process cwd.
fn cached_beam(dir: &std::path::Path) -> Beam {
    let out = dir.join("out.txt");
    Beam {
        name: "build".to_string(),
        description: None,
        depends_on: vec![],
        inputs: vec!["in.txt".to_string()],
        outputs: vec!["out.txt".to_string()],
        variables: vec![],
        args: vec![],
        dir: None,
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands: vec![format!("echo done > {}", out.display())],
            executor: None,
        }),
        allow_failure: false,
    }
}

fn status_of<'a>(events: &'a [SchedulerEvent], name: &str) -> Option<&'a BeamStatus> {
    events.iter().rev().find_map(|e| match e {
        SchedulerEvent::BeamCompleted { name: n, status } if n == name => Some(status),
        _ => None,
    })
}

#[tokio::test]
async fn cache_hit_replays_recorded_output() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("in.txt"), "content-v1").unwrap();
    let working_dir = PathBuf::from(dir.path());
    let out = dir.path().join("out.txt");

    let beam = Beam {
        name: "build".to_string(),
        description: None,
        depends_on: vec![],
        inputs: vec!["in.txt".to_string()],
        outputs: vec!["out.txt".to_string()],
        variables: vec![],
        args: vec![],
        dir: None,
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands: vec![
                "echo hello-cache".to_string(),
                format!("echo done > {}", out.display()),
            ],
            executor: None,
        }),
        allow_failure: false,
    };

    // First run: executes and records stdout in the cache.
    let (tx, mut rx) = mpsc::channel(64);
    Scheduler::new(
        vec![beam.clone()],
        local_executors(),
        tx,
        None,
        working_dir.clone(),
        HashMap::new(),
    )
    .run("build", &[])
    .await
    .unwrap();
    while rx.try_recv().is_ok() {}

    // Second run: the cache hit must replay the recorded stdout, not just
    // report the Cached status.
    let (tx, mut rx) = mpsc::channel(64);
    Scheduler::new(
        vec![beam],
        local_executors(),
        tx,
        None,
        working_dir,
        HashMap::new(),
    )
    .run("build", &[])
    .await
    .unwrap();

    let mut replayed = vec![];
    let mut cached = false;
    while let Ok(e) = rx.try_recv() {
        match e {
            SchedulerEvent::BeamOutput {
                line,
                is_stderr: false,
                ..
            } => replayed.push(line),
            SchedulerEvent::BeamCompleted {
                status:
                    BeamStatus::Skipped {
                        reason: SkipReason::Cached,
                    },
                ..
            } => cached = true,
            _ => {}
        }
    }

    assert!(cached, "second run should be a cache hit");
    assert!(
        replayed.iter().any(|l| l == "hello-cache"),
        "cached stdout should be replayed, got {replayed:?}"
    );
}

#[tokio::test]
async fn cache_hit_on_second_run_and_bypassed_with_no_cache() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("in.txt"), "content-v1").unwrap();
    let working_dir = PathBuf::from(dir.path());

    // First run: executes, populates the cache.
    let (tx, mut rx) = mpsc::channel(64);
    let scheduler = Scheduler::new(
        vec![cached_beam(dir.path())],
        local_executors(),
        tx,
        None,
        working_dir.clone(),
        HashMap::new(),
    );
    scheduler.run("build", &[]).await.unwrap();
    let mut events = vec![];
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    assert!(
        matches!(
            status_of(&events, "build"),
            Some(BeamStatus::Success { cached: false, .. })
        ),
        "first run should execute, got {:?}",
        status_of(&events, "build")
    );

    // Second run with a fresh scheduler on the same directory: cache hit.
    let (tx, mut rx) = mpsc::channel(64);
    let scheduler = Scheduler::new(
        vec![cached_beam(dir.path())],
        local_executors(),
        tx,
        None,
        working_dir.clone(),
        HashMap::new(),
    );
    scheduler.run("build", &[]).await.unwrap();
    let mut events = vec![];
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    assert!(
        matches!(
            status_of(&events, "build"),
            Some(BeamStatus::Skipped {
                reason: SkipReason::Cached
            })
        ),
        "second run should be a cache hit, got {:?}",
        status_of(&events, "build")
    );

    // Third run with --no-cache: the cache is ignored, the beam runs again.
    let (tx, mut rx) = mpsc::channel(64);
    let scheduler = Scheduler::new(
        vec![cached_beam(dir.path())],
        local_executors(),
        tx,
        None,
        working_dir.clone(),
        HashMap::new(),
    )
    .without_cache();
    scheduler.run("build", &[]).await.unwrap();
    let mut events = vec![];
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    assert!(
        matches!(
            status_of(&events, "build"),
            Some(BeamStatus::Success { cached: false, .. })
        ),
        "no-cache run should execute again, got {:?}",
        status_of(&events, "build")
    );
}
