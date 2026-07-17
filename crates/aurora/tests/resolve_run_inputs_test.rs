use std::fs;

const BEAMFILE: &str = r#"
aurora {
  version = "1"
  default = "greet"
  max_parallelism = 3
}

variable "who" {
  default = "world"
}

beam "greet" {
  run { commands = ["echo hello ${var.who}"] }
}
"#;

#[test]
fn resolves_beams_env_and_overrides() {
    let dir = tempfile::tempdir().unwrap();
    let beamfile = dir.path().join("Beamfile");
    fs::write(&beamfile, BEAMFILE).unwrap();

    let loaded = aurora::resolve_run_inputs(
        &beamfile,
        dir.path(),
        &["who=aurora".to_string()],
        "greet",
        &[],
    )
    .unwrap();

    let greet = loaded.beams.iter().find(|b| b.name == "greet").unwrap();
    let cmd = &greet.run.as_ref().unwrap().commands[0];
    assert_eq!(
        cmd, "echo hello aurora",
        "override interpolated into the command"
    );
    assert_eq!(loaded.max_parallelism, Some(3));
    // base_env is always present (PATH is allowlisted) even with no environment block.
    assert!(loaded.env.contains_key("PATH"));
}

#[test]
fn invalid_beamfile_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let beamfile = dir.path().join("Beamfile");
    fs::write(&beamfile, "this is not valid hcl {{{").unwrap();
    assert!(aurora::resolve_run_inputs(&beamfile, dir.path(), &[], "x", &[]).is_err());
}
