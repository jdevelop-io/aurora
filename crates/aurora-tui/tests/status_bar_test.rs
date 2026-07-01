use aurora_tui::widgets::status_bar::{
    fit_hints, hint_text, justify_gaps, progress_fill, progress_fill_width,
};

#[test]
fn progress_fill_empty_when_none_done() {
    assert_eq!(progress_fill(0, 8), Some((0, 16)));
}

#[test]
fn progress_fill_half_done() {
    assert_eq!(progress_fill(4, 8), Some((8, 8)));
}

#[test]
fn progress_fill_fully_done() {
    assert_eq!(progress_fill(8, 8), Some((16, 0)));
}

#[test]
fn progress_fill_zero_total_is_none() {
    assert_eq!(progress_fill(0, 0), None);
}

#[test]
fn progress_fill_sums_to_bar_width() {
    let (filled, empty) = progress_fill(3, 7).unwrap();
    assert_eq!(filled + empty, 16);
}

#[test]
fn hint_text_joins_keys_labels_with_separator() {
    let hints = [("Tab", "focus"), ("/", "search"), ("q", "quit")];
    assert_eq!(hint_text(&hints), "Tab focus · / search · q quit");
}

#[test]
fn progress_fill_width_scales_to_given_width() {
    assert_eq!(progress_fill_width(0, 8, 20), Some((0, 20)));
    assert_eq!(progress_fill_width(4, 8, 20), Some((10, 10)));
    assert_eq!(progress_fill_width(8, 8, 20), Some((20, 0)));
}

#[test]
fn progress_fill_width_none_when_no_total_or_no_width() {
    assert_eq!(progress_fill_width(0, 0, 20), None);
    assert_eq!(progress_fill_width(3, 8, 0), None);
}

#[test]
fn fit_hints_uses_full_set_when_it_fits() {
    let full = [("↑↓", "beam"), ("/", "search"), ("q", "quit")];
    let essential = [("/", "search"), ("q", "quit")];
    assert_eq!(fit_hints(&full, &essential, 200), &full[..]);
}

#[test]
fn fit_hints_falls_back_to_essential_when_too_narrow() {
    let full = [("↑↓", "beam"), ("/", "search"), ("q", "quit")];
    let essential = [("/", "search"), ("q", "quit")];
    assert_eq!(fit_hints(&full, &essential, 8), &essential[..]);
}

#[test]
fn justify_gaps_distributes_space_evenly() {
    // 3 items, 2 gaps; content 20, target 40 -> 20 of space distributed
    assert_eq!(justify_gaps(20, 3, 40), Some(vec![10, 10]));
}

#[test]
fn justify_gaps_puts_remainder_on_first_gaps() {
    assert_eq!(justify_gaps(20, 3, 41), Some(vec![11, 10]));
}

#[test]
fn justify_gaps_none_when_too_narrow() {
    // minimum = content + 3 per gap = 20 + 6 = 26
    assert_eq!(justify_gaps(20, 3, 25), None);
}

#[test]
fn justify_gaps_none_with_single_item() {
    assert_eq!(justify_gaps(10, 1, 40), None);
}
