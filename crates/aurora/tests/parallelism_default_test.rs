use aurora::resolve_max_parallelism;

#[test]
fn explicit_parallelism_is_preserved() {
    assert_eq!(resolve_max_parallelism(Some(4)), Some(4));
}

#[test]
fn explicit_zero_is_preserved() {
    // The scheduler clamps 0 to 1; the resolver must not reinterpret an
    // explicit user choice as "unset" and override it with a larger bound.
    assert_eq!(resolve_max_parallelism(Some(0)), Some(0));
}

#[test]
fn unset_parallelism_defaults_to_a_bound() {
    // A Beamfile that does not declare `parallelism` must not run unbounded:
    // an untrusted or merely large file would otherwise spawn one process per
    // independent beam at once (an accidental fork bomb). The default is the
    // host's available parallelism, always at least 1.
    let resolved = resolve_max_parallelism(None);
    assert!(matches!(resolved, Some(n) if n >= 1), "got {resolved:?}");
}
