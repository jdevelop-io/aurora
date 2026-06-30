use aurora_tui::app::{BeamView, LogKind};

fn lines(beam: &BeamView) -> Vec<(String, LogKind)> {
    beam.iter_log_lines()
        .map(|(t, k)| (t.to_string(), k))
        .collect()
}

#[test]
fn stdout_only() {
    let mut beam = BeamView::new("b".to_string(), vec![]);
    beam.stdout = vec!["un".to_string(), "deux".to_string()];

    let l = lines(&beam);

    assert_eq!(
        l,
        vec![
            ("un".to_string(), LogKind::Stdout),
            ("deux".to_string(), LogKind::Stdout),
        ]
    );
    assert_eq!(beam.log_line_count(), 2);
}

#[test]
fn stdout_then_stderr_with_separator() {
    let mut beam = BeamView::new("b".to_string(), vec![]);
    beam.stdout = vec!["out".to_string()];
    beam.stderr = vec!["err".to_string()];

    let l = lines(&beam);

    assert_eq!(
        l,
        vec![
            ("out".to_string(), LogKind::Stdout),
            ("── stderr ──".to_string(), LogKind::Separator),
            ("err".to_string(), LogKind::Stderr),
        ]
    );
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

#[test]
fn sanitize_strips_ansi_color_codes() {
    use aurora_tui::app::sanitize_log_line;
    // SGR couleur verte + reset autour du texte.
    let raw = "\x1b[32mViolations\x1b[0m 0";
    assert_eq!(sanitize_log_line(raw), "Violations 0");
}

#[test]
fn sanitize_strips_cursor_moves_and_carriage_return() {
    use aurora_tui::app::sanitize_log_line;
    // Déplacement de curseur (ESC[2K efface la ligne) + retour chariot.
    let raw = "ab\x1b[2K\rcd";
    assert_eq!(sanitize_log_line(raw), "abcd");
}

#[test]
fn sanitize_keeps_plain_text_and_tabs() {
    use aurora_tui::app::sanitize_log_line;
    assert_eq!(sanitize_log_line("a\tb c"), "a\tb c");
}

#[test]
fn sanitize_strips_osc_hyperlink() {
    use aurora_tui::app::sanitize_log_line;
    // OSC 8 (lien hypertexte) terminé par BEL.
    let raw = "\x1b]8;;https://x\x07link\x1b]8;;\x07";
    assert_eq!(sanitize_log_line(raw), "link");
}
