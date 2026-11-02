//! Run assay suites and collect each case's `Exit`.
//!
//! A run first evaluates every batchable claim in one `nix` process ([`run_batch`]), then runs
//! the other cases in parallel with [`for_each_par`] under `Concurrency::Bounded(jobs)` (default:
//! the machine's parallelism). Each such case may be retried (`--retry-flaky-eval`: a bounded
//! [`Schedule`], transient I/O failures only) and is bounded by `--case-timeout-ms`
//! (`timeout_fail` -> [`InfraError::Timeout`], the attempt is interrupted and its `nix` killed).
//! Waits and deadlines use the fiber's executor clock; nothing here reads the wall clock.

use std::fmt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use id_effect::{Concurrency, Effect, Exit, Schedule, for_each_par, need};
use serde::Serialize;

use crate::assay_suite::load_assay_suite;
use crate::batch::{partition_cases, run_batch};
use crate::caps::{AssayEnv, FsService, NixEvalService, SuiteEnv};
use crate::claims::{Claim, ClaimCaps, claim_caps, interpret_with};
use crate::compat::load_compat_suite;
use crate::discover::{SuiteKind, discover_suites, suite_kind};
use crate::eval::NixEval;
use crate::files::{EntryKind, Fs};
use crate::outcome::AssayOutcome;
use crate::verdict::{CaseVerdict, InfraError, case_exit_byte, exit_to_outcome};

/// Retries after the first attempt under `--retry-flaky-eval` (so 3 attempts at most).
pub const FLAKY_EVAL_RETRIES: u64 = 2;
/// Delay before each flaky-eval retry.
pub const FLAKY_EVAL_DELAY: Duration = Duration::from_millis(1);

/// Where `ASSAY_TRACE` progress notes go (an injected sink; off by default).
#[derive(Clone, Default)]
pub struct Trace(Option<TraceSink>);

type TraceSink = Arc<dyn Fn(&str) + Send + Sync>;

impl Trace {
  /// A sink that receives every note.
  pub fn to(sink: impl Fn(&str) + Send + Sync + 'static) -> Self {
    Self(Some(Arc::new(sink)))
  }

  /// Hands `note` to the sink, if any.
  pub fn note(&self, note: String) {
    if let Some(sink) = &self.0 {
      sink(&note);
    }
  }
}

impl fmt::Debug for Trace {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(if self.0.is_some() {
      "Trace(on)"
    } else {
      "Trace(off)"
    })
  }
}

#[derive(Debug, Clone)]
pub struct RunOptions {
  /// `--update-snapshots` (applied at the edge, where the snapshot store is built).
  pub update_snapshots: bool,
  pub json_output: bool,
  /// Per-case timeout budget in milliseconds (`None` = no limit).
  pub case_timeout_ms: Option<u64>,
  /// Retry transient eval failures ([`FLAKY_EVAL_RETRIES`] times, default off).
  pub retry_flaky_eval: bool,
  /// Collapse batchable claims into one `nix eval` via `tryEval` (default on).
  pub batch_eval: bool,
  /// How many cases (and suite loads) run at once.
  pub jobs: usize,
  pub trace: Trace,
}

/// The default parallelism: the machine's available parallelism (4 when unknown).
pub fn default_jobs() -> usize {
  std::thread::available_parallelism().map_or(4, |n| n.get())
}

impl Default for RunOptions {
  fn default() -> Self {
    Self {
      update_snapshots: false,
      json_output: false,
      case_timeout_ms: None,
      retry_flaky_eval: false,
      batch_eval: true,
      jobs: default_jobs(),
      trace: Trace::default(),
    }
  }
}

/// Every case of a run with its `Exit`, sorted by case name.
#[derive(Debug, Clone)]
pub struct SuiteReport {
  pub outcomes: Vec<(String, Exit<CaseVerdict, InfraError>)>,
}

impl SuiteReport {
  /// The run's exit code: the worst case (see [`case_exit_byte`]); `0` for no cases.
  pub fn exit_byte(&self) -> u8 {
    self
      .outcomes
      .iter()
      .map(|(_, exit)| case_exit_byte(exit))
      .max()
      .unwrap_or(0)
  }

  /// The legacy per-case outcomes, in report order.
  pub fn legacy_outcomes(&self) -> Vec<(String, AssayOutcome)> {
    self
      .outcomes
      .iter()
      .map(|(name, exit)| (name.clone(), exit_to_outcome(exit.clone())))
      .collect()
  }
}

#[derive(Debug, Clone, Copy, Default, Serialize, PartialEq, Eq)]
pub struct RunSummary {
  pub total: usize,
  pub passed: usize,
  pub failed: usize,
  pub errored: usize,
}

type NamedExit = (String, Exit<CaseVerdict, InfraError>);

