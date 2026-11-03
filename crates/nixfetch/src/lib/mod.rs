//! nixfetch — fixed-output fetchers + NAR / flat hash verify (id_effect).
//!
//! All I/O goes through the [`Http`], [`Git`] and [`PathIo`] capabilities ([`caps`]; live
//! adapters in [`live`]); hashing, NAR serialization and store-path math are pure. The network
//! fetches are retried and bounded by timeouts ([`resilience`]).

#![forbid(unsafe_code)]
// id_effect_lints: `effect!`'s `~` binds lower to `.await`, which `no_await_outside_from_async`
// flags (a false positive; real futures only run in `live`); pure hash / NAR / parse helpers
// return `Result` by design and are lifted into `Effect` where a capability is involved.
#![cfg_attr(
  dylint_lib = "id_effect_lints",
  allow(no_await_outside_from_async, no_result_return_in_pub_fn)
)]

pub mod caps;
pub mod commands;
pub mod error;
#[cfg(any(test, feature = "test-support"))]
pub mod fake;
pub mod hash_parse;
pub mod ingest;
pub mod live;
pub mod nar;
pub mod resilience;
pub mod verify;

#[cfg(test)]
mod coverage_tests;

pub use caps::{
  FetchGitEnv, FetchUrlEnv, FsNode, Git, GitService, Http, HttpService, NixfetchEnv, PathIo,
  PathIoEnv, PathIoService, layer, layer_from,
};
pub use commands::{
  DigestReport, FetchGitReport, FetchUrlReport, StorePathReport, VerifyReport, checkout_verified,
  digest_path, fetch_git, fetch_url, hash, store_path, store_path_report, verify,
};
pub use error::InfraError;
#[cfg(any(test, feature = "test-support"))]
pub use fake::{FakeGit, FakeHttp, MemFs, Reply};
pub use hash_parse::{
  ExpectedHash, format_digest, format_hex, format_nix32, parse_expected_hash, verify_digest,
};
pub use ingest::hash_flat_bytes;
pub use live::{FsPathIo, LiveGit, LiveHttp};
pub use nar::{hash_nar, nar_bytes, nar_serialize};
pub use resilience::{RetryPolicy, with_retry};
pub use verify::make_fixed_output_path;

pub use id_effect::Exit;

#[cfg(test)]
mod id_effect_dep_smoke {
  use id_effect::Exit;

  #[test]
  fn id_effect_exit_is_linked() {
    let exit: Exit<(), ()> = Exit::succeed(());
    assert!(matches!(exit, Exit::Success(())));
  }

  #[test]
  fn id_effect_cli_exit_code_linked() {
    use id_effect_cli::exit_code_for_exit;
    use std::process::ExitCode;
    let code = exit_code_for_exit(Exit::<(), ()>::succeed(()));
    assert_eq!(code, ExitCode::SUCCESS);
  }
}
