//! nixq — JSON/attrpath query (pure value algebra + Effect I/O).

pub mod caps;
pub mod commands;
pub mod diff;
pub mod error;
pub mod force_path;
pub mod normalize;
pub mod path;
pub mod predicate;

#[cfg(any(test, feature = "test-support"))]
pub use caps::MockJsonSource;
pub use caps::{FsJsonSource, JsonSource, JsonSourceService, NixqEnv, layer};
pub use commands::{DiffReport, JsonReport, diff_report, load_json, lookup, parse_json};
pub use diff::{structural_diff, values_equal};
pub use error::{InfraError, PathError, PredicateResult};
pub use force_path::force_paths;
pub use normalize::normalize_value;
pub use path::{AttrPath, PathSegment, get_at_path, parse_attrpath};
pub use predicate::{fold_object_keys, value_contains_subset, value_has_attrs};

pub use id_effect::Exit;

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
