//! Aurora CLI - Command-line interface for the Aurora build system.

mod commands;
mod discovery;
mod output;

use std::process::ExitCode;

use clap::{Parser, Subcommand};
use miette::Result;

#[derive(Parser)]
#[command(name = "aurora")]
#[command(
    author,
    version,
    about = "A next-generation task automation and build system"
)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Target beam to run (default beam if not specified)
    #[arg(value_name = "BEAM")]
    target: Option<String>,

    /// Maximum number of parallel jobs
    #[arg(short = 'j', long, default_value = "0")]
    parallel: usize,

    /// Show what would be executed without running
    #[arg(long)]
    dry_run: bool,

    /// Disable build cache
    #[arg(long)]
    no_cache: bool,

    /// Path to Beamfile (auto-detected if not specified)
    #[arg(short = 'f', long)]
    file: Option<String>,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a specific beam
    Run {
        /// Beam to execute
        beam: String,

        /// Maximum parallel jobs
        #[arg(short = 'j', long, default_value = "0")]
        parallel: usize,

        /// Dry run mode
        #[arg(long)]
        dry_run: bool,

        /// Disable cache
        #[arg(long)]
        no_cache: bool,
    },

    /// List all available beams
    List {
        /// Show detailed information
        #[arg(short, long)]
        detailed: bool,
    },

    /// Show dependency graph
    Graph {
        /// Target beam (shows all if not specified)
        beam: Option<String>,

        /// Output format (ascii, dot)
        #[arg(short, long, default_value = "ascii")]
        format: String,
    },

    /// Validate Beamfile syntax
    Validate,

    /// Cache management
    Cache {
        #[command(subcommand)]
        action: CacheAction,
    },

    /// Initialize a new Beamfile
    Init {
        /// Force overwrite existing Beamfile
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
enum CacheAction {
    /// Clear all cached data
    Clean,

    /// Show cache status
    Status,
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();

    let result = run(cli).await;

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{:?}", e);
            ExitCode::FAILURE
        }
    }
}

async fn run(cli: Cli) -> Result<()> {
    // Handle init command separately (doesn't need Beamfile)
    if let Some(Commands::Init { force }) = &cli.command {
        return commands::init::execute(*force);
    }

    // Find Beamfile
    let beamfile_path = match cli.file {
        Some(path) => std::path::PathBuf::from(path),
        None => discovery::find_beamfile()?,
    };

    match cli.command {
        Some(Commands::Run {
            beam,
            parallel,
            dry_run,
            no_cache,
        }) => commands::run::execute(&beamfile_path, &beam, parallel, dry_run, !no_cache).await,

        Some(Commands::List { detailed }) => commands::list::execute(&beamfile_path, detailed),

        Some(Commands::Graph { beam, format }) => {
            commands::graph::execute(&beamfile_path, beam.as_deref(), &format)
        }

        Some(Commands::Validate) => commands::validate::execute(&beamfile_path),

        Some(Commands::Cache { action }) => match action {
            CacheAction::Clean => commands::cache::clean(&beamfile_path),
            CacheAction::Status => commands::cache::status(&beamfile_path),
        },

        Some(Commands::Init { .. }) => unreachable!("Init is handled earlier"),

        None => {
            // Run default or specified target
            let target = match cli.target {
                Some(t) => t,
                None => {
                    // Get default beam from Beamfile
                    let beamfile = aurora_parser::parse_file(&beamfile_path)
                        .map_err(|e| miette::miette!("{}", e))?;

                    beamfile.default_beam.ok_or_else(|| {
                        miette::miette!("No target specified and no default beam defined")
                    })?
                }
            };

            commands::run::execute(
                &beamfile_path,
                &target,
                cli.parallel,
                cli.dry_run,
                !cli.no_cache,
            )
            .await
        }
    }
}
