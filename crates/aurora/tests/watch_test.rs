use aurora::watch::glob_root;
use std::path::PathBuf;

#[test]
fn glob_root_stops_at_the_first_metacharacter() {
    assert_eq!(glob_root("src/**/*.rs"), PathBuf::from("src"));
    assert_eq!(glob_root("assets/*.css"), PathBuf::from("assets"));
    assert_eq!(glob_root("a/b/c/*.txt"), PathBuf::from("a/b/c"));
}

#[test]
fn glob_root_of_a_bare_glob_is_empty() {
    assert_eq!(glob_root("*.rs"), PathBuf::new());
    assert_eq!(glob_root("?.rs"), PathBuf::new());
    assert_eq!(glob_root("[abc].rs"), PathBuf::new());
}

#[test]
fn glob_root_of_a_literal_is_the_whole_path() {
    assert_eq!(glob_root("input.txt"), PathBuf::from("input.txt"));
    assert_eq!(
        glob_root("config/app.toml"),
        PathBuf::from("config/app.toml")
    );
}
