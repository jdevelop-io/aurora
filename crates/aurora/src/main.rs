mod plugins;

use anyhow::{bail, Result};
use aurora_core::{parser::parse, scheduler::Scheduler};
use aurora_executor_api::Executor;
use aurora_executor_local::LocalExecutor;
use clap::{Arg, Command};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Command::new("aurora")
        .version("0.1.0")
        .about("Aurora — task runner with HCL-inspired Beamfile DSL")
        .arg(Arg::new("beam").help("Beam to run").index(1))
        .arg(Arg::new("no-cache").long("no-cache").action(clap::ArgAction::SetTrue))
        .arg(Arg::new("dry-run").long("dry-run").action(clap::ArgAction::SetTrue))
        .arg(Arg::new("list").long("list").short('l').action(clap::ArgAction::SetTrue))
        .arg(Arg::new("var").long("var").action(clap::ArgAction::Append)
             .help("Override variable: --var key=value"));

    let matches = cli.get_matches();

    let beamfile_path = find_beamfile()?;
    let content = fs::read_to_string(&beamfile_path)?;
    let mut beam_file = parse(&content)?;

    if let Some(vars) = matches.get_many::<String>("var") {
        for var_str in vars {
            let (key, val) = var_str.split_once('=')
                .ok_or_else(|| anyhow::anyhow!("Invalid --var format, expected key=value"))?;
            if let Some(v) = beam_file.variables.iter_mut().find(|v| v.name == key) {
                v.default = val.to_string();
            }
        }
    }

    if matches.get_flag("list") {
        println!("Available beams:");
        for beam in &beam_file.beams {
            let desc = beam.description.as_deref().unwrap_or("");
            println!("  {:<20}  {}", beam.name, desc);
        }
        return Ok(());
    }

    if matches.get_flag("dry-run") {
        let target = resolve_target(&beam_file, matches.get_one::<String>("beam").map(|s| s.as_str()))?;
        println!("Would execute beam: {}", target);
        return Ok(());
    }

    let target = if let Some(beam_name) = matches.get_one::<String>("beam") {
        beam_name.clone()
    } else if let Some(picker_result) = aurora_tui::run_picker(
        beam_file.beams.iter().map(|b| (b.name.clone(), b.description.clone(), b.depends_on.clone())).collect()
    )? {
        picker_result
    } else {
        return Ok(());
    };

    let executor: Arc<dyn Executor> = Arc::new(LocalExecutor::new());

    let (tx, rx) = mpsc::channel(128);
    let beam_names: Vec<String> = beam_file.beams.iter().map(|b| b.name.clone()).collect();

    let working_dir = beamfile_path.parent().unwrap().to_path_buf();
    let beams = beam_file.beams.clone();
    let scheduler = Scheduler::new(
        beams,
        executor,
        tx,
        beam_file.config.as_ref().and_then(|c| c.max_parallelism),
        working_dir,
    );

    let target_clone = target.clone();
    tokio::spawn(async move {
        if let Err(e) = scheduler.run(&target_clone).await {
            eprintln!("Scheduler error: {}", e);
        }
    });

    aurora_tui::run_execution_tui(beam_names, rx).await?;

    Ok(())
}

fn find_beamfile() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        let candidate = dir.join("Beamfile");
        if candidate.exists() { return Ok(candidate); }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => bail!("No Beamfile found in current directory or any parent"),
        }
    }
}

fn resolve_target(
    beam_file: &aurora_core::ast::BeamFile,
    explicit: Option<&str>,
) -> Result<String> {
    if let Some(name) = explicit {
        return Ok(name.to_string());
    }
    if let Some(cfg) = &beam_file.config {
        if let Some(default) = &cfg.default {
            return Ok(default.clone());
        }
    }
    bail!("No beam specified and no default configured in aurora {{ }}")
}
