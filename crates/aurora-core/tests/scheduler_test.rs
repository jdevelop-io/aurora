use aurora_core::scheduler::{Scheduler, SchedulerEvent, BeamStatus};
use aurora_core::ast::{Beam, Run};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

fn local_executors() -> HashMap<String, Arc<dyn Executor>> {
    let mut m: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    m.insert("local".into(), Arc::new(LocalExecutor::new()));
    m
}

fn make_beam(name: &str, deps: Vec<&str>, commands: Vec<&str>) -> Beam {
    Beam {
        name: name.to_string(),
        description: None,
        depends_on: deps.iter().map(|s| s.to_string()).collect(),
        inputs: vec![],
        outputs: vec![],
        skip_if: None,
        condition: None,
        run: if commands.is_empty() { None } else {
            Some(Run {
                commands: commands.iter().map(|s| s.to_string()).collect(),
                executor: None,
            })
        },
    }
}

#[tokio::test]
async fn test_scheduler_simple() {
    let beams = vec![
        make_beam("a", vec![], vec!["echo a"]),
        make_beam("b", vec!["a"], vec!["echo b"]),
    ];
    let (tx, mut rx) = mpsc::channel(32);
    let scheduler = Scheduler::new(beams, local_executors(), tx, None, std::path::PathBuf::from("/tmp"), HashMap::new());
    scheduler.run("b", &[]).await.unwrap();

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() { events.push(evt); }

    let success_beams: Vec<_> = events.iter()
        .filter_map(|e| if let SchedulerEvent::BeamCompleted { name, status: BeamStatus::Success { .. } } = e { Some(name.as_str()) } else { None })
        .collect();
    assert!(success_beams.contains(&"a"));
    assert!(success_beams.contains(&"b"));
    let a_pos = success_beams.iter().position(|&n| n == "a").unwrap();
    let b_pos = success_beams.iter().position(|&n| n == "b").unwrap();
    assert!(a_pos < b_pos);
}

#[tokio::test]
async fn test_scheduler_zero_parallelism_does_not_deadlock() {
    let beams = vec![
        make_beam("a", vec![], vec!["echo a"]),
        make_beam("b", vec!["a"], vec!["echo b"]),
    ];
    let (tx, mut rx) = mpsc::channel(32);
    // max_parallelism = 0 (issu d'un Beamfile) ne doit pas figer le run.
    let scheduler = Scheduler::new(beams, local_executors(), tx, Some(0), std::path::PathBuf::from("/tmp"), HashMap::new());
    let res = tokio::time::timeout(std::time::Duration::from_secs(10), scheduler.run("b", &[])).await;
    let ok = res.expect("le scheduler s'est figé avec max_parallelism = 0").unwrap();
    assert!(ok);

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() { events.push(evt); }
    let success: Vec<_> = events.iter()
        .filter_map(|e| if let SchedulerEvent::BeamCompleted { name, status: BeamStatus::Success { .. } } = e { Some(name.as_str()) } else { None })
        .collect();
    assert!(success.contains(&"a"));
    assert!(success.contains(&"b"));
}

#[tokio::test]
async fn test_scheduler_failed_cancels_dependents() {
    let beams = vec![
        make_beam("a", vec![], vec!["false"]),
        make_beam("b", vec!["a"], vec!["echo b"]),
    ];
    let (tx, mut rx) = mpsc::channel(32);
    Scheduler::new(beams, local_executors(), tx, None, std::path::PathBuf::from("/tmp"), HashMap::new()).run("b", &[]).await.unwrap();

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() { events.push(evt); }

    let failed = events.iter().any(|e| matches!(e, SchedulerEvent::BeamCompleted { name, status: BeamStatus::Failed { .. } } if name == "a"));
    let cancelled = events.iter().any(|e| matches!(e, SchedulerEvent::BeamCompleted { name, status: BeamStatus::Cancelled } if name == "b"));
    assert!(failed, "a should have failed");
    assert!(cancelled, "b should be cancelled");
}

#[tokio::test]
async fn test_scheduler_cancellation_is_transitive() {
    // deploy -> test -> build ; build échoue. test ET deploy doivent être
    // annulés : deploy ne doit jamais s'exécuter alors que test n'a pas tourné.
    let beams = vec![
        make_beam("build",  vec![],         vec!["false"]),
        make_beam("test",   vec!["build"],  vec!["echo test"]),
        make_beam("deploy", vec!["test"],   vec!["echo deploy"]),
    ];
    let (tx, mut rx) = mpsc::channel(32);
    Scheduler::new(beams, local_executors(), tx, None, std::path::PathBuf::from("/tmp"), HashMap::new())
        .run("deploy", &[]).await.unwrap();

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() { events.push(evt); }

    let status_of = |beam: &str| -> Option<BeamStatus> {
        events.iter().rev().find_map(|e| match e {
            SchedulerEvent::BeamCompleted { name, status } if name == beam => Some(status.clone()),
            _ => None,
        })
    };

    assert!(matches!(status_of("build"),  Some(BeamStatus::Failed { .. })),    "build doit échouer");
    assert!(matches!(status_of("test"),   Some(BeamStatus::Cancelled)),        "test doit être annulé");
    assert!(matches!(status_of("deploy"), Some(BeamStatus::Cancelled)),        "deploy doit être annulé (et non exécuté)");

    // deploy ne doit apparaître dans aucun événement de sortie : il n'a pas tourné.
    let deploy_ran = events.iter().any(|e| matches!(e, SchedulerEvent::BeamOutput { name, .. } if name == "deploy"));
    assert!(!deploy_ran, "deploy ne devait produire aucune sortie");
}
