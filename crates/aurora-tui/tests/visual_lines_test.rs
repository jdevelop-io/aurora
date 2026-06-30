use aurora_tui::app::{visual_rows, wrap_log_line, BeamView};

#[test]
fn wrap_short_line_is_single_row() {
    assert_eq!(wrap_log_line("abcdefghij", 10), vec!["abcdefghij"]);
    assert_eq!(visual_rows("abcdefghij", 10), 1);
}

#[test]
fn wrap_splits_by_char_width() {
    assert_eq!(wrap_log_line("abcdefghijk", 10), vec!["abcdefghij", "k"]);
    assert_eq!(visual_rows("abcdefghijk", 10), 2);
}

#[test]
fn empty_line_is_single_row() {
    assert_eq!(wrap_log_line("", 10), vec![""]);
    assert_eq!(visual_rows("", 10), 1);
}

#[test]
fn wrap_count_matches_visual_rows() {
    let s = "0123456789012345"; // 16 chars
    assert_eq!(wrap_log_line(s, 7).len() as u16, visual_rows(s, 7)); // 3 rows
    assert_eq!(visual_rows(s, 7), 3);
}

#[test]
fn logical_line_at_visual_maps_offset_back_to_logical_line() {
    // largeur 10 : ligne0 (5 chars -> 1 row), ligne1 (12 chars -> 2 rows), ligne2 (3 chars -> 1 row)
    // disposition visuelle : row0=l0, row1+2=l1, row3=l2
    let mut beam = BeamView::new("b".to_string(), vec![]);
    beam.stdout = vec![
        "12345".to_string(),
        "123456789012".to_string(),
        "abc".to_string(),
    ];

    assert_eq!(beam.logical_line_at_visual(0, 10), 0);
    assert_eq!(beam.logical_line_at_visual(1, 10), 1);
    assert_eq!(beam.logical_line_at_visual(2, 10), 1);
    assert_eq!(beam.logical_line_at_visual(3, 10), 2);
}

#[test]
fn visual_offset_accumulates_wrapped_rows() {
    // ligne 0: 5 chars -> 1 row ; ligne 1: 12 chars -> 2 rows (width 10)
    let mut beam = BeamView::new("b".to_string(), vec![]);
    beam.stdout = vec!["12345".to_string(), "123456789012".to_string()];

    assert_eq!(beam.visual_offset(0, 10), 0);
    assert_eq!(beam.visual_offset(1, 10), 1);
    assert_eq!(beam.visual_offset(2, 10), 3); // 1 + 2
    assert_eq!(beam.total_visual_rows(10), 3);
}
