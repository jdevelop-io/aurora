use aurora_tui::app::LogViewState;

#[test]
fn bottom_is_total_minus_height() {
    let mut s = LogViewState::new(0);
    s.scroll_to_bottom(100, 20);
    assert_eq!(s.scroll, 80);
    assert!(!s.scroll_locked);
}

#[test]
fn content_fitting_screen_has_bottom_zero() {
    let mut s = LogViewState::new(0);
    s.scroll_to_bottom(10, 20); // total < height => max 0
    assert_eq!(s.scroll, 0);
}

#[test]
fn auto_scroll_pins_to_bottom_when_unlocked() {
    let mut s = LogViewState::new(0);
    s.auto_scroll(100, 20);
    assert_eq!(s.scroll, 80);
}

#[test]
fn auto_scroll_noop_when_locked() {
    let mut s = LogViewState::new(0);
    s.scroll = 10;
    s.scroll_locked = true;
    s.auto_scroll(100, 20);
    assert_eq!(s.scroll, 10);
}

#[test]
fn scroll_up_locks_and_clamps_at_zero() {
    let mut s = LogViewState::new(0);
    s.scroll = 3;
    s.scroll_lines(-5, 100, 20);
    assert_eq!(s.scroll, 0);
    assert!(s.scroll_locked);
}

#[test]
fn scroll_down_unlocks_at_bottom() {
    let mut s = LogViewState::new(0);
    s.scroll = 70;
    s.scroll_locked = true;
    s.scroll_lines(50, 100, 20); // max 80, clamp
    assert_eq!(s.scroll, 80);
    assert!(!s.scroll_locked);
}

#[test]
fn scroll_down_midway_stays_locked() {
    let mut s = LogViewState::new(0);
    s.scroll = 0;
    s.scroll_locked = true;
    s.scroll_lines(10, 100, 20);
    assert_eq!(s.scroll, 10);
    assert!(s.scroll_locked);
}

#[test]
fn scroll_to_top_locks() {
    let mut s = LogViewState::new(0);
    s.scroll = 50;
    s.scroll_to_top();
    assert_eq!(s.scroll, 0);
    assert!(s.scroll_locked);
}
