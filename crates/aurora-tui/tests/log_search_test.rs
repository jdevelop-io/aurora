use aurora_tui::app::{BeamView, LogSearch};

fn beam_with_logs(stdout: &[&str], stderr: &[&str]) -> BeamView {
    let mut beam = BeamView::new("b".to_string(), vec![]);
    beam.stdout = stdout.iter().map(|s| s.to_string()).collect();
    beam.stderr = stderr.iter().map(|s| s.to_string()).collect();
    beam
}

#[test]
fn recompute_finds_matching_lines_case_insensitive() {
    let beam = beam_with_logs(&["Error here", "all good", "another ERROR"], &[]);
    let mut search = LogSearch::new();
    search.query = "error".to_string();

    search.recompute(&beam);

    // lignes 0 et 2 contiennent "error" (insensible à la casse)
    assert_eq!(search.matches, vec![0, 2]);
    assert_eq!(search.match_count(), 2);
}

#[test]
fn recompute_matches_stderr_lines_with_offset() {
    // stdout: ["ok"] (idx 0), séparateur (idx 1), stderr: ["boom"] (idx 2)
    let beam = beam_with_logs(&["ok"], &["boom"]);
    let mut search = LogSearch::new();
    search.query = "boom".to_string();

    search.recompute(&beam);

    assert_eq!(search.matches, vec![2]);
}

#[test]
fn recompute_ignores_separator_line() {
    let beam = beam_with_logs(&["ok"], &["boom"]);
    let mut search = LogSearch::new();
    search.query = "stderr".to_string();

    search.recompute(&beam);

    // le séparateur "── stderr ──" ne doit pas matcher
    assert!(search.matches.is_empty());
}

#[test]
fn empty_query_yields_no_match() {
    let beam = beam_with_logs(&["anything"], &[]);
    let mut search = LogSearch::new();
    search.query = String::new();

    search.recompute(&beam);

    assert!(search.matches.is_empty());
    assert_eq!(search.match_count(), 0);
}

#[test]
fn next_and_prev_wrap_around() {
    let beam = beam_with_logs(&["x", "x", "x"], &[]);
    let mut search = LogSearch::new();
    search.query = "x".to_string();
    search.recompute(&beam);
    assert_eq!(search.matches, vec![0, 1, 2]);
    assert_eq!(search.current, 0);

    search.next();
    assert_eq!(search.current_line(), Some(1));
    search.next();
    assert_eq!(search.current_line(), Some(2));
    search.next(); // wrap
    assert_eq!(search.current_line(), Some(0));
    search.prev(); // wrap back
    assert_eq!(search.current_line(), Some(2));
}

#[test]
fn navigation_noop_when_no_matches() {
    let beam = beam_with_logs(&["abc"], &[]);
    let mut search = LogSearch::new();
    search.query = "zzz".to_string();
    search.recompute(&beam);

    search.next();
    search.prev();

    assert_eq!(search.current_line(), None);
}
