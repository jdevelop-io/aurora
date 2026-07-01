// WASM plugin loader present but not yet wired into the executor map
// (see CLAUDE.md): dead code is tolerated as long as it is not wired up,
// rather than removing it or wiring it prematurely.
#[allow(dead_code)]
mod plugins;

use anyhow::{bail, Result};
use aurora::headless;
use aurora_core::{
    env::evaluate,
    parser::parse,
    scheduler::{Scheduler, SchedulerEvent},
};
use aurora_executor_api::Executor;
use aurora_executor_docker::DockerExecutor;
use aurora_executor_local::LocalExecutor;
use clap::{Arg, Command};
use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Synthetic beam injected when several beams are selected at once in the
/// picker: it depends on every selected beam and is filtered out of any
/// user-facing listing.
const MULTI_BEAM: &str = "__multi__";

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Command::new("aurora")
        .version(env!("CARGO_PKG_VERSION"))
        .about("Aurora: task runner with HCL-inspired Beamfile DSL")
        .arg(Arg::new("beam").help("Beam to run").index(1))
        .arg(
            Arg::new("no-cache")
                .long("no-cache")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("dry-run")
                .long("dry-run")
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("list")
                .long("list")
                .short('l')
                .action(clap::ArgAction::SetTrue),
        )
        .arg(
            Arg::new("var")
                .long("var")
                .action(clap::ArgAction::Append)
                .help("Override variable: --var key=value"),
        )
        .arg(
            Arg::new("no-tui")
                .long("no-tui")
                .action(clap::ArgAction::SetTrue)
                .help("Force plain output, even in a terminal"),
        )
        .arg(
            Arg::new("interactive")
                .long("interactive")
                .short('i')
                .action(clap::ArgAction::SetTrue)
                .conflicts_with("no-tui")
                .help("Force the TUI, even when output is not a terminal"),
        );

    let matches = cli.get_matches();

    let beamfile_path = find_beamfile()?;
    let content = fs::read_to_string(&beamfile_path)?;
    let mut beam_file = parse(&content)?;

    if let Some(vars) = matches.get_many::<String>("var") {
        for var_str in vars {
            let (key, val) = var_str
                .split_once('=')
                .ok_or_else(|| anyhow::anyhow!("Invalid --var format, expected key=value"))?;
            if let Some(v) = beam_file.variables.iter_mut().find(|v| v.name == key) {
                v.default = val.to_string();
            }
        }
    }

    // Resolve `var.<name>` references in executor configs now that any --var
    // override has been applied, so the overrides actually take effect.
    aurora_core::parser::resolve_variables(&mut beam_file);

    if matches.get_flag("list") {
        println!("Available beams:");
        for beam in &beam_file.beams {
            let desc = beam.description.as_deref().unwrap_or("");
            println!("  {:<20}  {}", beam.name, desc);
        }
        return Ok(());
    }

    if matches.get_flag("dry-run") {
        let target = resolve_target(
            &beam_file,
            matches.get_one::<String>("beam").map(|s| s.as_str()),
        )?;
        print_execution_plan(&beam_file, &target)?;
        return Ok(());
    }

    let interactive = matches.get_flag("interactive")
        || (std::io::stdout().is_terminal() && !matches.get_flag("no-tui"));

    // Target resolution: picker in interactive mode, `default` beam in headless mode
    // (the picker is inherently interactive and does not exist outside a TTY).
    let target = if interactive {
        if let Some(beam_name) = matches.get_one::<String>("beam") {
            beam_name.clone()
        } else if let Some(picker_results) = aurora_tui::run_picker(
            beam_file
                .beams
                .iter()
                .map(|b| (b.name.clone(), b.description.clone(), b.depends_on.clone()))
                .collect(),
        )? {
            if picker_results.len() == 1 {
                picker_results.into_iter().next().unwrap()
            } else {
                // Multi-select: virtual beam __multi__ depending on the selected beams
                let virtual_beam = aurora_core::ast::Beam {
                    name: MULTI_BEAM.to_string(),
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
                MULTI_BEAM.to_string()
            }
        } else {
            return Ok(());
        }
    } else {
        resolve_target(
            &beam_file,
            matches.get_one::<String>("beam").map(|s| s.as_str()),
        )?
    };

    // Register each executor under the name it reports, so the registry key and
    // Executor::name() cannot drift apart.
    let mut executors: std::collections::HashMap<String, Arc<dyn Executor>> =
        std::collections::HashMap::new();
    for executor in [
        Arc::new(LocalExecutor::new()) as Arc<dyn Executor>,
        Arc::new(DockerExecutor::new()) as Arc<dyn Executor>,
    ] {
        executors.insert(executor.name().to_string(), executor);
    }

    let working_dir = beamfile_path.parent().unwrap().to_path_buf();

    // Evaluate environment variables (shell(...)) sequentially. When no
    // `environment { }` block is declared, fall back to the allowlisted base
    // environment, never to the full process environment: a Beamfile is
    // untrusted and must not inherit ambient secrets (CI tokens, AWS_*, ...).
    let env = match &beam_file.environment {
        Some(env_block) => evaluate(env_block, &working_dir)?,
        None => aurora_core::env::base_env(),
    };

    let (tx, rx) = mpsc::channel(128);
    // Exclude the virtual beam __multi__ from the displayed list / prefixes
    let beam_info: Vec<(String, Vec<String>)> = beam_file
        .beams
        .iter()
        .filter(|b| b.name != MULTI_BEAM)
        .map(|b| (b.name.clone(), b.depends_on.clone()))
        .collect();

    let no_cache = matches.get_flag("no-cache");

    let beams = beam_file.beams.clone();
    let mut scheduler = Scheduler::new(
        beams,
        executors.clone(),
        tx,
        beam_file.config.as_ref().and_then(|c| c.max_parallelism),
        working_dir.clone(),
        env.clone(),
    );
    if no_cache {
        scheduler = scheduler.without_cache();
    }

    if interactive {
        let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();
        let target_clone = target.clone();
        tokio::spawn(async move {
            if let Err(e) = scheduler
                .run_cancellable(&target_clone, &[], cancel_rx)
                .await
            {
                eprintln!("Scheduler error: {}", e);
            }
        });

        let rerun_beams: Vec<_> = beam_file
            .beams
            .iter()
            .filter(|b| b.name != MULTI_BEAM)
            .cloned()
            .collect();
        let rerun_executors = executors.clone();
        let rerun_max_par = beam_file.config.as_ref().and_then(|c| c.max_parallelism);
        let rerun_working_dir = working_dir.clone();
        let rerun_env = env.clone();

        let rerun = move |root: String,
                          pre_success: Vec<String>|
              -> (
            mpsc::Receiver<SchedulerEvent>,
            mpsc::UnboundedSender<String>,
        ) {
            let (tx, rx) = mpsc::channel(128);
            let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();
            let mut scheduler = Scheduler::new(
                rerun_beams.clone(),
                rerun_executors.clone(),
                tx,
                rerun_max_par,
                rerun_working_dir.clone(),
                rerun_env.clone(),
            );
            if no_cache {
                scheduler = scheduler.without_cache();
            }
            tokio::runtime::Handle::current().spawn(async move {
                if let Err(e) = scheduler
                    .run_cancellable(&root, &pre_success, cancel_rx)
                    .await
                {
                    eprintln!("Scheduler error: {}", e);
                }
            });
            (rx, cancel_tx)
        };

        aurora_tui::run_execution_tui(beam_info, rx, cancel_tx, rerun).await?;
    } else {
        // Headless mode: no interactive cancellation, `run` manages its own channel.
        let target_clone = target.clone();
        let handle = tokio::spawn(async move { scheduler.run(&target_clone, &[]).await });

        let beam_names: Vec<String> = beam_info.iter().map(|(name, _)| name.clone()).collect();
        // Color decided per stream: stdout and stderr can be redirected
        // independently (e.g. `aurora --no-tui 2>err.log` in a terminal).
        let out_color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        let err_color = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        let mut stdout = std::io::stdout();
        let mut stderr = std::io::stderr();
        let success = headless::run_headless(
            &beam_names,
            out_color,
            err_color,
            rx,
            &mut stdout,
            &mut stderr,
        )
        .await?;

        // The scheduler can fail before emitting AllDone (DAG construction error:
        // cycle, unknown dependency). We join its task to propagate
        // the failure, otherwise an invalid Beamfile would exit with 0.
        let scheduler_ok = match handle.await {
            Ok(Ok(ok)) => ok,
            Ok(Err(e)) => {
                eprintln!("Scheduler error: {}", e);
                false
            }
            Err(e) => {
                eprintln!("Scheduler task panicked: {}", e);
                false
            }
        };
        if !success || !scheduler_ok {
            std::process::exit(1);
        }
    }

    Ok(())
}

fn find_beamfile() -> Result<PathBuf> {
    let mut dir = std::env::current_dir()?;
    loop {
        let candidate = dir.join("Beamfile");
        if candidate.exists() {
            return Ok(candidate);
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => bail!("No Beamfile found in current directory or any parent"),
        }
    }
}

/// Prints, without running anything, the beams a target would execute grouped
/// by dependency level (level 0 runs first). Building the DAG here also
/// surfaces a malformed Beamfile (cycle, unknown dependency) during a dry run.
fn print_execution_plan(beam_file: &aurora_core::ast::BeamFile, target: &str) -> Result<()> {
    let deps: Vec<(String, Vec<String>)> = beam_file
        .beams
        .iter()
        .map(|b| (b.name.clone(), b.depends_on.clone()))
        .collect();
    let graph = aurora_core::dag::BeamGraph::from_deps(deps)?;
    let levels = graph.execution_levels(target)?;

    println!("Execution plan for '{target}':");
    if levels.is_empty() {
        println!("  (nothing to run)");
    }
    for (i, level) in levels.iter().enumerate() {
        let mut names = level.clone();
        names.sort();
        println!("  level {i}: {}", names.join(", "));
    }
    Ok(())
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
