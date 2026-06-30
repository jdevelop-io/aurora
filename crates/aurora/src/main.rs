mod plugins;

use anyhow::{bail, Result};
use aurora_core::{env::evaluate, parser::parse, scheduler::{Scheduler, SchedulerEvent}};
use aurora_executor_api::Executor;
use aurora_executor_docker::DockerExecutor;
use aurora_executor_local::LocalExecutor;
use clap::{Arg, Command};
use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Command::new("aurora")
        .version(env!("CARGO_PKG_VERSION"))
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
    } else if let Some(picker_results) = aurora_tui::run_picker(
        beam_file.beams.iter().map(|b| (b.name.clone(), b.description.clone(), b.depends_on.clone())).collect()
    )? {
        if picker_results.len() == 1 {
            picker_results.into_iter().next().unwrap()
        } else {
            // Multi-select : beam virtuel __multi__ qui dépend de tous les beams sélectionnés
            let virtual_beam = aurora_core::ast::Beam {
                name: "__multi__".to_string(),
                description: Some("Multi-beam run".to_string()),
                depends_on: picker_results,
                inputs: vec![],
                outputs: vec![],
                skip_if: None,
                condition: None,
                run: None,
                allow_failure: false,
            };
            beam_file.beams.push(virtual_beam);
            "__multi__".to_string()
        }
    } else {
        return Ok(());
    };

    let mut executors: std::collections::HashMap<String, Arc<dyn Executor>> = std::collections::HashMap::new();
    executors.insert("local".into(), Arc::new(LocalExecutor::new()));
    executors.insert("docker".into(), Arc::new(DockerExecutor::new()));

    let working_dir = beamfile_path.parent().unwrap().to_path_buf();

    // Évaluer les variables environment (shell(...)) séquentiellement
    let env = if let Some(env_block) = &beam_file.environment {
        evaluate(env_block, &working_dir)?
    } else {
        std::env::vars().collect()
    };

    let (tx, rx) = mpsc::channel(128);
    let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();
    // Exclure le beam virtuel __multi__ de la liste affichée dans la TUI
    let beam_info: Vec<(String, Vec<String>)> = beam_file.beams.iter()
        .filter(|b| b.name != "__multi__")
        .map(|b| (b.name.clone(), b.depends_on.clone()))
        .collect();

    let beams = beam_file.beams.clone();
    let scheduler = Scheduler::new(
        beams,
        executors.clone(),
        tx,
        beam_file.config.as_ref().and_then(|c| c.max_parallelism),
        working_dir.clone(),
        env.clone(),
    );

    let target_clone = target.clone();
    tokio::spawn(async move {
        if let Err(e) = scheduler.run_cancellable(&target_clone, &[], cancel_rx).await {
            eprintln!("Scheduler error: {}", e);
        }
    });

    let rerun_beams: Vec<_> = beam_file.beams.iter().filter(|b| b.name != "__multi__").cloned().collect();
    let rerun_executors = executors.clone();
    let rerun_max_par = beam_file.config.as_ref().and_then(|c| c.max_parallelism);
    let rerun_working_dir = working_dir.clone();
    let rerun_env = env.clone();

    let rerun = move |root: String, pre_success: Vec<String>| -> (mpsc::Receiver<SchedulerEvent>, mpsc::UnboundedSender<String>) {
        let (tx, rx) = mpsc::channel(128);
        let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();
        let scheduler = Scheduler::new(
            rerun_beams.clone(),
            rerun_executors.clone(),
            tx,
            rerun_max_par,
            rerun_working_dir.clone(),
            rerun_env.clone(),
        );
        tokio::runtime::Handle::current().spawn(async move {
            if let Err(e) = scheduler.run_cancellable(&root, &pre_success, cancel_rx).await {
                eprintln!("Scheduler error: {}", e);
            }
        });
        (rx, cancel_tx)
    };

    aurora_tui::run_execution_tui(beam_info, rx, cancel_tx, rerun).await?;

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
