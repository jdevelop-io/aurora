use aurora_tui::app::{BeamView, LogSearch};

fn beam(stdout: &[&str]) -> BeamView {
    let mut b = BeamView::new("b".to_string(), vec![]);
    b.stdout = stdout.iter().map(|s| s.to_string()).collect();
    b
}

#[test]
fn preserves_current_line_when_new_matches_arrive() {
    let mut b = beam(&["x", "y", "x"]);
    let mut search = LogSearch::new();
    search.query = "x".to_string();
    search.recompute(&b);
    assert_eq!(search.matches, vec![0, 2]);
    search.next(); // current points to line 2
    assert_eq!(search.current_line(), Some(2));

    // New output containing a match
    b.stdout.push("x".to_string());
    search.recompute_preserving(&b);

    // the list grows but we stay on the same logical line
    assert_eq!(search.matches, vec![0, 2, 3]);
    assert_eq!(search.current_line(), Some(2));
}

#[test]
fn first_match_appears_after_search() {
    let mut b = beam(&["a"]);
    let mut search = LogSearch::new();
    search.query = "z".to_string();
    search.recompute(&b);
    assert!(search.matches.is_empty());
    assert_eq!(search.current_line(), None);

    // The searched-for output arrives later
    b.stdout.push("z".to_string());
    search.recompute_preserving(&b);

    assert_eq!(search.matches, vec![1]);
    assert_eq!(search.current_line(), Some(1));
}

#[test]
fn does_not_jump_index_back_to_zero_on_recompute() {
    let mut b = beam(&["m", "m", "m"]);
    let mut search = LogSearch::new();
    search.query = "m".to_string();
    search.recompute(&b);
    search.next();
    search.next(); // current on line 2 (index 2 in matches)
    assert_eq!(search.current_line(), Some(2));

    search.recompute_preserving(&b); // nothing new

    assert_eq!(search.current_line(), Some(2));
}
