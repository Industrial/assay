//! The derivation source capability.
//!
//! A derivation is read as raw bytes from a file or from stdin (`-`), exactly like a nixq JSON
//! input, so nixdrv re-uses nixq's `JsonSource` capability under neutral names instead of
//! duplicating it. Its failures are `nixq::InfraError` (widened into
//! [`InfraError`](crate::error::InfraError) by `From`).

use id_effect::Layer;

/// Filesystem / stdin source (the live adapter, nixq's [`FsJsonSource`](nixq::FsJsonSource)).
pub use nixq::caps::FsJsonSource as FsDrvSource;
/// Read the raw bytes of a derivation: a file, or stdin when the path is `-` (nixq's
/// [`JsonSource`](nixq::JsonSource)).
pub use nixq::caps::JsonSource as DrvSource;
/// The capability effects ask for (`~DrvSourceService`); nixq's
/// [`JsonSourceService`](nixq::JsonSourceService).
pub use nixq::caps::JsonSourceService as DrvSourceService;

/// Everything nixdrv's file commands need (the same type as `nixq::NixqEnv`).
pub type NixdrvEnv = nixq::NixqEnv;

/// The live layer (files and stdin). `E` is the error of the graph it joins; building it never
/// fails.
pub fn layer<E>() -> Layer<NixdrvEnv, E>
where
  E: Send + 'static,
{
  nixq::layer()
}

#[cfg(test)]
mod tests {
  use super::*;
  use id_effect::{Effect, expect_exit_failure, expect_exit_success, need, run_test};
  use std::path::PathBuf;
  use std::sync::Arc;

  fn read_via_env(input: PathBuf) -> Effect<Vec<u8>, nixq::InfraError, NixdrvEnv> {
    need::<DrvSourceService>().flat_map(move |s| s.0.read(&input).narrow())
  }

  #[test]
  fn provided_fake_is_what_effects_need() {
    let mock = nixq::MockJsonSource::default();
    mock.set_file("t.drv", b"Derive");
    let bytes = expect_exit_success(run_test(
      read_via_env("t.drv".into()).provide(DrvSourceService(Arc::new(mock))),
    ));
    assert_eq!(bytes, b"Derive");
  }

  #[test]
  fn live_layer_reads_temp_file_and_fails_on_missing() {
    let dir = std::env::temp_dir().join(format!("nixdrv-fs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("x.drv");
    std::fs::write(&path, b"Derive").expect("write temp file");
    let read = |p: PathBuf| read_via_env(p).provide_layer(layer::<nixq::InfraError>());
    assert_eq!(expect_exit_success(run_test(read(path))), b"Derive");
    let err = expect_exit_failure(run_test(read(dir.join("nope.drv"))));
    assert!(matches!(err, nixq::InfraError::Io { .. }));
    let _ = std::fs::remove_dir_all(&dir);
  }
}
