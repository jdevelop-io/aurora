//! How a beam's child process is started.
//!
//! Two decisions, both invisible to a Beamfile author and both easy to get
//! subtly wrong: whether a shell is needed at all, and which binary a bare
//! command name resolves to.

use aurora_executor_local::{direct_argv, resolve_program};

fn cmds(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

// --- direct_argv: when can the shell be skipped? -------------------------

#[test]
fn a_single_plain_command_needs_no_shell() {
    assert_eq!(
        direct_argv(&cmds(&["cargo build --release"])),
        Some(vec![
            "cargo".to_string(),
            "build".to_string(),
            "--release".to_string()
        ])
    );
}

#[test]
fn surrounding_whitespace_is_ignored() {
    assert_eq!(
        direct_argv(&cmds(&["  ls   -la  "])),
        Some(vec!["ls".to_string(), "-la".to_string()])
    );
}

/// Several commands share a single shell, so a `cd` in the first is visible to
/// the second. Executing them separately would silently break that.
#[test]
fn several_commands_need_a_shell() {
    assert_eq!(direct_argv(&cmds(&["cd sub", "pwd"])), None);
}

#[test]
fn no_command_needs_no_process() {
    assert_eq!(direct_argv(&cmds(&[])), None);
    assert_eq!(direct_argv(&cmds(&["   "])), None);
}

/// Each of these means something to a shell and nothing to `exec`. Running them
/// directly would pass the metacharacter to the program as a literal argument.
#[test]
fn every_shell_metacharacter_forces_a_shell() {
    for command in [
        "a | b",           // pipe
        "a && b",          // and
        "a; b",            // separator
        "a > out.txt",     // redirect out
        "a < in.txt",      // redirect in
        "a &",             // background
        "echo $HOME",      // variable
        "echo ${VAR}",     // braced variable
        "echo `date`",     // legacy substitution
        "echo $(date)",    // substitution
        "rm *.tmp",        // glob
        "rm file?.txt",    // single-char glob
        "rm [ab].txt",     // class glob
        "cp ~/a /tmp",     // home expansion
        "FOO=bar program", // env assignment
        "echo \"quoted\"", // double quote
        "echo 'quoted'",   // single quote
        "echo a\\ b",      // escape
        "echo hi # note",  // comment
        "echo a\nb",       // newline
        "printf 100%%",    // percent
        "echo !!",         // history
    ] {
        assert_eq!(
            direct_argv(&cmds(&[command])),
            None,
            "`{command}` must go through a shell"
        );
    }
}

/// A builtin has no binary to exec, or execing one would silently do nothing
/// (`cd` in a child process changes nothing the next command could see).
#[test]
fn shell_builtins_force_a_shell() {
    for command in [
        "cd /tmp",
        "export FOO",
        "source script.sh",
        ". script.sh",
        ":",
        "eval something",
        "exec program",
        "set -x",
        "unset FOO",
        "umask 022",
        "trap cleanup EXIT",
    ] {
        assert_eq!(
            direct_argv(&cmds(&[command])),
            None,
            "`{command}` is a shell builtin"
        );
    }
}

/// `echo` exists as a binary, but the builtin and `/bin/echo` disagree on flags
/// such as `-e`. Not worth a behaviour change for one saved fork.
#[test]
fn echo_forces_a_shell_despite_having_a_binary() {
    assert_eq!(direct_argv(&cmds(&["echo hello"])), None);
}

// --- resolve_program: which binary does a bare name mean? ----------------

#[test]
fn an_absolute_path_is_used_as_is() {
    let path = "/usr/bin/env".to_string();
    assert_eq!(
        resolve_program("/usr/bin/env", Some(&path)),
        Some(std::path::PathBuf::from("/usr/bin/env"))
    );
}

#[test]
fn a_relative_path_is_used_as_is() {
    let path = "/usr/bin".to_string();
    assert_eq!(
        resolve_program("./script.sh", Some(&path)),
        Some(std::path::PathBuf::from("./script.sh"))
    );
}

#[test]
fn a_bare_name_is_resolved_against_the_declared_path() {
    let path = "/nonexistent:/usr/bin".to_string();
    assert_eq!(
        resolve_program("env", Some(&path)),
        Some(std::path::PathBuf::from("/usr/bin/env"))
    );
}

#[test]
fn a_name_absent_from_the_declared_path_does_not_resolve() {
    let path = "/nonexistent".to_string();
    assert_eq!(resolve_program("env", Some(&path)), None);
}

/// A directory named like the command must not be mistaken for it.
#[test]
fn a_directory_is_not_a_program() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::create_dir(dir.path().join("tool")).unwrap();
    let path = dir.path().display().to_string();
    assert_eq!(resolve_program("tool", Some(&path)), None);
}

#[test]
fn a_non_executable_file_is_not_a_program() {
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("tool"), "not executable").unwrap();
    let path = dir.path().display().to_string();
    assert_eq!(resolve_program("tool", Some(&path)), None);
}

#[test]
fn without_a_declared_path_a_bare_name_does_not_resolve() {
    assert_eq!(resolve_program("env", None), None);
}

/// The first match along PATH wins, as in any shell.
#[test]
fn the_first_match_along_the_path_wins() {
    let first = tempfile::tempdir().unwrap();
    let second = tempfile::tempdir().unwrap();
    for dir in [&first, &second] {
        let tool = dir.path().join("tool");
        std::fs::write(&tool, "#!/bin/sh\n").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
    }
    let path = format!("{}:{}", first.path().display(), second.path().display());
    assert_eq!(
        resolve_program("tool", Some(&path)),
        Some(first.path().join("tool"))
    );
}
