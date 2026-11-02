//! nixstore CLI — read-only path-info over db.sqlite.

use std::fmt;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand};
use id_effect::{Effect, HNil, fail, succeed, suspend};
use id_effect_cli::{RunMainConfig, run_main};

use nixstore::caps::layer;
use nixstore::commands::{PathInfoFlags, PathInfoReport, path_info, resolve_store_root};
use nixstore::error::InfraError;

#[derive(Parser)]
#[command(
    name = "nixstore",
    about = "Read-only Nix store path-info from db.sqlite",
    // Free `-h` for nix-compatible `--human-readable`.
    disable_help_flag = true
)]
struct Cli {
  #[command(subcommand)]
  command: Commands,
}

#[derive(Subcommand)]
enum Commands {
  /// Query information about store paths (nix path-info compatible)
  PathInfo {
    /// Store paths to query
    paths: Vec<String>,
    /// Emit JSON object keyed by path
    #[arg(long)]
    json: bool,
    /// Print NAR size
    #[arg(long, short = 's')]
    size: bool,
    /// Print closure size
    #[arg(long, short = 'S')]
    closure_size: bool,
    /// Human-readable sizes
    #[arg(long, short = 'h')]
    human_readable: bool,
    /// Show signatures
    #[arg(long)]
    sigs: bool,
    /// Also show referrers
    #[arg(long)]
    referrers: bool,
    /// Alternate store root (default /nix)
    #[arg(long)]
    store: Option<PathBuf>,
  },
}

fn main() -> ExitCode {
  let mut cmd = Cli::command();
  cmd = cmd.arg(
    clap::Arg::new("help")
      .long("help")
      .action(ArgAction::Help)
      .global(true)
      .help("Print help"),
  );
  let matches = cmd.get_matches();
  let cli = Cli::from_arg_matches(&matches).unwrap_or_else(|e| e.exit());
  match cli.command {
    Commands::PathInfo {
      paths,
      json,
      size,
      closure_size,
      human_readable,
      sigs,
      referrers,
      store,
    } => {
      if paths.is_empty() {
        eprintln!("nixstore path-info: at least one PATH required");
        return ExitCode::from(2);
      }
      let flags = PathInfoFlags {
        json,
        size,
        closure_size,
        human_readable,
        sigs,
        referrers,
      };
      let root = resolve_store_root(store);
      let program = path_info(paths, flags.clone())
        .provide_layer(layer(root))
        .flat_map(move |report| print_report(report, flags))
        .map_err(CliError);
      // Ctrl-C interrupts (finalizers run, exit 130); typed failure 1; defect 101.
      run_main(program, HNil, RunMainConfig::minimal())
    }
  }
}

/// Writes the rendered report (plus newline) to stdout.
fn print_report(report: PathInfoReport, flags: PathInfoFlags) -> Effect<(), InfraError> {
  suspend(move || {
    let mut out = std::io::stdout().lock();
    match writeln!(out, "{}", report.render(&flags)) {
      Ok(()) => succeed(()),
      Err(e) => fail(InfraError::Io {
        path: "<stdout>".into(),
        message: e.to_string(),
      }),
    }
  })
}

/// `run_main` prints failures with `Debug`; this shows the `Display` message instead.
struct CliError(InfraError);

impl fmt::Debug for CliError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Display::fmt(&self.0, f)
  }
}
