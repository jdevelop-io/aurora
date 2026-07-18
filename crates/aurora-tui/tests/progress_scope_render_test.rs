use aurora_core::scheduler::BeamStatus;
use aurora_tui::app::{ExecutionState, LogSearch, LogViewState};
use aurora_tui::execution::split_layout::render_execution;
use ratatui::{backend::TestBackend, Terminal};
use std::time::Duration;

/// Concatenates the whole rendered buffer so we can assert on the footer text.
fn rendered_text(exec: &ExecutionState) -> String {
    let backend = TestBackend::new(100, 24);
    let mut terminal = Terminal::new(backend).unwrap();
    let log_state = LogViewState::new(0);
    let search = LogSearch::new();
    terminal
        .draw(|f| render_execution(f, exec, &log_state, &search, 0, false, None, None))
        .unwrap();
    let buf = terminal.backend().buffer().clone();
    let area = buf.area;
    let mut text = String::new();
    for y in 0..area.height {
        for x in 0..area.width {
            text.push_str(buf[(x, y)].symbol());
        }
    }
    text
}

/// The repository's own Beamfile: running `check` executes {fmt, clippy, test,
/// check}; `build`, `bench` and `install` sit outside that subgraph.
fn check_run_all_done() -> ExecutionState {
    fn beam(name: &str, deps: &[&str]) -> (String, Vec<String>) {
        (
            name.to_string(),
            deps.iter().map(|d| d.to_string()).collect(),
        )
    }
    let mut exec = ExecutionState::new(vec![
        beam("fmt", &[]),
        beam("clippy", &["fmt"]),
        beam("test", &["fmt"]),
        beam("build", &[]),
        beam("check", &["clippy", "test"]),
        beam("bench", &["build"]),
        beam("install", &["check"]),
    ]);
    // The run covers only `check`'s closure; mark those four as finished.
    exec.set_run_set(["fmt", "clippy", "test", "check"].map(String::from));
    for b in exec.beams.iter_mut() {
        if exec_names().contains(&b.name.as_str()) {
            b.status = BeamStatus::Success {
                duration: Duration::from_secs(1),
                cached: false,
            };
        }
    }
    exec.done = Some(true);
    exec
}

fn exec_names() -> [&'static str; 4] {
    ["fmt", "clippy", "test", "check"]
}

#[test]
fn progress_count_reflects_the_run_not_the_whole_beamfile() {
    // The bug: the status bar counted every declared beam (`4/7`) so the bar
    // never filled. Scoped to the run, the four executed beams read `4/4`.
    let text = rendered_text(&check_run_all_done());
    assert!(
        text.contains("4/4"),
        "the progress count should be scoped to the run, rendered:\n{text}"
    );
    assert!(
        !text.contains("4/7"),
        "the count must not fold in beams outside the run, rendered:\n{text}"
    );
}

#[test]
fn out_of_run_beams_still_appear_in_the_sidebar() {
    // Option 1: the other beams stay listed (as launchers), just dimmed.
    let text = rendered_text(&check_run_all_done());
    for name in ["build", "bench", "install"] {
        assert!(
            text.contains(name),
            "{name} should remain visible in the sidebar, rendered:\n{text}"
        );
    }
}
