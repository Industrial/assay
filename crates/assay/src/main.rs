//! Assay CLI — discover and run Nix unit test suites.
//!
//! Exit codes: `0` every case passed; `1` any case did not pass, a suite could not be loaded (or
//! the path is missing), a bad `--format`, or a `discover` / `laws` failure; `2` usage errors
//! (clap); `101` a defect (in the runner or a case); `130` Ctrl-C (running `nix` children are
//! killed).

use std::fmt;
use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;

use clap::{Parser, Subcommand};
use id_effect::{Effect, suspend};
use nixstore::edge::{CliError, from_result, run_main_coded_by};
use serde::Serialize;

use assay::{
  AssayOutcome, InfraError, ReportFormat, RunOptions, SnapshotStore, Trace, default_snapshot_root,
  discover, fs_layer, layer, render_report, run_path, store_root_from, summarize,
};

#[derive(Parser)]
#[command(
  name = "assay",
  about = "Nix unit testing: discover and run assay suites",
  version
)]
struct Cli {
  #[command(subcommand)]
  command: Commands,
}

#[derive(Subcommand)]
enum Commands {
  Run {
    path: PathBuf,
    #[arg(long)]
    json: bool,
    #[arg(long, value_name = "FORMAT")]
    format: Option<String>,
    #[arg(long)]
    update_snapshots: bool,
    #[arg(long, value_name = "MS")]
    case_timeout_ms: Option<u64>,
    #[arg(long)]
    retry_flaky_eval: bool,
    /// Disable tryEval mega-batch (one nix process per claim).
    #[arg(long)]
    no_batch: bool,
  },
  Discover {
    path: PathBuf,
  },
  Laws {
    #[arg(long, default_value_t = 0)]
    seed: u64,
    #[arg(long)]
    json: bool,
  },
}

/// Why a command failed (every one exits 1).
#[derive(Debug)]
enum MainError {
  /// A bad `--format` / `--json` combination.
  Usage(String),
  Infra(InfraError),
}

impl fmt::Display for MainError {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Self::Usage(m) => write!(f, "assay: {m}"),
      Self::Infra(e) => write!(f, "assay: {e}"),
    }
  }
}

impl From<InfraError> for MainError {
  fn from(e: InfraError) -> Self {
    Self::Infra(e)
  }
}

#[derive(Serialize)]
struct JsonOutcome {
  name: String,
  outcome: AssayOutcome,
}

fn main() -> ExitCode {
  let program = match Cli::parse().command {
    Commands::Run {
      path,
      json,
      format,
      update_snapshots,
      case_timeout_ms,
      retry_flaky_eval,
      no_batch,
    } => cmd_run(
      path,
      json,
      format,
      RunOptions {
        update_snapshots,
        case_timeout_ms,
        retry_flaky_eval,
        batch_eval: !no_batch,
        trace: trace_from_env(),
        ..RunOptions::default()
      },
    ),
    Commands::Discover { path } => cmd_discover(path),
    Commands::Laws { seed, json } => cmd_laws(seed, json),
  };
  run_main_coded_by(program.map_err(CliError), |_| 1)
}

/// `assay run`: runs the suite (or every suite under a directory) and prints the report; exits
/// with the worst case's code.
fn cmd_run(
  path: PathBuf,
  json: bool,
  format: Option<String>,
  mut opts: RunOptions,
) -> Effect<u8, MainError> {
  let report_format = match resolve_format(json, format.as_deref()) {
    Ok(f) => f,
    Err(e) => return id_effect::fail(MainError::Usage(e)),
  };
  opts.json_output = report_format == ReportFormat::Json;
  let snapshots = SnapshotStore::new(default_snapshot_root()).with_update(opts.update_snapshots);
  let store_root = store_root_from(std::env::var_os("ASSAY_NIX_STORE"));
  let trace = opts.trace.clone();
  run_path(&path, &opts)
    .provide_layer(layer::<InfraError>(store_root, snapshots))
    .map_err(MainError::from)
    .flat_map(move |report| {
      trace.note("run".into());
      let summary = summarize(&report);
      let text = render_report(&report.legacy_outcomes(), report_format, &summary);
      let code = report.exit_byte();
      from_result(text.map_err(MainError::from))
        .flat_map(print_text)
        .map(move |()| code)
    })
}

fn resolve_format(json: bool, format: Option<&str>) -> Result<ReportFormat, String> {
  match (json, format) {
    (true, Some(f)) => {
      let parsed = ReportFormat::parse(f).ok_or_else(|| format!("unknown report format: {f}"))?;
      if parsed != ReportFormat::Json {
        return Err(format!("--json conflicts with --format {f}"));
      }
      Ok(parsed)
    }
    (true, None) => Ok(ReportFormat::Json),
    (false, Some(f)) => ReportFormat::parse(f)
      .ok_or_else(|| format!("unknown report format: {f} (expected human, json, or tap)")),
    (false, None) => Ok(ReportFormat::Human),
  }
}

/// `assay discover`: one line per suite file.
fn cmd_discover(path: PathBuf) -> Effect<u8, MainError> {
  discover(&path)
    .provide_layer(fs_layer::<InfraError>())
    .map_err(move |e| MainError::Infra(InfraError::Io(format!("discover {}: {e}", path.display()))))
    .flat_map(|suites| {
      let text: String = suites
        .iter()
        .map(|s| format!("{} ({:?})\n", s.path.display(), s.kind))
        .collect();
      print_text(text)
    })
    .map(|()| 0)
}

/// `assay laws`: runs the built-in laws; exits 1 when any fails.
fn cmd_laws(seed: u64, json: bool) -> Effect<u8, MainError> {
  suspend(move || {
    let laws = assay::run_builtin_laws(seed);
    let code = u8::from(laws.iter().any(|(_, o)| *o != AssayOutcome::Pass));
    let text = if json {
      let rows: Vec<JsonOutcome> = laws
        .into_iter()
        .map(|(name, outcome)| JsonOutcome {
          name: name.to_string(),
          outcome,
        })
        .collect();
      serde_json::to_string_pretty(&rows)
        .map(|s| format!("{s}\n"))
        .map_err(|e| MainError::Infra(InfraError::Io(format!("render laws: {e}"))))
    } else {
      Ok(
        laws
          .into_iter()
          .map(|(name, outcome)| {
            let mark = if outcome == AssayOutcome::Pass {
              "PASS"
            } else {
              "FAIL"
            };
            format!("{mark} {name}\n")
          })
          .collect(),
      )
    };
    from_result(text).flat_map(print_text).map(move |()| code)
  })
}

/// Writes `text` to stdout as is (a human report has no trailing newline).
fn print_text(text: String) -> Effect<(), MainError> {
  suspend(move || {
    let mut out = std::io::stdout().lock();
    from_result(
      out
        .write_all(text.as_bytes())
        .and_then(|()| out.flush())
        .map_err(|e| MainError::Infra(InfraError::Io(format!("<stdout>: {e}")))),
    )
  })
}

/// With `ASSAY_TRACE` set, progress notes go to stderr with the time since the previous note
/// (wall-clock timing lives here, at the edge).
fn trace_from_env() -> Trace {
  if std::env::var_os("ASSAY_TRACE").is_none() {
    return Trace::default();
  }
  let last = Arc::new(Mutex::new(Instant::now()));
  Trace::to(move |note| {
    let mut last = last.lock().unwrap_or_else(PoisonError::into_inner);
    let now = Instant::now();
    eprintln!(
      "assay_trace: {note} {:.1}ms",
      now.duration_since(*last).as_secs_f64() * 1000.0
    );
    *last = now;
  })
}
