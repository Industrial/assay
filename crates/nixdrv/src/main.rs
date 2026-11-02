//! nixdrv CLI — derivation parse/project and store-path helpers.
//!
//! Exit codes: `0` ok; `1` typed failure (unreadable input, not a derivation, invalid store
//! path or argument); `2` usage errors; `101` defect; `130` Ctrl-C (finalizers run).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use id_effect::{Effect, HNil};
use id_effect_cli::{RunMainConfig, run_main};

use nixdrv::caps::layer;
use nixdrv::commands::{
  DerivationReport, FixedPathReport, Projection, StorePathReport, parse, project_fields,
  store_path_make_fixed, store_path_parse,
};
use nixdrv::error::InfraError;
use nixstore::edge::{CliError, from_result, print_line};

#[derive(Parser)]
#[command(
  name = "nixdrv",
  about = "Derivation ATerm/JSON parse, project fields, store-path helpers"
)]
struct Cli {
  #[command(subcommand)]
  command: Commands,
}

#[derive(Subcommand)]
enum Commands {
  /// Parse a .drv (ATerm) or JSON derivation file
  Parse {
    #[arg(long, short = 'f', default_value = "-")]
    file: PathBuf,
  },
  /// Project selected derivation fields to JSON
  Project {
    #[arg(long, short = 'f', default_value = "-")]
    file: PathBuf,
    #[arg(long, value_delimiter = ',')]
    fields: Vec<String>,
  },
  #[command(subcommand, name = "store-path")]
  StorePath(StorePathCommands),
}

#[derive(Subcommand)]
enum StorePathCommands {
  /// Parse a Nix store path into hash, name, and full path
  Parse { path: String },
  /// Build a fixed-output store path from a known digest
  MakeFixed {
    #[arg(long)]
    name: String,
    #[arg(long, value_parser = ["flat", "recursive"])]
    method: String,
    #[arg(long)]
    hash_algo: String,
    #[arg(long)]
    digest_hex: String,
    #[arg(long)]
    store_dir: Option<String>,
  },
}

fn main() -> ExitCode {
  match Cli::parse().command {
    Commands::Parse { file } => run(parse(file).provide_layer(layer()), DerivationReport::render),
    Commands::Project { file, fields } => run(
      project_fields(file, fields).provide_layer(layer()),
      Projection::render,
    ),
    Commands::StorePath(StorePathCommands::Parse { path }) => {
      run(store_path_parse(path), StorePathReport::render)
    }
    Commands::StorePath(StorePathCommands::MakeFixed {
      name,
      method,
      hash_algo,
      digest_hex,
      store_dir,
    }) => run(
      store_path_make_fixed(name, method, hash_algo, digest_hex, store_dir),
      FixedPathReport::render,
    ),
  }
}

/// Runs `command` and prints its output rendered by `render` (plus newline). A typed failure
/// exits 1 (Ctrl-C 130, defect 101).
fn run<A: Send + 'static>(
  command: Effect<A, InfraError>,
  render: fn(&A) -> Result<String, InfraError>,
) -> ExitCode {
  let program = command
    .flat_map(move |out| from_result(render(&out)))
    .flat_map(|text| print_line::<nixq::InfraError>(text).map_err(InfraError::from))
    .map_err(CliError);
  run_main(program, HNil, RunMainConfig::minimal())
}