/// `assay run <path>`: [`run_discovered`] when `path` is a directory, else [`run_suite`].
pub fn run_path(path: &Path, opts: &RunOptions) -> Effect<SuiteReport, InfraError, AssayEnv> {
  let (path, opts) = (path.to_path_buf(), opts.clone());
  claim_caps().flat_map(move |caps| {
    caps
      .fs
      .kind(&path)
      .flat_map(move |kind| {
        if kind == Some(EntryKind::Dir) {
          run_discovered_with(caps, path, opts)
        } else {
          run_suite_with(caps, path, opts)
        }
      })
      .any_env()
  })
}

/// Runs the suite file at `path`. A suite that cannot be loaded fails the run.
pub fn run_suite(path: &Path, opts: &RunOptions) -> Effect<SuiteReport, InfraError, AssayEnv> {
  let (path, opts) = (path.to_path_buf(), opts.clone());
  claim_caps().flat_map(move |caps| run_suite_with(caps, path, opts).any_env())
}

/// [`run_suite`] over explicit capability values.
pub fn run_suite_with(
  caps: ClaimCaps,
  path: PathBuf,
  opts: RunOptions,
) -> Effect<SuiteReport, InfraError> {
  let trace = opts.trace.clone();
  load_suite_cases(&caps.eval, &caps.fs, &path)
    .flat_map(move |cases| {
      trace.note(format!("load_suite ({} cases)", cases.len()));
      run_cases(cases, caps, opts)
    })
    .map(|outcomes| SuiteReport { outcomes })
}

/// Discovers every suite under `root`, loads them in parallel and runs all their cases as one
/// run (one batch for all batchable claims); case names are prefixed with the suite path.
pub fn run_discovered(root: &Path, opts: &RunOptions) -> Effect<SuiteReport, InfraError, AssayEnv> {
  let (root, opts) = (root.to_path_buf(), opts.clone());
  claim_caps().flat_map(move |caps| run_discovered_with(caps, root, opts).any_env())
}

/// [`run_discovered`] over explicit capability values.
pub fn run_discovered_with(
  caps: ClaimCaps,
  root: PathBuf,
  opts: RunOptions,
) -> Effect<SuiteReport, InfraError> {
  let jobs = Concurrency::Bounded(opts.jobs.max(1));
  let (eval, fs) = (Arc::clone(&caps.eval), Arc::clone(&caps.fs));
  discover_suites(&caps.fs, &root)
    .flat_map(move |suites| {
      for_each_par(
        suites,
        move |suite| {
          let prefix = suite.path.display().to_string();
          load_suite_cases(&eval, &fs, &suite.path).map(move |cases| {
            cases
              .into_iter()
              .map(|(name, claim)| (format!("{prefix}::{name}"), claim))
              .collect::<Vec<_>>()
          })
        },
        jobs,
      )
    })
    .flat_map(move |loaded| run_cases(loaded.into_iter().flatten().collect(), caps, opts))
    .map(|outcomes| SuiteReport { outcomes })
}

/// [`load_suite_cases`] with the capabilities from [`SuiteEnv`].
pub fn load_suite(path: &Path) -> Effect<Vec<(String, Claim)>, InfraError, SuiteEnv> {
  let path = path.to_path_buf();
  need::<NixEvalService>().flat_map(move |eval: Arc<NixEvalService>| {
    need::<FsService>()
      .flat_map(move |fs: Arc<FsService>| load_suite_cases(&eval.0, &fs.0, &path).any_env())
  })
}

/// The cases of one suite file (its kind decides the loader): `.nix` suites are evaluated with
/// `eval`, `.json` suites read with `fs`.
pub fn load_suite_cases(
  eval: &Arc<dyn NixEval>,
  fs: &Arc<dyn Fs>,
  path: &Path,
) -> Effect<Vec<(String, Claim)>, InfraError> {
  match suite_kind(path).unwrap_or(SuiteKind::CompatJson) {
    SuiteKind::CompatJson | SuiteKind::CompatNix => {
      load_compat_suite(Arc::clone(eval), Arc::clone(fs), path).map(|suite| {
        suite
          .cases
          .into_iter()
          .map(|c| {
            (
              c.name,
              Claim::Eq {
                left_expr: c.expr,
                right_expr: c.expected,
              },
            )
          })
          .collect()
      })
    }
    SuiteKind::AssayNix => load_assay_suite(Arc::clone(eval), Arc::clone(fs), path),
  }
}

