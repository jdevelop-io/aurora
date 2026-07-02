use aurora_tui::picker::fuzzy::fuzzy_score;

#[test]
fn exact_name_match_is_highest() {
    let score = fuzzy_score("build", "build", None);
    assert!(score >= 1000);
}

#[test]
fn substring_match_is_high() {
    let score = fuzzy_score("build", "rebuild", None);
    assert!((500..1000).contains(&score));
}

#[test]
fn fuzzy_match_chars_in_order() {
    // "bld" matches "build" (b..l..d)
    let score = fuzzy_score("bld", "build", None);
    assert!(score > 0 && score < 500);
}

#[test]
fn no_match_returns_none_score() {
    let score = fuzzy_score("xyz", "build", None);
    assert_eq!(score, 0);
}

#[test]
fn description_match_is_lowest() {
    let score_name = fuzzy_score("build", "build", None);
    let score_desc = fuzzy_score("build", "other", Some("run a build"));
    assert!(score_name > score_desc);
    assert!(score_desc > 0);
}
