#[test]
fn watch_flag_parses_short_and_long() {
    let m = aurora::cli()
        .try_get_matches_from(["aurora", "build", "--watch"])
        .expect("--watch parses");
    assert!(m.get_flag("watch"));

    let m = aurora::cli()
        .try_get_matches_from(["aurora", "-w"])
        .expect("-w parses without a beam");
    assert!(m.get_flag("watch"));
}

#[test]
fn watch_defaults_to_false() {
    let m = aurora::cli()
        .try_get_matches_from(["aurora", "build"])
        .unwrap();
    assert!(!m.get_flag("watch"));
}

#[test]
fn watch_conflicts_with_json_list_and_dry_run() {
    for other in ["--json", "--list", "--dry-run"] {
        assert!(
            aurora::cli()
                .try_get_matches_from(["aurora", "-w", other])
                .is_err(),
            "-w must conflict with {other}"
        );
    }
}

#[test]
fn watch_is_compatible_with_no_tui_and_interactive() {
    assert!(aurora::cli()
        .try_get_matches_from(["aurora", "build", "-w", "--no-tui"])
        .is_ok());
    assert!(aurora::cli()
        .try_get_matches_from(["aurora", "build", "-w", "-i"])
        .is_ok());
}
