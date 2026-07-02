use aurora_core::ast::{Beam, Run};
use aurora_core::scheduler::{BeamStatus, Scheduler, SchedulerEvent};
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

fn output_lines(events: &[SchedulerEvent], beam: &str) -> Vec<String> {
    events
        .iter()
        .filter_map(|e| match e {
            SchedulerEvent::BeamOutput { name, line, .. } if name == beam => Some(line.clone()),
            _ => None,
        })
        .collect()
}

/// A beam whose `dir` points at a subdirectory runs its commands there: a
/// relative `cat` only resolves when the process cwd is that subdirectory.
#[tokio::test]
async fn dir_sets_command_working_directory() {
    let root = tempfile::tempdir().unwrap();
    let pkg = root.path().join("pkg");
    std::fs::create_dir(&pkg).unwrap();
    std::fs::write(pkg.join("marker.txt"), "in-pkg").unwrap();

    let beam = Beam {
        name: "build".to_string(),
        description: None,
        depends_on: vec![],
        inputs: vec![],
        outputs: vec![],
        dir: Some("pkg".to_string()),
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands: vec!["cat marker.txt".to_string()],
            executor: None,
        }),
        allow_failure: false,
    };

    let (tx, mut rx) = mpsc::channel(64);
    let scheduler = Scheduler::new(
        vec![beam],
        local_executors(),
        tx,
        None,
        PathBuf::from(root.path()),
        HashMap::new(),
    );
    scheduler.run("build", &[]).await.unwrap();

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() {
        events.push(evt);
    }

    assert!(
        events.iter().any(|e| matches!(
            e,
            SchedulerEvent::BeamCompleted {
                name,
                status: BeamStatus::Success { .. },
            } if name == "build"
        )),
        "beam should succeed"
    );
    assert!(
        output_lines(&events, "build").iter().any(|l| l == "in-pkg"),
        "cat should read pkg/marker.txt relative to dir"
    );
}

/// A beam whose `dir` does not exist fails with a clear error rather than a
/// raw shell "cannot cd" or a confusing cache miss.
#[tokio::test]
async fn missing_dir_fails_beam_clearly() {
    let root = tempfile::tempdir().unwrap();

    let beam = Beam {
        name: "build".to_string(),
        description: None,
        depends_on: vec![],
        inputs: vec![],
        outputs: vec![],
        dir: Some("does-not-exist".to_string()),
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands: vec!["echo hi".to_string()],
            executor: None,
        }),
        allow_failure: false,
    };

    let (tx, mut rx) = mpsc::channel(64);
    let scheduler = Scheduler::new(
        vec![beam],
        local_executors(),
        tx,
        None,
        PathBuf::from(root.path()),
        HashMap::new(),
    );
    scheduler.run("build", &[]).await.unwrap();

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() {
        events.push(evt);
    }

    assert!(
        events.iter().any(|e| matches!(
            e,
            SchedulerEvent::BeamCompleted {
                name,
                status: BeamStatus::Failed { .. },
            } if name == "build"
        )),
        "missing dir should fail the beam"
    );
    assert!(
        output_lines(&events, "build").iter().any(|l| l
            .contains("working directory does not exist")
            && l.contains("does-not-exist")),
        "error should name the missing directory"
    );
}

/// A relative `input` is resolved under `dir`: an unchanged file inside `dir`
/// yields a cache-skip on rerun, proving inputs are hashed package-locally.
#[tokio::test]
async fn dir_scopes_input_hashing() {
    use aurora_core::scheduler::SkipReason;

    let root = tempfile::tempdir().unwrap();
    let pkg = root.path().join("pkg");
    std::fs::create_dir(&pkg).unwrap();
    std::fs::write(pkg.join("in.txt"), "v1").unwrap();

    let make = || Beam {
        name: "build".to_string(),
        description: None,
        depends_on: vec![],
        inputs: vec!["in.txt".to_string()],
        outputs: vec!["out.txt".to_string()],
        dir: Some("pkg".to_string()),
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands: vec!["echo done > out.txt".to_string()],
            executor: None,
        }),
        allow_failure: false,
    };

    // First run populates the cache (input read from pkg/in.txt, output
    // written to pkg/out.txt because cwd == pkg).
    let (tx, mut rx) = mpsc::channel(64);
    let scheduler = Scheduler::new(
        vec![make()],
        local_executors(),
        tx,
        None,
        PathBuf::from(root.path()),
        HashMap::new(),
    );
    scheduler.run("build", &[]).await.unwrap();
    while rx.try_recv().is_ok() {}

    // Second run with the input unchanged: cache hit -> Skipped(Cached).
    let (tx, mut rx) = mpsc::channel(64);
    let scheduler = Scheduler::new(
        vec![make()],
        local_executors(),
        tx,
        None,
        PathBuf::from(root.path()),
        HashMap::new(),
    );
    scheduler.run("build", &[]).await.unwrap();
    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() {
        events.push(evt);
    }
    assert!(
        events.iter().any(|e| matches!(
            e,
            SchedulerEvent::BeamCompleted {
                name,
                status: BeamStatus::Skipped {
                    reason: SkipReason::Cached
                },
            } if name == "build"
        )),
        "unchanged pkg/in.txt should produce a cache skip"
    );
}
