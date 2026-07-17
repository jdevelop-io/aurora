use aurora::watch::debounce_loop;
use aurora_core::events::WatchTrigger;
use std::time::Duration;
use tokio::sync::mpsc;

#[tokio::test(start_paused = true)]
async fn coalesces_a_burst_into_one_trigger() {
    let (raw_tx, raw_rx) = mpsc::unbounded_channel::<bool>();
    let (trig_tx, mut trig_rx) = mpsc::channel::<WatchTrigger>(8);
    tokio::spawn(debounce_loop(raw_rx, trig_tx, Duration::from_millis(250)));

    // Three input changes in quick succession, none is the Beamfile.
    raw_tx.send(false).unwrap();
    raw_tx.send(false).unwrap();
    raw_tx.send(false).unwrap();

    // Before the quiet period elapses: nothing yet.
    tokio::time::advance(Duration::from_millis(100)).await;
    assert!(
        trig_rx.try_recv().is_err(),
        "no trigger before the quiet period"
    );

    // After the quiet period: exactly one coalesced trigger.
    tokio::time::advance(Duration::from_millis(250)).await;
    tokio::task::yield_now().await;
    let trig = trig_rx
        .try_recv()
        .expect("one trigger after the quiet period");
    assert_eq!(
        trig,
        WatchTrigger {
            beamfile_changed: false
        }
    );
    assert!(
        trig_rx.try_recv().is_err(),
        "only one trigger for the burst"
    );
}

#[tokio::test(start_paused = true)]
async fn beamfile_change_is_ored_across_the_burst() {
    let (raw_tx, raw_rx) = mpsc::unbounded_channel::<bool>();
    let (trig_tx, mut trig_rx) = mpsc::channel::<WatchTrigger>(8);
    tokio::spawn(debounce_loop(raw_rx, trig_tx, Duration::from_millis(250)));

    raw_tx.send(false).unwrap(); // an input
    raw_tx.send(true).unwrap(); // the Beamfile
    tokio::time::advance(Duration::from_millis(250)).await;
    tokio::task::yield_now().await;

    assert_eq!(
        trig_rx.try_recv().unwrap(),
        WatchTrigger {
            beamfile_changed: true
        },
        "beamfile_changed is the OR over the window"
    );
}

#[tokio::test(start_paused = true)]
async fn a_late_signal_extends_the_quiet_period() {
    let (raw_tx, raw_rx) = mpsc::unbounded_channel::<bool>();
    let (trig_tx, mut trig_rx) = mpsc::channel::<WatchTrigger>(8);
    tokio::spawn(debounce_loop(raw_rx, trig_tx, Duration::from_millis(250)));

    raw_tx.send(false).unwrap();
    tokio::time::advance(Duration::from_millis(200)).await; // not yet quiet
    raw_tx.send(false).unwrap(); // resets the timer
    tokio::time::advance(Duration::from_millis(200)).await;
    assert!(
        trig_rx.try_recv().is_err(),
        "the late signal reset the timer"
    );
    tokio::time::advance(Duration::from_millis(250)).await;
    tokio::task::yield_now().await;
    assert!(trig_rx.try_recv().is_ok(), "trigger fires once quiet again");
}
