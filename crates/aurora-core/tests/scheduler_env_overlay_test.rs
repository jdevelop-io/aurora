use aurora_core::ast::{Beam, Dependency, Run};
use aurora_core::scheduler::{Scheduler, SchedulerEvent};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;
use tokio::sync::mpsc;

fn local_executors() -> HashMap<String, Arc<dyn Executor>> {
    let mut m: HashMap<String, Arc<dyn Executor>> = HashMap::new();
    m.insert("local".into(), Arc::new(LocalExecutor::new()));
    m
}

// A beam with an env_overlay echoes its overlaid variable; a sibling beam
// without any overlay, running the exact same command, sees the variable
// unset. The overlay must shadow the global environment for its own instance
// only, and leak nowhere else (not even to a sibling in the same run).
#[tokio::test]
async fn env_overlay_shadows_for_its_own_instance_only() {
    let mut overlay = BTreeMap::new();
    overlay.insert("ONLY_ME".to_string(), "yes".to_string());

    let with_overlay = Beam {
        name: "with_overlay".to_string(),
        env_overlay: overlay,
        run: Some(Run {
            commands: vec!["echo \"$ONLY_ME\"".to_string()],
            executor: None,
        }),
        ..Beam::default()
    };
    let without_overlay = Beam {
        name: "without_overlay".to_string(),
        run: Some(Run {
            commands: vec!["echo \"$ONLY_ME\"".to_string()],
            executor: None,
        }),
        ..Beam::default()
    };
    let root = Beam {
        name: "root".to_string(),
        depends_on: vec![
            Dependency::named("with_overlay"),
            Dependency::named("without_overlay"),
        ],
        ..Beam::default()
    };

    let (tx, mut rx) = mpsc::channel(64);
    Scheduler::new(
        vec![with_overlay, without_overlay, root],
        local_executors(),
        tx,
        None,
        std::path::PathBuf::from("/tmp"),
        HashMap::new(),
    )
    .run("root", &[])
    .await
    .unwrap();

    let mut events = vec![];
    while let Ok(evt) = rx.try_recv() {
        events.push(evt);
    }

    let output_of = |beam: &str| -> Vec<String> {
        events
            .iter()
            .filter_map(|e| match e {
                SchedulerEvent::BeamOutput { name, line, .. } if name == beam => Some(line.clone()),
                _ => None,
            })
            .collect()
    };

    assert_eq!(output_of("with_overlay"), vec!["yes".to_string()]);
    assert_eq!(output_of("without_overlay"), vec!["".to_string()]);
}
