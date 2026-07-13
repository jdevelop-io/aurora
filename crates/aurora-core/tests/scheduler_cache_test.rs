use aurora_core::ast::{Beam, Run};
use aurora_core::scheduler::{BeamStatus, Scheduler, SchedulerEvent, SkipReason};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::collections::{BTreeMap, HashMap};
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

/// Runs `beam` to completion on `working_dir` and reports its final status.
async fn run_once(beam: Beam, working_dir: &std::path::Path) -> BeamStatus {
    run_once_with_env(beam, working_dir, BTreeMap::new()).await
}

/// Same, with a declared `environment {}` block folded into the cache key.
async fn run_once_with_env(
    beam: Beam,
    working_dir: &std::path::Path,
    declared_env: BTreeMap<String, String>,
) -> BeamStatus {
    let name = beam.name.clone();
    let (tx, mut rx) = mpsc::channel(64);
    let env: HashMap<String, String> = declared_env.clone().into_iter().collect();
    Scheduler::new(
        vec![beam],
        local_executors(),
        tx,
        None,
        working_dir.to_path_buf(),
        env,
    )
    .with_declared_env(declared_env)
    .run(&name, &[])
    .await
    .unwrap();

    let mut events = vec![];
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }
    status_of(&events, &name)
        .cloned()
        .expect("the beam should have completed")
}

/// The bug this cache key exists to prevent: the beam's `inputs` are untouched,
/// but its command changed. Hashing only the inputs served the previous run's
/// result, silently leaving the stale artifact on disk.
#[tokio::test]
async fn a_changed_command_invalidates_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("in.txt"), "unchanged").unwrap();
    let out = dir.path().join("out.txt");

    let beam_with = |cmd: &str| Beam {
        run: Some(Run {
            commands: vec![format!("echo {} > {}", cmd, out.display())],
            executor: None,
        }),
        ..cached_beam(dir.path())
    };

    let first = run_once(beam_with("VERSION-ONE"), dir.path()).await;
    assert!(
        matches!(first, BeamStatus::Success { cached: false, .. }),
        "first run should execute, got {first:?}"
    );
    assert_eq!(std::fs::read_to_string(&out).unwrap().trim(), "VERSION-ONE");

    // Only the command changed. in.txt is byte-for-byte identical.
    let second = run_once(beam_with("VERSION-TWO"), dir.path()).await;
    assert!(
        matches!(second, BeamStatus::Success { cached: false, .. }),
        "a changed command must re-run the beam, got {second:?}"
    );
    assert_eq!(
        std::fs::read_to_string(&out).unwrap().trim(),
        "VERSION-TWO",
        "the stale artifact from the previous command must have been rebuilt"
    );

    // Running the unchanged definition again is still a cache hit: the key must
    // invalidate on a real change, not defeat caching altogether.
    let third = run_once(beam_with("VERSION-TWO"), dir.path()).await;
    assert!(
        matches!(
            third,
            BeamStatus::Skipped {
                reason: SkipReason::Cached
            }
        ),
        "an unchanged definition must still hit the cache, got {third:?}"
    );
}

/// A `shell(...)` value in the `environment {}` block (a commit sha, a branch)
/// feeds the commands without appearing in them. When it changes, the beam's
/// result changes, so the entry must not be reused.
#[tokio::test]
async fn a_changed_declared_env_value_invalidates_the_cache() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("in.txt"), "unchanged").unwrap();
    let out = dir.path().join("out.txt");

    let beam = Beam {
        run: Some(Run {
            commands: vec![format!("echo $GIT_SHA > {}", out.display())],
            executor: None,
        }),
        ..cached_beam(dir.path())
    };
    let env_with = |sha: &str| -> BTreeMap<String, String> {
        [("GIT_SHA".to_string(), sha.to_string())].into()
    };

    let first = run_once_with_env(beam.clone(), dir.path(), env_with("aaaa")).await;
    assert!(
        matches!(first, BeamStatus::Success { cached: false, .. }),
        "first run should execute, got {first:?}"
    );
    assert_eq!(std::fs::read_to_string(&out).unwrap().trim(), "aaaa");

    let second = run_once_with_env(beam, dir.path(), env_with("bbbb")).await;
    assert!(
        matches!(second, BeamStatus::Success { cached: false, .. }),
        "a changed declared env value must re-run the beam, got {second:?}"
    );
    assert_eq!(std::fs::read_to_string(&out).unwrap().trim(), "bbbb");
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
