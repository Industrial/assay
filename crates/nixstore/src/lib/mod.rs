//! nixstore — read-only Nix store path-info from db.sqlite.

// id_effect_lints: `effect!`'s `~` binds lower to `.await`, which `no_await_outside_from_async`
// flags (a false positive); DB / model helpers return `Result` by design and are lifted into
// `Effect` by the `PathInfoStore` capability.
#![cfg_attr(
  dylint_lib = "id_effect_lints",
  allow(no_await_outside_from_async, no_result_return_in_pub_fn)
)]

pub mod caps;
pub mod commands;
pub mod db;
pub mod edge;
pub mod error;
pub mod model;

#[cfg(any(test, feature = "test-support"))]
pub use caps::MockPathInfoStore;
pub use caps::{NixstoreEnv, PathInfoService, PathInfoStore, SqlitePathInfoStore, layer};
pub use commands::{PathInfoFlags, PathInfoReport, path_info, resolve_store_root};
pub use db::{DEFAULT_STORE_ROOT, db_path_for_store, open_db, open_store_db, query_path_info};
pub use edge::{CliError, from_result, print_line, run_main_coded};
pub use error::InfraError;
pub use model::{PathInfo, QueryOpts, db_hash_to_sri, human_bytes};

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
