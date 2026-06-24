use aurora_tui::widgets::status_bar::{hint_text, progress_fill};

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
