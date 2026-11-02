//! assay's capabilities: Nix evaluation, the snapshot store and store path-info, their
//! newtypes, environment aliases and the live layer.
//!
//! Each claim kind needs only part of [`AssayEnv`]: `eq` / `throws` / `subset` / `hasAttrs` /
//! `drv` / `module` / `forces` need [`EvalEnv`], `snapshot` needs [`SnapshotEnv`], `pathInfo`
//! needs [`PathInfoEnv`]; value claims, laws and props need nothing. Live adapters are in
//! [`live`](crate::live); the fakes behind `cfg(test)` / `test-support` in `fake`.

use std::ffi::OsString;
use std::path::PathBuf;
use std::sync::Arc;

use id_effect::{Layer, caps, hlist};
pub use nixstore::PathInfoService;

use crate::eval::NixEval;
use crate::live::{LazyPathInfoStore, LiveNixEval};
use crate::outcome::AssayOutcome;
use crate::snapshot::SnapshotStore;

/// The capability effects ask for (`~NixEvalService`).
#[derive(Clone)]
pub struct NixEvalService(pub Arc<dyn NixEval>);

/// The golden-file snapshot store, a plain value (its root and the `--update-snapshots` flag).
#[derive(Clone)]
pub struct SnapshotService(pub SnapshotStore);

/// What claims that evaluate Nix need.
pub type EvalEnv = caps!(NixEvalService);
/// What `snapshot` claims need.
pub type SnapshotEnv = caps!(NixEvalService, SnapshotService);
/// What `pathInfo` claims need.
pub type PathInfoEnv = caps!(PathInfoService);
/// Every assay capability: what a run needs (and [`layer`] provides).
pub type AssayEnv = caps!(NixEvalService, SnapshotService, PathInfoService);

/// Where goldens live: `testdata/goldens/` of this crate.
pub fn default_snapshot_root() -> PathBuf {
  PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("testdata/goldens")
}

/// The store root for `pathInfo` claims: `$ASSAY_NIX_STORE` (read once at the edge), else
/// `/nix`.
pub fn store_root_from(assay_nix_store: Option<OsString>) -> PathBuf {
  assay_nix_store.map_or_else(|| PathBuf::from("/nix"), PathBuf::from)
}

/// The live layer: `nix` on PATH, `snapshots`, and the store database under `store_root`
/// (opened on the first `pathInfo` query, see [`LazyPathInfoStore`]). `E` is the error of the
/// graph it joins; building it never fails.
pub fn layer<E>(store_root: PathBuf, snapshots: SnapshotStore) -> Layer<AssayEnv, E>
where
  E: Send + 'static,
{
  layer_from(
    NixEvalService(Arc::new(LiveNixEval)),
    SnapshotService(snapshots),
    PathInfoService(Arc::new(LazyPathInfoStore::new(store_root))),
  )
}

/// A layer over the given capability values (fakes in tests).
pub fn layer_from<E>(
  eval: NixEvalService,
  snapshots: SnapshotService,
  path_info: PathInfoService,
) -> Layer<AssayEnv, E>
where
  E: Send + 'static,
{
  Layer::succeed(hlist![eval, snapshots, path_info])
}

/// Marker for an in-memory / fake nix store used in sandboxed module tests.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq)]
pub struct FakeStore;

impl FakeStore {
  pub fn allow_ifd(&self) -> bool {
    false
  }
}

/// Require a [`FakeStore`] for sandboxed / IFD module evaluation.
pub fn require_store(store: Option<&FakeStore>) -> Result<&FakeStore, AssayOutcome> {
  store.ok_or(AssayOutcome::EvalError {
    kind: "sandbox".into(),
    message: "IFD denied: provide FakeStore capability".into(),
    span: None,
  })
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn snapshot_root_is_goldens() {
    assert!(default_snapshot_root().ends_with("testdata/goldens"));
  }

  #[test]
  fn store_root_defaults_to_nix() {
    assert_eq!(store_root_from(None), PathBuf::from("/nix"));
    assert_eq!(
      store_root_from(Some("/tmp/store".into())),
      PathBuf::from("/tmp/store")
    );
  }

  #[test]
  fn fake_store_denies_ifd_by_default() {
    assert!(!FakeStore.allow_ifd());
  }

  #[test]
  fn require_store_err_when_missing() {
    assert_eq!(
      require_store(None),
      Err(AssayOutcome::EvalError {
        kind: "sandbox".into(),
        message: "IFD denied: provide FakeStore capability".into(),
        span: None,
      })
    );
  }

  #[test]
  fn require_store_ok_when_present() {
    let store = FakeStore;
    assert!(require_store(Some(&store)).is_ok());
  }
}