/// Runs `cases`: the batchable ones in one batch, the rest in parallel; sorted by name.
pub fn run_cases(
  cases: Vec<(String, Claim)>,
  caps: ClaimCaps,
  opts: RunOptions,
) -> Effect<Vec<NamedExit>, InfraError> {
  let (batchable, isolated) = if opts.batch_eval {
    partition_cases(cases)
  } else {
    (Vec::new(), cases)
  };
  let trace = opts.trace.clone();
  run_batch(batchable, caps.clone(), opts.trace.clone()).flat_map(move |mut outcomes| {
    let jobs = Concurrency::Bounded(opts.jobs.max(1));
    for_each_par(
      isolated,
      move |(name, claim)| run_case(&caps, name, claim, &opts),
      jobs,
    )
    .map(move |isolated| {
      outcomes.extend(isolated);
      outcomes.sort_by(|a, b| a.0.cmp(&b.0));
      trace.note(format!("run_cases ({} cases)", outcomes.len()));
      outcomes
    })
  })
}

/// One isolated case: retried and bounded per `opts`; never fails (its `Exit` is the value).
fn run_case(
  caps: &ClaimCaps,
  name: String,
  claim: Claim,
  opts: &RunOptions,
) -> Effect<NamedExit, InfraError> {
  let caps = caps.clone();
  let attempt = move || interpret_with(&caps, claim.clone());
  let case = if opts.retry_flaky_eval {
    let schedule = Schedule::spaced(FLAKY_EVAL_DELAY)
      .intersect(Schedule::recurs(FLAKY_EVAL_RETRIES))
      .while_input(InfraError::is_transient);
    Effect::retry(attempt, schedule)
  } else {
    attempt()
  };
  let case = match opts.case_timeout_ms {
    Some(limit_ms) => {
      let case_name = name.clone();
      case.timeout_fail(Duration::from_millis(limit_ms), move || {
        InfraError::Timeout {
          case: case_name,
          limit_ms,
        }
      })
    }
    None => case,
  };
  case.exit::<InfraError>().map(move |exit| (name, exit))
}

pub fn summarize(report: &SuiteReport) -> RunSummary {
  summarize_exits(&report.outcomes)
}

pub fn summarize_exits(outcomes: &[(String, Exit<CaseVerdict, InfraError>)]) -> RunSummary {
  let mut s = RunSummary {
    total: outcomes.len(),
    ..Default::default()
  };
  for (_, exit) in outcomes {
    match exit_to_outcome(exit.clone()) {
      AssayOutcome::Pass => s.passed += 1,
      AssayOutcome::EvalError { .. }
      | AssayOutcome::Recursion
      | AssayOutcome::Timeout
      | AssayOutcome::ResourceLeak => s.errored += 1,
      _ => s.failed += 1,
    }
  }
  s
}

#[cfg(test)]
mod tests {
  use std::sync::atomic::{AtomicUsize, Ordering};

  use id_effect::{
    Executor, TestExecutor, assert_exit_fail, expect_exit_success, run_test, run_test_on, sleep,
  };
  use serde_json::{Value, json};

  use super::*;
  use crate::eval::EvalResult;
  use crate::fake::{MockNixEval, SyncEval, temp_snapshot_store};

  fn caps(eval: impl NixEval + 'static) -> ClaimCaps {
    ClaimCaps {
      eval: Arc::new(eval),
      snapshots: temp_snapshot_store(),
      path_info: Arc::new(nixstore::MockPathInfoStore::default()),
      fs: Arc::new(crate::fake::MemFs::default()),
    }
  }

  fn eq(l: &str, r: &str) -> Claim {
    Claim::Eq {
      left_expr: l.into(),
      right_expr: r.into(),
    }
  }

  fn throws(expr: &str) -> Claim {
    Claim::Throws {
      expr: expr.into(),
      pattern: None,
    }
  }

  fn opts() -> RunOptions {
    RunOptions {
      jobs: 2,
      ..RunOptions::default()
    }
  }

