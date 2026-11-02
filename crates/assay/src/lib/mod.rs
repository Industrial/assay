//! Assay — Nix unit testing framework (claim algebra + isolated eval runner).
//!
//! All `nix` evaluation goes through the [`NixEval`] capability and store path-info through
//! [`PathInfoService`] ([`caps`]; live adapters in [`live`]). A run is one effect over
//! [`AssayEnv`] ([`run`]); its typed [`SuiteReport`] is rendered at the edge ([`report`]).

#![forbid(unsafe_code)]

pub mod assay_suite;
pub mod batch;
pub mod caps;
pub mod claims;
pub mod compat;
pub mod discover;
pub mod eval;
#[cfg(any(test, feature = "test-support"))]
pub mod fake;
pub mod force;
pub mod laws;
pub mod live;
pub mod outcome;
pub mod prop;
pub mod report;
pub mod run;
pub mod schema;
pub mod simple_eval;
pub mod snapshot;
pub mod verdict;

pub use caps::{
  AssayEnv, EvalEnv, FakeStore, NixEvalService, PathInfoEnv, PathInfoService, SnapshotEnv,
  SnapshotService, default_snapshot_root, layer, layer_from, require_store, store_root_from,
};
pub use claims::{
  Claim, ClaimCaps, build_module_eval_expr, claim_caps, eval_claim, interpret_claim,
  interpret_with, path_info_claim, snapshot_claim,
};
pub use compat::{CompatCase, CompatSuite, load_compat_suite};
pub use discover::{SuiteFile, discover_suites};
pub use eval::{EvalResult, NixEval};
#[cfg(any(test, feature = "test-support"))]
pub use fake::{MockNixEval, SyncEval, fake_layer, temp_snapshot_store};
pub use id_effect::Exit;
pub use laws::{run_builtin_laws, run_law_by_name};
pub use live::{LazyPathInfoStore, LiveNixEval};
pub use outcome::{AssayOutcome, run_case};
pub use report::{ReportFormat, render_report};
pub use run::{
  RunOptions, RunSummary, SuiteReport, Trace, default_jobs, run_discovered, run_suite, summarize,
};
pub use schema::{decode_claim_json, decode_suite_cases, encode_claim_json};
pub use snapshot::SnapshotStore;
pub use verdict::{CaseVerdict, InfraError, case_exit_byte, exit_to_outcome, outcome_to_exit};

/// Wave-0 compile gate: id_effect is linked.
#[cfg(test)]
mod coverage_tests;

#[cfg(test)]
mod branch_coverage_tests;

#[cfg(test)]
mod id_effect_dep_smoke {
  use id_effect::Exit;

  #[test]
  fn id_effect_exit_is_linked() {
    let exit: Exit<(), ()> = Exit::succeed(());
    assert!(matches!(exit, Exit::Success(())));
  }

  #[cfg(feature = "cli-exit")]
  #[test]
  fn id_effect_cli_exit_code_linked() {
    use id_effect_cli::exit_code_for_exit;
    use std::process::ExitCode;
    let code = exit_code_for_exit(Exit::<(), ()>::succeed(()));
    assert_eq!(code, ExitCode::SUCCESS);
  }
}
