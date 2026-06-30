use aurora_core::ast::{Beam, Run};
use aurora_core::scheduler::{BeamStatus, Scheduler, SchedulerEvent};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
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
        run: if commands.is_empty() {
            None
        } else {
            Some(Run {
                commands: commands.iter().map(|s| s.to_string()).collect(),
                executor: None,
            })
        },
        allow_failure: false,
    }
}

// `slow` tourne longtemps et a un dépendant `after`. `independent` est court et
// sans lien. On annule `slow` en cours : `slow` -> Cancelled, `after` -> Cancelled,
// `independent` -> Success, et le run global échoue.
#[tokio::test]
async fn test_cancel_running_beam_cancels_it_and_dependents() {
    let beams = vec![
        make_beam("slow", vec![], vec!["sleep 30"]),
        make_beam("after", vec!["slow"], vec!["echo after"]),
        make_beam("independent", vec![], vec!["echo indep"]),
        make_beam("root", vec!["after", "independent"], vec!["echo root"]),
    ];

    let (tx, mut rx) = mpsc::channel(256);
    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();

    let handle = tokio::spawn(async move {
        Scheduler::new(
            beams,
            local_executors(),
            tx,
            None,
            std::path::PathBuf::from("/tmp"),
            HashMap::new(),
        )
        .run_cancellable("root", &[], cancel_rx)
        .await
        .unwrap()
    });

    // Attendre que `slow` démarre, puis l'annuler.
    let mut slow_started = false;
    let mut events = vec![];
    while let Some(evt) = rx.recv().await {
        if let SchedulerEvent::BeamStarted { name } = &evt {
            if name == "slow" && !slow_started {
                slow_started = true;
                cancel_tx.send("slow".to_string()).unwrap();
            }
        }
        let done = matches!(evt, SchedulerEvent::AllDone { .. });
        events.push(evt);
        if done {
            break;
        }
    }

    let overall = tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("le scheduler ne s'est pas terminé après annulation")
        .unwrap();

    let status_of = |beam: &str| -> Option<BeamStatus> {
        events.iter().rev().find_map(|e| match e {
            SchedulerEvent::BeamCompleted { name, status } if name == beam => Some(status.clone()),
            _ => None,
        })
    };

    assert!(slow_started, "slow aurait dû démarrer");
    assert!(
        matches!(status_of("slow"), Some(BeamStatus::Cancelled)),
        "slow doit être annulé"
    );
    assert!(
        matches!(status_of("after"), Some(BeamStatus::Cancelled)),
        "after (dépendant) doit être annulé"
    );
    assert!(
        matches!(status_of("independent"), Some(BeamStatus::Success { .. })),
        "independent doit réussir"
    );
    assert!(!overall, "le run global doit échouer après une annulation");

    let done_ok = events
        .iter()
        .any(|e| matches!(e, SchedulerEvent::AllDone { success: false }));
    assert!(done_ok, "AllDone doit reporter success=false");
}

// Annuler un beam `allow_failure` se comporte comme un échec toléré : son
// dépendant tourne quand même (débloqué) et le run global reste vert. Le beam
// annulé s'affiche tout de même `Cancelled`.
#[tokio::test]
async fn test_cancel_allow_failure_beam_does_not_cancel_dependents() {
    let mut slow = make_beam("slow", vec![], vec!["sleep 30"]);
    slow.allow_failure = true;
    let after = make_beam("after", vec!["slow"], vec!["echo after"]);

    let (tx, mut rx) = mpsc::channel(256);
    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();

    let handle = tokio::spawn(async move {
        Scheduler::new(
            vec![slow, after],
            local_executors(),
            tx,
            None,
            std::path::PathBuf::from("/tmp"),
            HashMap::new(),
        )
        .run_cancellable("after", &[], cancel_rx)
        .await
        .unwrap()
    });

    let mut slow_started = false;
    let mut events = vec![];
    while let Some(evt) = rx.recv().await {
        if let SchedulerEvent::BeamStarted { name } = &evt {
            if name == "slow" && !slow_started {
                slow_started = true;
                cancel_tx.send("slow".to_string()).unwrap();
            }
        }
        let done = matches!(evt, SchedulerEvent::AllDone { .. });
        events.push(evt);
        if done {
            break;
        }
    }

    let overall = tokio::time::timeout(Duration::from_secs(5), handle)
        .await
        .expect("le scheduler ne s'est pas terminé après annulation")
        .unwrap();

    let status_of = |beam: &str| -> Option<BeamStatus> {
        events.iter().rev().find_map(|e| match e {
            SchedulerEvent::BeamCompleted { name, status } if name == beam => Some(status.clone()),
            _ => None,
        })
    };

    assert!(slow_started, "slow aurait dû démarrer");
    assert!(
        matches!(status_of("slow"), Some(BeamStatus::Cancelled)),
        "slow annulé doit s'afficher Cancelled"
    );
    assert!(
        matches!(status_of("after"), Some(BeamStatus::Success { .. })),
        "after doit tourner malgré l'annulation de slow (allow_failure)"
    );
    assert!(
        overall,
        "annuler un beam allow_failure ne doit pas faire échouer le run"
    );
}
