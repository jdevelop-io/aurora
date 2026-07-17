use anyhow::{bail, Result};
use aurora::headless;
use aurora_core::{env::evaluate, events::SchedulerEvent, parser::parse};
use aurora_executor_api::Executor;
use aurora_executor_docker::DockerExecutor;
use aurora_executor_local::LocalExecutor;
use std::fs;
use std::io::IsTerminal;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Synthetic beam injected when several beams are selected at once in the
/// picker: it depends on every selected beam and is filtered out of any
/// user-facing listing.
const MULTI_BEAM: &str = "__multi__";

#[tokio::main]
async fn main() -> Result<()> {
    let matches = aurora::cli().get_matches();

    // Pure emitters: a packager runs them from an arbitrary directory, so they
    // must not depend on a Beamfile being present.
    if let Some(&shell) = matches.get_one::<clap_complete::Shell>("completions") {
        aurora::print_completions(shell, &mut std::io::stdout());
        return Ok(());
    }
    if matches.get_flag("man") {
        aurora::print_man_page(&mut std::io::stdout())?;
        return Ok(());
    }

    let json = matches.get_flag("json");

    let beamfile_path = match find_beamfile(json) {
        Ok(path) => path,
        Err(e) => fail_prerun(json, "beamfile", &e),
    };
    let content = match fs::read_to_string(&beamfile_path) {
        Ok(c) => c,
        Err(e) => fail_prerun(json, "beamfile", &anyhow::Error::from(e)),
    };
    let mut beam_file = match parse(&content) {
        Ok(bf) => bf,
        Err(e) => fail_prerun(json, "beamfile", &e),
    };

    if let Some(vars) = matches.get_many::<String>("var") {
        if let Err(e) = aurora::apply_var_overrides(&mut beam_file, vars) {
            fail_prerun(json, "variable", &e);
        }
    }

    // Resolve `var.<name>` references now that any --var override has been
    // applied, so the overrides actually take effect.
    if let Err(e) = aurora_core::parser::resolve_variables(&mut beam_file) {
        fail_prerun(json, "variable", &e);
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
        let target = aurora::resolve_target(
            &beam_file,
            matches.get_one::<String>("beam").map(|s| s.as_str()),
        )?;
        print_execution_plan(&beam_file, &target)?;
        return Ok(());
    }

    let interactive = !json
        && (matches.get_flag("interactive")
            || (std::io::stdout().is_terminal() && !matches.get_flag("no-tui")));

    // Target resolution: picker in interactive mode, `default` beam in headless mode
    // (the picker is inherently interactive and does not exist outside a TTY).
    let target = if interactive {
        if let Some(beam_name) = matches.get_one::<String>("beam") {
            // The picker can only ever yield beams that exist; an explicitly
            // named one must be validated exactly as in headless mode.
            aurora::ensure_beam_exists(&beam_file, beam_name)?;
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
                    variables: vec![],
                    args: vec![],
                    dir: None,
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
        match aurora::resolve_target(
            &beam_file,
            matches.get_one::<String>("beam").map(|s| s.as_str()),
        ) {
            Ok(target) => target,
            Err(e) => fail_prerun(json, "target", &e),
        }
    };

    // Positional arguments belong to the explicitly invoked target. Resolve
    // `${arg.N}` / `${args}` now that the target is known; a value that must
    // reach a dependency is a global variable, not an argument.
    let args: Vec<String> = matches
        .get_many::<String>("args")
        .map(|values| values.cloned().collect())
        .unwrap_or_default();
    if let Err(e) = aurora_core::parser::resolve_arguments(&mut beam_file, &target, &args) {
        fail_prerun(json, "argument", &e);
    }

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

    // Register community WASM executors discovered under ~/.aurora/plugins.
    // Native executors take precedence: a plugin cannot shadow local/docker.
    // Warnings go to stderr in the default mode, but are suppressed under
    // `--json`, which keeps stderr clean and reserves stdout for the event
    // stream.
    let plugin_registration =
        aurora::plugins::register_plugins(&mut executors, aurora::plugins::discover_plugins());
    if !json {
        for warning in &plugin_registration.warnings {
            eprintln!("{warning}");
        }
    }

    // `beamfile_path` always ends with the `Beamfile` component, so it has a
    // parent; fall back to the current directory rather than panic if not.
    let working_dir = beamfile_path
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();

    // Evaluate environment variables (shell(...)) sequentially. When no
    // `environment { }` block is declared, fall back to the allowlisted base
    // environment, never to the full process environment: a Beamfile is
    // untrusted and must not inherit ambient secrets (CI tokens, AWS_*, ...).
    let env = match &beam_file.environment {
        Some(env_block) => match evaluate(env_block, &working_dir) {
            Ok(env) => env,
            Err(e) => fail_prerun(json, "beamfile", &e),
        },
        None => aurora_core::env::base_env(),
    };

    // The declared half of the environment takes part in every beam's cache key:
    // a `shell(...)` value that changes (a commit sha, a branch) changes what the
    // beams produce without changing any of their input files. The ambient half
    // stays out of the key (see `env::declared_only`).
    let declared_env = aurora_core::env::declared_only(beam_file.environment.as_ref(), &env);

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
    let scheduler = aurora::build_scheduler(
        beams,
        executors.clone(),
        tx,
        beam_file.config.as_ref().and_then(|c| c.max_parallelism),
        working_dir.clone(),
        env.clone(),
        declared_env.clone(),
        !no_cache,
    );

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
        let rerun_declared_env = declared_env.clone();

        let rerun = move |root: String,
                          pre_success: Vec<String>|
              -> (
            mpsc::Receiver<SchedulerEvent>,
            mpsc::UnboundedSender<String>,
        ) {
            let (tx, rx) = mpsc::channel(128);
            let (cancel_tx, cancel_rx) = mpsc::unbounded_channel::<String>();
            let scheduler = aurora::build_scheduler(
                rerun_beams.clone(),
                rerun_executors.clone(),
                tx,
                rerun_max_par,
                rerun_working_dir.clone(),
                rerun_env.clone(),
                rerun_declared_env.clone(),
                !no_cache,
            );
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
        // Headless mode: no interactive cancellation, `run` manages its own
        // channel. Ctrl-C and SIGTERM tear the run down instead of killing
        // Aurora outright, which would orphan the beams' process subtrees.
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let interrupted = Arc::new(AtomicBool::new(false));
        let signalled = interrupted.clone();
        tokio::spawn(async move {
            aurora::wait_for_termination_signal().await;
            signalled.store(true, Ordering::SeqCst);
            let _ = shutdown_tx.send(());
        });

        let scheduler = scheduler.with_shutdown(shutdown_rx);
        let target_clone = target.clone();
        let handle = tokio::spawn(async move { scheduler.run(&target_clone, &[]).await });

        let beam_names: Vec<String> = beam_info.iter().map(|(name, _)| name.clone()).collect();
        // run_started.beams is the target's dependency closure, not every declared
        // beam. Fall back to the full list if the graph cannot be built (a cycle):
        // the scheduler then surfaces that error as an event.
        let json_beams: Vec<String> = if json {
            aurora_core::dag::BeamGraph::from_deps(beam_info.clone())
                .ok()
                .and_then(|graph| graph.execution_levels(&target).ok())
                .map(|levels| levels.into_iter().flatten().collect())
                .unwrap_or_else(|| beam_names.clone())
        } else {
            beam_names.clone()
        };
        // Color decided per stream: stdout and stderr can be redirected
        // independently (e.g. `aurora --no-tui 2>err.log` in a terminal).
        let out_color = std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        let err_color = std::io::stderr().is_terminal() && std::env::var_os("NO_COLOR").is_none();
        let mut stdout = std::io::stdout();
        let mut stderr = std::io::stderr();
        use aurora::reporter::Reporter;
        let mut reporter: Box<dyn Reporter> = if json {
            Box::new(aurora::json::JsonReporter::new(
                target.clone(),
                json_beams,
                &mut stdout,
            ))
        } else {
            Box::new(headless::HeadlessReporter::new(
                beam_names.clone(),
                out_color,
                err_color,
                &mut stdout,
                &mut stderr,
            ))
        };
        let success = reporter.run(rx).await?;

        // The scheduler can fail before emitting AllDone (DAG construction error:
        // cycle, unknown dependency). We join its task to propagate
        // the failure, otherwise an invalid Beamfile would exit with 0.
        let scheduler_ok = match handle.await {
            Ok(Ok(ok)) => ok,
            Ok(Err(e)) => {
                if json {
                    let mut stdout = std::io::stdout();
                    let _ = aurora::json::write_error(&mut stdout, "beamfile", &e.to_string());
                } else {
                    eprintln!("Scheduler error: {}", e);
                }
                false
            }
            Err(e) => {
                if json {
                    let mut stdout = std::io::stdout();
                    let _ = aurora::json::write_error(&mut stdout, "internal", &e.to_string());
                } else {
                    eprintln!("Scheduler task panicked: {}", e);
                }
                false
            }
        };
        // 128 + SIGINT(2): the shell convention for "terminated by interrupt",
        // and distinct from the plain 1 of a beam that failed on its own.
        if interrupted.load(Ordering::SeqCst) {
            if !json {
                eprintln!("aurora: interrupted, cancelled the running beams");
            }
            std::process::exit(130);
        }
        if !success || !scheduler_ok {
            std::process::exit(1);
        }
    }

    Ok(())
}

/// Reports a failure that happens before any beam runs (Beamfile parsing,
/// `--var` overrides, target/argument resolution) and exits the process.
///
/// Under `--json` this must be the sole output: an `error` event on stdout,
/// nothing on stderr, so a consumer parsing NDJSON never has to also watch
/// stderr for a pre-run failure. Outside `--json`, behavior is unchanged:
/// the message goes to stderr, exactly as the former `anyhow` bail did.
fn fail_prerun(json: bool, kind: &str, err: &anyhow::Error) -> ! {
    if json {
        let mut stdout = std::io::stdout();
        let _ = aurora::json::write_error(&mut stdout, kind, &err.to_string());
    } else {
        // Matches the format the default `Result<(), E: Debug>` process
        // termination used before this function took over: `{err:?}`, not
        // `{err}`, so a wrapped `anyhow::Error` still prints its full
        // "Caused by:" chain (e.g. the pest parse error location).
        eprintln!("Error: {err:?}");
    }
    std::process::exit(1);
}

fn find_beamfile(json: bool) -> Result<PathBuf> {
    let start = std::env::current_dir()?;
    let mut dir = start.clone();
    loop {
        let candidate = dir.join("Beamfile");
        if candidate.exists() {
            // A Beamfile runs arbitrary commands. When it is picked up from an
            // ancestor directory (not the one Aurora was launched in), warn:
            // the user may not expect that ancestor's beams to execute. Under
            // `--json` stdout is the sole output contract, so stay silent.
            if dir != start && !json {
                eprintln!(
                    "aurora: using Beamfile from a parent directory: {}",
                    candidate.display()
                );
            }
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
