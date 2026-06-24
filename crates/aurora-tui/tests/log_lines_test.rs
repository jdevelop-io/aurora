use aurora_tui::app::{BeamView, LogKind};

fn lines(beam: &BeamView) -> Vec<(String, LogKind)> {
    beam.iter_log_lines().map(|(t, k)| (t.to_string(), k)).collect()
}

#[test]
fn stdout_only() {
    let mut beam = BeamView::new("b".to_string(), vec![]);
    beam.stdout = vec!["un".to_string(), "deux".to_string()];

    let l = lines(&beam);

    assert_eq!(l, vec![
        ("un".to_string(), LogKind::Stdout),
        ("deux".to_string(), LogKind::Stdout),
    ]);
    assert_eq!(beam.log_line_count(), 2);
}

#[test]
fn stdout_then_stderr_with_separator() {
    let mut beam = BeamView::new("b".to_string(), vec![]);
    beam.stdout = vec!["out".to_string()];
    beam.stderr = vec!["err".to_string()];

    let l = lines(&beam);

    assert_eq!(l, vec![
        ("out".to_string(), LogKind::Stdout),
        ("── stderr ──".to_string(), LogKind::Separator),
        ("err".to_string(), LogKind::Stderr),
    ]);
    assert_eq!(beam.log_line_count(), 3);
}

#[test]
fn empty_yields_single_placeholder() {
    let beam = BeamView::new("b".to_string(), vec![]);

    let l = lines(&beam);

    assert_eq!(l.len(), 1);
    assert_eq!(l[0].1, LogKind::Placeholder);
    assert_eq!(beam.log_line_count(), 1);
}
