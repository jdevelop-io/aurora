use aurora_core::scheduler::{Scheduler, SchedulerEvent, BeamStatus};
use aurora_core::ast::{Beam, Run};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
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
        allow_failure: false,
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

// Vérifie que, lorsque pre_success n'est pas downward-closed (p est pre_success
// mais sa propre dépendance q est dans le run set), p reste silencieux même si
// q arrive à 0 d'in-degree — le scheduler ne doit pas le spawner.
#[tokio::test]
async fn test_pre_success_beam_not_spawned_when_dependency_completes() {
    // Graphe : q <- p <- root   ;   pre_success = ["p"]
    let beams = vec![
        make_beam("q",    vec![],     vec!["echo q"]),
        make_beam("p",    vec!["q"],  vec!["echo p"]),
        make_beam("root", vec!["p"],  vec!["echo root"]),
    ];
    let (tx, mut rx) = mpsc::channel(64);
    Scheduler::new(beams, local_executors(), tx, None, std::path::PathBuf::from("/tmp"), HashMap::new())
        .run("root", &["p".to_string()])
        .await
        .unwrap();

    let mut events = vec![];
    while let Ok(e) = rx.try_recv() { events.push(e); }

    let p_events: Vec<_> = events.iter().filter(|e| match e {
        SchedulerEvent::BeamStarted  { name }     => name == "p",
        SchedulerEvent::BeamCompleted { name, .. } => name == "p",
        SchedulerEvent::BeamOutput   { name, .. } => name == "p",
        _ => false,
    }).collect();
    assert!(p_events.is_empty(), "p est pre_success et ne doit émettre aucun événement : {:?}", p_events);
}

#[tokio::test]
async fn test_allow_failure_does_not_block_dependents() {
    // `b` échoue mais est toléré ; `d` en dépend et doit s'exécuter, et le run
    // global doit réussir (overall_success == true).
    let mut b = make_beam("b", vec![], vec!["false"]);
    b.allow_failure = true;
    let d = make_beam("d", vec!["b"], vec!["echo d"]);

    let (tx, mut rx) = mpsc::channel(64);
    let ok = Scheduler::new(vec![b, d], local_executors(), tx, None, std::path::PathBuf::from("/tmp"), HashMap::new())
        .run("d", &[]).await.unwrap();
    assert!(ok, "un échec toléré ne doit pas faire échouer le run");

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() { events.push(evt); }

    let b_allowed = events.iter().any(|e| matches!(e,
        SchedulerEvent::BeamCompleted { name, status: BeamStatus::FailedAllowed { .. } } if name == "b"));
    let d_ran = events.iter().any(|e| matches!(e,
        SchedulerEvent::BeamCompleted { name, status: BeamStatus::Success { .. } } if name == "d"));
    assert!(b_allowed, "b doit être en échec toléré (FailedAllowed)");
    assert!(d_ran, "d doit s'exécuter malgré l'échec toléré de b");
}

// `slow` (3s) et `quick_a` (1s) n'ont aucune dépendance. `quick_b` dépend
// UNIQUEMENT de `quick_a`, `root` dépend de `quick_b` et `slow`. Avec un vrai
// pipelining, `quick_b` doit démarrer dès la fin de `quick_a` (~1s), sans
// attendre `slow`. Le modèle par niveaux le retardait jusqu'à la fin de `slow`.
#[tokio::test]
async fn test_scheduler_pipelines_across_levels() {
    let beams = vec![
        make_beam("quick_a", vec![],                   vec!["sleep 1"]),
        make_beam("slow",    vec![],                   vec!["sleep 3"]),
        make_beam("quick_b", vec!["quick_a"],          vec!["echo b"]),
        make_beam("root",    vec!["quick_b", "slow"],  vec!["echo root"]),
    ];
    let (tx, mut rx) = mpsc::channel(256);
    let start = Instant::now();
    tokio::spawn(async move {
        Scheduler::new(beams, local_executors(), tx, None, std::path::PathBuf::from("/tmp"), HashMap::new())
            .run("root", &[]).await.unwrap();
    });

    let mut b_started_at = None;
    while let Some(evt) = rx.recv().await {
        match &evt {
            SchedulerEvent::BeamStarted { name } if name == "quick_b" => {
                b_started_at = Some(start.elapsed());
            }
            SchedulerEvent::AllDone { .. } => break,
            _ => {}
        }
    }

    let t = b_started_at.expect("quick_b n'a jamais démarré").as_secs_f64();
    assert!(t < 2.0, "quick_b devait démarrer vers 1s, démarré à {:.2}s (barrière de niveau)", t);
}
