use aurora_tui::widgets::status_bar::build_progress_bar;

#[test]
fn progress_bar_empty_when_no_beams_done() {
    let bar = build_progress_bar(0, 8);
    assert_eq!(bar, "[░░░░░░░░░░░░░░░░] 0/8");
}

#[test]
fn progress_bar_half_done() {
    let bar = build_progress_bar(4, 8);
    assert_eq!(bar, "[████████░░░░░░░░] 4/8");
}

#[test]
fn progress_bar_fully_done() {
    let bar = build_progress_bar(8, 8);
    assert_eq!(bar, "[████████████████] 8/8");
}

#[test]
fn progress_bar_zero_total_returns_empty() {
    let bar = build_progress_bar(0, 0);
    assert_eq!(bar, "");
}
