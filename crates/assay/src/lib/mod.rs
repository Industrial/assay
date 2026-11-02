//! Assay — Nix unit testing framework (claim algebra + isolated eval runner).
//!
//! All `nix` evaluation goes through the [`NixEval`] capability, store path-info through
//! [`PathInfoService`] and file access through [`Fs`] ([`caps`]; live adapters in [`live`]). A
//! run is one effect over [`AssayEnv`] ([`run`]); its typed [`SuiteReport`] is rendered at the
//! edge ([`report`]). Suites decode through derived schemas ([`schema`]).

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
pub mod files;
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
  AssayEnv, EvalEnv, FsEnv, FsService, NixEvalService, PathInfoEnv, PathInfoService, SnapshotEnv,
  SnapshotService, SuiteEnv, default_snapshot_root, fs_layer, layer, layer_from, store_root_from,
};
#[cfg(any(test, feature = "test-support"))]
pub use caps::{FakeStore, require_store};
pub use claims::{
  Claim, ClaimCaps, build_module_eval_expr, claim_caps, eval_claim, interpret_claim,
  interpret_with, path_info_claim, snapshot_claim,
};
pub use compat::{CompatCase, CompatSuite, load_compat_suite};
pub use discover::{SuiteFile, discover, discover_suites};
pub use eval::{EvalResult, NixEval};
#[cfg(any(test, feature = "test-support"))]
pub use fake::{MemFs, MockNixEval, SyncEval, fake_layer, fake_layer_with_fs, temp_snapshot_store};
pub use files::{DirEntry, EntryKind, Fs};
pub use id_effect::Exit;
pub use laws::{run_builtin_laws, run_law_by_name};
pub use live::{LazyPathInfoStore, LiveFs, LiveNixEval};
pub use outcome::{AssayOutcome, run_case};
pub use report::{ReportFormat, render_report};
pub use run::{
  RunOptions, RunSummary, SuiteReport, Trace, default_jobs, load_suite, load_suite_cases,
  run_discovered, run_path, run_suite, summarize,
};
pub use schema::{
  decode_claim_json, decode_compat_cases, decode_suite, describe_issues, encode_claim_json,
};
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
