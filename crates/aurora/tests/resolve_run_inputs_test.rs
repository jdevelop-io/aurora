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

const BEAMFILE_WITH_BEAM_ENV: &str = r#"
aurora {
  version = "1"
  default = "deploy"
}

beam "deploy" {
  param "version" {}
  environment {
    RELEASE = "v${param.version}"
    STAMP   = shell("echo built-$RELEASE")
  }
  run { commands = ["echo ${param.version}"] }
}
"#;

#[test]
fn beam_environment_block_populates_env_overlay_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let beamfile = dir.path().join("Beamfile");
    fs::write(&beamfile, BEAMFILE_WITH_BEAM_ENV).unwrap();

    // Full wiring: parse the beam `environment {}` block (Task 2), interpolate
    // `${param.version}` into it (Task 3), then evaluate the overlay per
    // instance (Task 5). The instance id carries the bound param.
    let loaded =
        aurora::resolve_run_inputs(&beamfile, dir.path(), &[], "deploy", &["1.2.3".to_string()])
            .unwrap();

    let deploy = loaded
        .beams
        .iter()
        .find(|b| b.name == loaded.target_id)
        .unwrap();

    // Literal overlay value, with the param interpolated in.
    assert_eq!(
        deploy.env_overlay.get("RELEASE").map(String::as_str),
        Some("v1.2.3"),
        "the param must be interpolated into the beam's environment literal"
    );
    // Sequential evaluation: STAMP's shell() sees RELEASE set earlier in the
    // same overlay, proving the block is evaluated on the host in order.
    assert_eq!(
        deploy.env_overlay.get("STAMP").map(String::as_str),
        Some("built-v1.2.3"),
        "shell() in the overlay must see the earlier overlay value"
    );
}

#[test]
fn invalid_beamfile_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    let beamfile = dir.path().join("Beamfile");
    fs::write(&beamfile, "this is not valid hcl {{{").unwrap();
    assert!(aurora::resolve_run_inputs(&beamfile, dir.path(), &[], "x", &[]).is_err());
}
