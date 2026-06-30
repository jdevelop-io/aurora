use aurora_core::ast::{Beam, Run};
use aurora_core::scheduler::{Scheduler, SchedulerEvent};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

fn beam(name: &str, deps: Vec<&str>) -> Beam {
    Beam {
        name: name.to_string(),
        description: None,
        depends_on: deps.iter().map(|s| s.to_string()).collect(),
        inputs: vec![],
        outputs: vec![],
        skip_if: None,
        condition: None,
        run: Some(Run {
            commands: vec!["echo ok".to_string()],
            executor: None,
        }),
        allow_failure: false,
    }
}

fn local_executors() -> HashMap<String, Arc<dyn Executor>> {
    let mut m: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    m.insert("local".into(), Arc::new(LocalExecutor::new()));
    m
}

#[tokio::test]
async fn pre_success_beams_emit_no_events() {
    let beams = vec![beam("dep", vec![]), beam("main", vec!["dep"])];

    let (tx, mut rx) = mpsc::channel(32);
    let scheduler = Scheduler::new(
        beams,
        local_executors(),
        tx,
        None,
        PathBuf::from("/tmp"),
        std::env::vars().collect(),
    );

    // "dep" est déjà réussi — ne doit émettre aucun événement
    scheduler.run("main", &["dep".to_string()]).await.unwrap();

    let mut events = vec![];
    while let Ok(e) = rx.try_recv() {
        events.push(e);
    }

    // Aucun BeamStarted ni BeamCompleted pour "dep"
    let dep_events: Vec<_> = events
        .iter()
        .filter(|e| match e {
            SchedulerEvent::BeamStarted { name } | SchedulerEvent::BeamCompleted { name, .. } => {
                name == "dep"
            }
            _ => false,
        })
        .collect();
    assert!(
        dep_events.is_empty(),
        "dep ne doit pas émettre d'événements : {:?}",
        dep_events
    );

    // "main" doit avoir été exécuté normalement
    let main_started = events
        .iter()
        .any(|e| matches!(e, SchedulerEvent::BeamStarted { name } if name == "main"));
    assert!(main_started, "main doit avoir été démarré");
}
