//! nixq CLI — JSON/attrpath query on id_effect.
//!
//! Exit codes: `0` ok / predicate true / documents equal; `1` predicate false, documents differ
//! or a typed failure of a JSON command (I/O, bad JSON, attrpath); `2` usage errors and any
//! `path-info` failure; `101` defect; `130` Ctrl-C (finalizers run).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{ArgAction, CommandFactory, FromArgMatches, Parser, Subcommand};
use id_effect::{Effect, succeed};

use nixq::caps::{NixqEnv, layer};
use nixq::commands::{diff, force_path, get, has_attrs, normalize, path_info, subset};
use nixq::error::{InfraError, PredicateResult};
use nixstore::edge::{CliError, print_line, run_main_coded};

#[derive(Parser)]
#[command(
    name = "nixq",
    about = "JSON/attrpath query + path-info via nixstore",
    // Free `-h` for nix-compatible `--human-readable` on path-info.
    disable_help_flag = true
)]
struct Cli {
  #[command(subcommand)]
  command: Commands,
}

#[derive(Subcommand)]
enum Commands {
  /// Get value at attrpath
  Get {
    attrpath: String,
    #[arg(long, short = 'f', default_value = "-")]
    file: PathBuf,
  },
  /// Exit 0 if object has all attrs
  HasAttrs {
    attrs: Vec<String>,
    #[arg(long, short = 'f', default_value = "-")]
    file: PathBuf,
  },
  /// Exit 0 if file is a structural subset of --expected
  Subset {
    #[arg(long)]
    expected: PathBuf,
    #[arg(long, short = 'f', default_value = "-")]
    file: PathBuf,
  },
  /// Exit 0 if all attrpaths resolve
  ForcePath {
    paths: Vec<String>,
    #[arg(long, short = 'f', default_value = "-")]
    file: PathBuf,
  },
  /// Print normalized JSON
  Normalize {
    #[arg(long, short = 'f', default_value = "-")]
    file: PathBuf,
  },
  /// Print structural diff vs --right (empty if equal after normalize)
  Diff {
    #[arg(long)]
    right: PathBuf,
    #[arg(long, short = 'f', default_value = "-")]
    file: PathBuf,
  },
  /// Query Nix store path-info (delegates to nixstore)
  PathInfo {
    paths: Vec<String>,
    #[arg(long)]
    json: bool,
    #[arg(long, short = 's')]
    size: bool,
    #[arg(long, short = 'S')]
    closure_size: bool,
    #[arg(long, short = 'h')]
    human_readable: bool,
    #[arg(long)]
    sigs: bool,
    #[arg(long)]
    referrers: bool,
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
    Commands::Get { attrpath, file } => {
      run_json(get(file, attrpath), |r| emit(Some(r.render()), 0))
    }
    Commands::HasAttrs { attrs, file } => run_json(has_attrs(file, attrs), predicate),
    Commands::Subset { expected, file } => run_json(subset(file, expected), predicate),
    Commands::ForcePath { paths, file } => run_json(force_path(file, paths), predicate),
    Commands::Normalize { file } => run_json(normalize(file), |r| emit(Some(r.render()), 0)),
    Commands::Diff { right, file } => run_json(diff(file, right), |d| {
      emit(d.render().map(str::to_owned), d.exit_code())
    }),
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
        eprintln!("nixq path-info: at least one PATH required");
        return ExitCode::from(2);
      }
      let flags = nixstore::PathInfoFlags {
        json,
        size,
        closure_size,
        human_readable,
        sigs,
        referrers,
      };
      let root = nixstore::resolve_store_root(store);
      let program = path_info(paths, flags.clone())
        .provide_layer(nixstore::layer(root))
        .flat_map(move |report| emit(Some(report.render(&flags)), 0))
        .map_err(CliError);
      // Any path-info failure (unknown path, missing db, ...) exits 2.
      run_main_coded(program, 2)
    }
  }
}

/// Runs a JSON command on the live `JsonSource`; `show` turns its output into stdout + exit code.
/// A typed failure exits 1 (Ctrl-C 130, defect 101).
fn run_json<A: Send + 'static>(
  command: Effect<A, InfraError, NixqEnv>,
  show: fn(A) -> Effect<u8, InfraError>,
) -> ExitCode {
  let program = command
    .provide_layer(layer())
    .flat_map(show)
    .map_err(CliError);
  run_main_coded(program, 1)
}

/// Prints `stdout` (when any, plus newline) and ends with exit code `code`.
fn emit(stdout: Option<String>, code: u8) -> Effect<u8, InfraError> {
  match stdout {
    Some(text) => print_line(text).map(move |()| code),
    None => succeed(code),
  }
}

/// Predicates print nothing: true exits 0, false exits 1.
fn predicate(result: PredicateResult) -> Effect<u8, InfraError> {
  emit(None, result.exit_code())
}