  #[test]
  fn batched_and_isolated_cases_are_collected_sorted_by_name() {
    let eval = MockNixEval::default();
    eval.set("1", EvalResult::Ok(json!(1)));
    eval.set("2", EvalResult::Ok(json!(2)));
    let cases = vec![
      ("c".to_string(), eq("1", "1")),
      ("a".to_string(), throws("1")),
      ("b".to_string(), eq("1", "2")),
    ];
    let mut no_batch = opts();
    no_batch.batch_eval = false;
    let out = expect_exit_success(run_test(run_cases(cases, caps(eval), no_batch)));
    let names: Vec<_> = out.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["a", "b", "c"]);
    assert!(matches!(out[2].1, Exit::Success(CaseVerdict::Pass)));
    assert!(matches!(
      out[1].1,
      Exit::Success(CaseVerdict::AssertFail { .. })
    ));
  }

  /// An evaluator whose every evaluation takes `delay` (on the executor clock).
  struct SlowEval(Duration);

  impl NixEval for SlowEval {
    fn eval_expr(&self, _expr: &str) -> Effect<Value, AssayOutcome> {
      sleep(self.0).map(|()| json!([1, 1]))
    }
    fn eval_file(&self, _path: &Path) -> Effect<Value, AssayOutcome> {
      sleep(self.0).map(|()| Value::Null)
    }
    fn eval_source(&self, _source: &str) -> Effect<Value, AssayOutcome> {
      sleep(self.0).map(|()| Value::Null)
    }
  }

  #[test]
  fn case_timeout_is_a_timeout_failure_after_exactly_the_limit() {
    let mut o = opts();
    o.batch_eval = false;
    o.case_timeout_ms = Some(50);
    let exec = TestExecutor::new();
    let start = exec.now();
    let run = run_cases(
      vec![("slow".to_string(), eq("1", "1"))],
      caps(SlowEval(Duration::from_secs(60))),
      o,
    );
    let out = expect_exit_success(run_test_on(&exec, run));
    assert_eq!(exec.now() - start, Duration::from_millis(50));
    assert_exit_fail(
      &out[0].1,
      &InfraError::Timeout {
        case: "slow".into(),
        limit_ms: 50,
      },
    );
    assert_eq!(case_exit_byte(&out[0].1), 1);
  }

  /// Fails with `io` `failures` times, then evaluates `[1 1]`.
  struct Flaky {
    calls: AtomicUsize,
    failures: usize,
    kind: &'static str,
  }

  impl SyncEval for Flaky {
    fn eval_json(&self, _expr: &str) -> EvalResult {
      if self.calls.fetch_add(1, Ordering::SeqCst) < self.failures {
        EvalResult::Err(AssayOutcome::EvalError {
          kind: self.kind.into(),
          message: "flaky".into(),
          span: None,
        })
      } else {
        EvalResult::Ok(json!([1, 1]))
      }
    }
  }

  fn run_flaky(
    failures: usize,
    kind: &'static str,
    retry: bool,
  ) -> (Exit<CaseVerdict, InfraError>, usize) {
    let eval = Arc::new(Flaky {
      calls: AtomicUsize::new(0),
      failures,
      kind,
    });
    let mut o = opts();
    o.batch_eval = false;
    o.retry_flaky_eval = retry;
    let caps = ClaimCaps {
      eval: Arc::clone(&eval) as Arc<dyn NixEval>,
      snapshots: temp_snapshot_store(),
      path_info: Arc::new(nixstore::MockPathInfoStore::default()),
      fs: Arc::new(crate::fake::MemFs::default()),
    };
    let exec = TestExecutor::new();
    let mut out = expect_exit_success(run_test_on(
      &exec,
      run_cases(vec![("x".to_string(), eq("a", "b"))], caps, o),
    ));
    (out.remove(0).1, eval.calls.load(Ordering::SeqCst))
  }

  #[test]
  fn retry_flaky_eval_retries_transient_io_failures_at_most_twice() {
    let (exit, calls) = run_flaky(2, "io", true);
    assert!(matches!(exit, Exit::Success(CaseVerdict::Pass)), "{exit:?}");
    assert_eq!(calls, 3);
    let (exit, calls) = run_flaky(5, "io", true);
    assert_exit_fail(&exit, &InfraError::Io("flaky".into()));
    assert_eq!(calls, 3);
  }

  #[test]
  fn retry_is_off_by_default_and_skips_permanent_failures() {
    let (exit, calls) = run_flaky(1, "io", false);
    assert_exit_fail(&exit, &InfraError::Io("flaky".into()));
    assert_eq!(calls, 1);
    let (exit, calls) = run_flaky(1, "nix_missing", true);
    assert!(matches!(exit, Exit::Failure(_)));
    assert_eq!(calls, 1);
  }

  #[test]
  fn summarize_and_exit_byte_fold_every_case() {
    let report = SuiteReport {
      outcomes: vec![
        ("p".into(), Exit::succeed(CaseVerdict::Pass)),
        ("e".into(), Exit::fail(InfraError::Io("x".into()))),
        (
          "f".into(),
          Exit::succeed(CaseVerdict::AssertFail {
            claim: "eq".into(),
            left: None,
            right: None,
            diff: String::new(),
          }),
        ),
      ],
    };
    assert_eq!(
      summarize(&report),
      RunSummary {
        total: 3,
        passed: 1,
        failed: 1,
        errored: 1
      }
    );
    assert_eq!(report.exit_byte(), 1);
    assert_eq!(SuiteReport { outcomes: vec![] }.exit_byte(), 0);
    let defect = SuiteReport {
      outcomes: vec![("d".into(), Exit::die("bug"))],
    };
    assert_eq!(defect.exit_byte(), 101);
  }

  #[test]
  fn missing_suite_fails_the_run() {
    let run = run_suite_with(caps(MockNixEval::default()), "/nonexistent".into(), opts());
    assert!(matches!(
      id_effect::expect_exit_failure(run_test(run)),
      InfraError::SuiteLoad(_)
    ));
  }
}
