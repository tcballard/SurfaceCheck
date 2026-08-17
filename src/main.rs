use std::path::{Path, PathBuf};
use std::process::ExitCode;

use anyhow::Result;
use clap::{Parser, Subcommand};
use surfacecheck::config::Config;

#[derive(Debug, Parser)]
#[command(
    name = "surfacecheck",
    version,
    about = "Check whether a software project's public surfaces agree"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Create a configuration from the current project.
    Init {
        #[arg(long, default_value = "surfacecheck.toml")]
        output: PathBuf,
        #[arg(long)]
        force: bool,
    },
    /// Check local files, public URLs, and GitHub facts.
    Check {
        #[arg(short, long, default_value = "surfacecheck.toml")]
        config: PathBuf,
        #[arg(long)]
        json: bool,
        /// Skip URL and GitHub checks.
        #[arg(long)]
        offline: bool,
    },
}

fn main() -> ExitCode {
    match execute() {
        Ok(code) => ExitCode::from(code),
        Err(error) => {
            eprintln!("surfacecheck: {error:#}");
            ExitCode::from(2)
        }
    }
}

fn execute() -> Result<u8> {
    let cli = Cli::parse();
    match cli.command {
        Command::Init { output, force } => {
            let path = surfacecheck::init::create(Path::new("."), &output, force)?;
            println!("created {}", path.display());
            Ok(0)
        }
        Command::Check {
            config,
            json,
            offline,
        } => {
            let loaded = Config::load(&config)?;
            let root = config.parent().unwrap_or_else(|| Path::new("."));
            let report = surfacecheck::check::run(&loaded, root, offline)?;
            if json {
                surfacecheck::output::print_json(&report)?;
            } else {
                surfacecheck::output::print_human(&report);
            }
            Ok(u8::from(report.failed > 0))
        }
    }
}
