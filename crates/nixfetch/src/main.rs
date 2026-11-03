//! nixfetch CLI — hash / verify / fetch-url / fetch-git / store-path.
//!
//! Exit codes: `0` ok; `1` hash mismatch (and only that); `2` every other failure (io, parse,
//! http, git, timeouts) and usage errors; `101` defect; `130` Ctrl-C (finalizers run, so a
//! `fetch-git` staging dir is removed).

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use id_effect::{Effect, fail};

use nixfetch::{
  DigestReport, FetchGitReport, FetchUrlReport, InfraError, RetryPolicy, StorePathReport,
  VerifyReport, fetch_git, fetch_url, hash, layer, store_path, verify,
};
use nixstore::edge::{CliError, print_line, run_main_coded_by};

#[derive(Debug, Parser)]
#[command(
  name = "nixfetch",
  about = "Fixed-output fetchers: hash, verify, fetchurl/fetchgit"
)]
struct Cli {
  #[command(subcommand)]
  command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
  /// Hash a path (flat file bytes or recursive NAR)
  Hash {
    path: PathBuf,
    #[arg(long, group = "mode")]
    flat: bool,
    #[arg(long, group = "mode")]
    recursive: bool,
  },
  /// Verify a path against an expected digest (SRI, nix32, or hex)
  Verify {
    path: PathBuf,
    #[arg(long)]
    expected: String,
    #[arg(long, group = "mode")]
    flat: bool,
    #[arg(long, group = "mode")]
    recursive: bool,
  },
  /// Download URL and flat-verify against expected hash
  FetchUrl {
    url: String,
    #[arg(long)]
    hash: String,
    #[arg(long)]
    out: Option<PathBuf>,
    #[arg(long)]
    name: Option<String>,
  },
  /// Export git rev and recursive-verify against expected hash
  FetchGit {
    url: String,
    #[arg(long)]
    rev: String,
    #[arg(long)]
    hash: String,
    #[arg(long)]
    dest: Option<PathBuf>,
    #[arg(long)]
    name: Option<String>,
  },
  /// Compute nixdrv fixed-output store path from digest + name
  StorePath {
    #[arg(long)]
    name: String,
    #[arg(long)]
    hash: String,
    #[arg(long)]
    recursive: bool,
    #[arg(long)]
    store_dir: Option<String>,
  },
}

fn ingestion_recursive(flat: bool, recursive: bool) -> Result<bool, InfraError> {
  match (flat, recursive) {
    (true, false) => Ok(false),
    (false, true) => Ok(true),
    (false, false) => Ok(false),
    (true, true) => Err(InfraError::Parse(
      "pass only one of --flat or --recursive".into(),
    )),
  }
}

/// `command(recursive)` for a valid `--flat` / `--recursive` pair, else its parse failure.
fn with_mode<A, R>(
  flat: bool,
  recursive: bool,
  command: impl FnOnce(bool) -> Effect<A, InfraError, R>,
) -> Effect<A, InfraError, R> {
  match ingestion_recursive(flat, recursive) {
    Ok(rec) => command(rec),
    Err(e) => fail(e),
  }
}

fn main() -> ExitCode {
  let policy = RetryPolicy::DEFAULT;
  match Cli::parse().command {
    Commands::Hash {
      path,
      flat,
      recursive,
    } => run(
      with_mode(flat, recursive, |rec| hash(path, rec)).provide_layer(layer()),
      DigestReport::render,
    ),
    Commands::Verify {
      path,
      expected,
      flat,
      recursive,
    } => run(
      with_mode(flat, recursive, |rec| verify(path, expected, rec)).provide_layer(layer()),
      VerifyReport::render,
    ),
    Commands::FetchUrl {
      url,
      hash,
      out,
      name,
    } => run(
      fetch_url(url, hash, out, name, policy).provide_layer(layer()),
      FetchUrlReport::render,
    ),
    Commands::FetchGit {
      url,
      rev,
      hash,
      dest,
      name,
    } => run(
      fetch_git(url, rev, hash, dest, name, policy).provide_layer(layer()),
      FetchGitReport::render,
    ),
    Commands::StorePath {
      name,
      hash,
      recursive,
      store_dir,
    } => run(
      store_path(name, hash, recursive, store_dir),
      StorePathReport::render,
    ),
  }
}

/// Runs `command` and prints its output rendered by `render` (plus newline). A hash mismatch
/// exits 1, every other typed failure 2 (Ctrl-C 130, defect 101).
fn run<A: Send + 'static>(
  command: Effect<A, InfraError>,
  render: fn(&A) -> Result<String, InfraError>,
) -> ExitCode {
  let program = command
    .flat_map(move |out| match render(&out) {
      Ok(text) => print_line::<nixstore::InfraError>(text).map_err(InfraError::from),
      Err(e) => fail(e),
    })
    .map(|()| 0u8)
    .map_err(CliError);
  run_main_coded_by(program, |e: &CliError<InfraError>| e.0.exit_code())
}
