use aurora_tui::widgets::status_bar::{fit_hints, hint_text, progress_fill, progress_fill_width};

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
    let hints = [("Tab", "focus"), ("/", "cherche"), ("q", "quitter")];
    assert_eq!(hint_text(&hints), "Tab focus · / cherche · q quitter");
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
    let full = [("↑↓", "beam"), ("/", "cherche"), ("q", "quitter")];
    let essential = [("/", "cherche"), ("q", "quitter")];
    assert_eq!(fit_hints(&full, &essential, 200), &full[..]);
}

#[test]
fn fit_hints_falls_back_to_essential_when_too_narrow() {
    let full = [("↑↓", "beam"), ("/", "cherche"), ("q", "quitter")];
    let essential = [("/", "cherche"), ("q", "quitter")];
    assert_eq!(fit_hints(&full, &essential, 8), &essential[..]);
}
