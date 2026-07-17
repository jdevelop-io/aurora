//! A beam emitting more than u16::MAX visual lines (a verbose build/test run)
//! must not overflow the visual-row accounting: in debug that panics on every
//! frame, in release it wraps around and collapses the scroll so most of the
//! log becomes unreachable. The counts saturate at u16::MAX instead.

use aurora_tui::app::BeamView;

fn beam_with_many_lines(n: usize) -> BeamView {
    let mut beam = BeamView::new("noisy".to_string(), vec![]);
    beam.stdout = (0..n).map(|_| "x".to_string()).collect();
    beam
}

#[test]
fn total_visual_rows_saturates_instead_of_overflowing() {
    let beam = beam_with_many_lines(70_000);
    assert_eq!(beam.total_visual_rows(80), u16::MAX);
}

#[test]
fn visual_offset_saturates_instead_of_overflowing() {
    let beam = beam_with_many_lines(70_000);
    assert_eq!(beam.visual_offset(70_000, 80), u16::MAX);
}
