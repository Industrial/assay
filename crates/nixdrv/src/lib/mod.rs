//! nixdrv — derivation ATerm/JSON parse, project, CA store-path helpers.

// id_effect_lints: `effect!`'s `~` binds lower to `.await`, which `no_await_outside_from_async`
// flags (a false positive); ATerm / JSON parsing and store-path math stay pure and return
// `Result` by design, lifted into `Effect` where a capability is involved.
#![cfg_attr(
  dylint_lib = "id_effect_lints",
  allow(no_await_outside_from_async, no_result_return_in_pub_fn)
)]

pub mod ca;
pub mod caps;
pub mod commands;
pub mod error;
pub mod hash;
pub mod model;
pub mod parse_aterm;
pub mod parse_json;
pub mod project;
pub mod store_path;

pub use ca::{FileIngestionMethod, fixed_output_path, make_store_path, text_path};
pub use caps::{DrvSource, DrvSourceService, FsDrvSource, NixdrvEnv, layer};
pub use commands::{
  DerivationReport, FixedPathReport, OutputReport, Projection, StorePathReport, fixed_path_report,
  load_derivation, parse_derivation, store_path_report,
};
pub use error::{InfraError, ParseError};
pub use hash::{NIX_BASE32_ALPHABET, compress_hash, nix_base32_decode, nix_base32_encode};
pub use model::{Derivation, DerivationOutput};
pub use parse_aterm::parse_drv_aterm;
pub use project::{project, project_map};
pub use store_path::{DEFAULT_STORE_DIR, StorePath, parse_store_path};

pub use id_effect::Exit;

#[cfg(test)]
mod coverage_tests;

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
