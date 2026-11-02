//! The `PathInfoStore` capability: trait, newtype, environment alias and live layer.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};

use id_effect::{Effect, Layer, caps, hlist, suspend, sync};
use rusqlite::Connection;

use crate::db::{open_store_db, query_path_info};
use crate::edge::from_result;
use crate::error::InfraError;
use crate::model::{PathInfo, QueryOpts};

/// Read path-info for store paths. Methods return effects that need nothing (`R = HNil`).
pub trait PathInfoStore: Send + Sync {
  fn query(&self, path: &str, opts: QueryOpts) -> Effect<PathInfo, InfraError>;
}

/// The capability effects ask for (`~PathInfoService` / `need::<PathInfoService>()`).
#[derive(Clone)]
pub struct PathInfoService(pub Arc<dyn PathInfoStore>);

/// Everything nixstore commands need.
pub type NixstoreEnv = caps!(PathInfoService);

/// Sqlite-backed store over one read-only connection to `{store_root}/var/nix/db/db.sqlite`.
///
/// `rusqlite::Connection` is `Send` but `!Sync`, so it sits behind a `Mutex`; queries are short
/// point lookups and run serially on the fiber's thread. `None` once [`close`](Self::close)d.
#[derive(Debug)]
// The `Mutex` makes the `!Sync` connection shareable (see above); it coordinates no fibers.
#[cfg_attr(dylint_lib = "id_effect_lints", allow(no_mutex_for_fiber_coordination))]
pub struct SqlitePathInfoStore {
  conn: Arc<Mutex<Option<Connection>>>,
}

impl SqlitePathInfoStore {
  /// Open the store DB under `store_root` read-only (fails with `DbMissing` / `Sqlite`).
  pub fn open(store_root: &Path) -> Result<Self, InfraError> {
    Ok(Self {
      conn: Arc::new(Mutex::new(Some(open_store_db(store_root)?))),
    })
  }

  /// Close the connection; later queries fail with `InfraError::Sqlite`.
  pub fn close(&self) {
    let conn = self
      .conn
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .take();
    drop(conn);
  }
}

impl PathInfoStore for SqlitePathInfoStore {
  fn query(&self, path: &str, opts: QueryOpts) -> Effect<PathInfo, InfraError> {
    let conn = Arc::clone(&self.conn);
    let path = path.to_owned();
    suspend(move || {
      let guard = conn.lock().unwrap_or_else(PoisonError::into_inner);
      from_result(match guard.as_ref() {
        Some(c) => query_path_info(c, &path, opts),
        None => Err(InfraError::Sqlite("store connection is closed".into())),
      })
    })
  }
}

/// The live layer: opens the sqlite connection once per build and closes it when the layer's
/// scope closes (after the program, however it ends). `E` is the error of the graph it joins.
pub fn layer<E>(store_root: impl Into<PathBuf>) -> Layer<NixstoreEnv, E>
where
  E: From<InfraError> + Send + 'static,
{
  let store_root = store_root.into();
  Layer::scoped(move |scope| {
    let store_root = store_root.clone();
    suspend(move || from_result(SqlitePathInfoStore::open(&store_root).map(Arc::new)))
      .map_err(E::from)
      .flat_map(move |store: Arc<SqlitePathInfoStore>| {
        let held = Arc::clone(&store);
        scope
          .add_finalizer(sync(move || held.close()))
          .map(move |()| hlist![PathInfoService(store)])
      })
      // Open + register is one step, as in `Layer::acquire_release`.
      .uninterruptible()
  })
}

#[cfg(any(test, feature = "test-support"))]
mod fake {
  use std::collections::HashMap;
  use std::sync::{Arc, Mutex, PoisonError};

  use id_effect::{Effect, suspend};

  use super::PathInfoStore;
  use crate::edge::from_result;
  use crate::error::InfraError;
  use crate::model::{PathInfo, QueryOpts};

  /// In-memory path-info (tests; `test-support` feature).
  #[derive(Default)]
  // A test fake's in-memory state, not fiber coordination.
  #[cfg_attr(dylint_lib = "id_effect_lints", allow(no_mutex_for_fiber_coordination))]
  pub struct MockPathInfoStore {
    paths: Arc<Mutex<HashMap<String, PathInfo>>>,
  }

  impl MockPathInfoStore {
    pub fn set(&self, info: PathInfo) {
      self
        .paths
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(info.path.clone(), info);
    }
  }

  impl PathInfoStore for MockPathInfoStore {
    fn query(&self, path: &str, opts: QueryOpts) -> Effect<PathInfo, InfraError> {
      let paths = Arc::clone(&self.paths);
      let path = path.to_owned();
      suspend(move || {
        let found = paths
          .lock()
          .unwrap_or_else(PoisonError::into_inner)
          .get(&path)
          .cloned();
        from_result(
          found
            .map(|mut info| {
              if !opts.include_referrers {
                info.referrers = None;
              }
              if !opts.include_closure_size {
                info.closure_size = None;
              }
              info
            })
            .ok_or(InfraError::UnknownPath(path)),
        )
      })
    }
  }
}

#[cfg(any(test, feature = "test-support"))]
pub use fake::MockPathInfoStore;

#[cfg(test)]
mod tests {
  use super::*;
  use id_effect::{assert_exit_fail, expect_exit_failure, expect_exit_success, need, run_test};

  const A: &str = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-a";

  fn fixture_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/minimal")
  }

  fn query_a() -> Effect<PathInfo, InfraError, NixstoreEnv> {
    need::<PathInfoService>().flat_map(|s| s.0.query(A, QueryOpts::default()).narrow())
  }

  #[test]
  fn mock_query_drops_unrequested_fields() {
    let mock = MockPathInfoStore::default();
    mock.set(PathInfo {
      path: "/nix/store/x".into(),
      nar_hash: "sha256-x".into(),
      nar_size: 1,
      deriver: None,
      registration_time: 0,
      ultimate: false,
      signatures: vec![],
      ca: None,
      references: vec![],
      referrers: Some(vec!["/nix/store/y".into()]),
      closure_size: Some(1),
    });
    let info = expect_exit_success(run_test(mock.query("/nix/store/x", QueryOpts::default())));
    assert!(info.referrers.is_none());
    assert!(info.closure_size.is_none());
  }

  #[test]
  fn mock_unknown_path_fails() {
    let mock = MockPathInfoStore::default();
    assert_exit_fail(
      &run_test(mock.query("/nix/store/x", QueryOpts::default())),
      &InfraError::UnknownPath("/nix/store/x".into()),
    );
  }

  #[test]
  fn layer_provides_sqlite_store() {
    let info = expect_exit_success(run_test(
      query_a().provide_layer(layer::<InfraError>(fixture_root())),
    ));
    assert_eq!(info.nar_size, 100);
  }

  #[test]
  fn layer_fails_when_db_missing() {
    let exit = run_test(query_a().provide_layer(layer::<InfraError>("/nonexistent")));
    assert_exit_fail(
      &exit,
      &InfraError::DbMissing("/nonexistent/var/nix/db/db.sqlite".into()),
    );
  }

  #[test]
  fn closed_store_fails_queries() {
    let store = SqlitePathInfoStore::open(&fixture_root()).expect("open fixture");
    store.close();
    let exit = run_test(store.query(A, QueryOpts::default()));
    assert!(matches!(expect_exit_failure(exit), InfraError::Sqlite(_)));
  }
}
