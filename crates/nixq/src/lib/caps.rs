//! The `JsonSource` capability: trait, newtype, environment alias and live layer.

use std::path::Path;
use std::sync::Arc;

use id_effect::{Effect, Layer, caps, hlist};

use crate::error::InfraError;
pub use crate::live::FsJsonSource;

/// Read the raw bytes of a JSON input: a file, or stdin when the path is `-`. Methods return
/// effects that need nothing (`R = HNil`).
pub trait JsonSource: Send + Sync {
  fn read(&self, input: &Path) -> Effect<Vec<u8>, InfraError>;
}

/// The capability effects ask for (`~JsonSourceService` / `need::<JsonSourceService>()`).
#[derive(Clone)]
pub struct JsonSourceService(pub Arc<dyn JsonSource>);

/// Everything nixq's JSON commands need.
pub type NixqEnv = caps!(JsonSourceService);

/// The live layer (files and stdin). `E` is the error of the graph it joins; building it never
/// fails.
pub fn layer<E>() -> Layer<NixqEnv, E>
where
  E: Send + 'static,
{
  Layer::succeed(hlist![JsonSourceService(Arc::new(FsJsonSource))])
}

#[cfg(any(test, feature = "test-support"))]
mod fake {
  use std::collections::HashMap;
  use std::path::{Path, PathBuf};
  use std::sync::{Arc, Mutex, PoisonError};

  use id_effect::{Effect, suspend};
  use nixstore::edge::from_result;

  use super::JsonSource;
  use crate::error::InfraError;

  /// In-memory JSON source: path → bytes, plus stdin for `-` (tests; `test-support` feature).
  #[derive(Default)]
  // A test fake's in-memory state, not fiber coordination.
  #[cfg_attr(dylint_lib = "id_effect_lints", allow(no_mutex_for_fiber_coordination))]
  pub struct MockJsonSource {
    files: Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>,
    stdin: Arc<Mutex<Vec<u8>>>,
  }

  impl MockJsonSource {
    pub fn set_file(&self, path: impl Into<PathBuf>, bytes: impl Into<Vec<u8>>) {
      self
        .files
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .insert(path.into(), bytes.into());
    }

    pub fn set_stdin(&self, bytes: impl Into<Vec<u8>>) {
      *self.stdin.lock().unwrap_or_else(PoisonError::into_inner) = bytes.into();
    }
  }

  impl JsonSource for MockJsonSource {
    fn read(&self, input: &Path) -> Effect<Vec<u8>, InfraError> {
      let files = Arc::clone(&self.files);
      let stdin = Arc::clone(&self.stdin);
      let input = input.to_path_buf();
      suspend(move || {
        let found = if input.as_os_str() == "-" {
          Some(stdin.lock().unwrap_or_else(PoisonError::into_inner).clone())
        } else {
          files
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(&input)
            .cloned()
        };
        from_result(found.ok_or_else(|| InfraError::Io {
          path: input.display().to_string(),
          message: "mock: file not found".into(),
        }))
      })
    }
  }
}

#[cfg(any(test, feature = "test-support"))]
pub use fake::MockJsonSource;

#[cfg(test)]
mod tests {
  use super::*;
  use id_effect::{assert_exit_fail, expect_exit_failure, expect_exit_success, need, run_test};
  use std::path::PathBuf;

  fn read_via_env(input: PathBuf) -> Effect<Vec<u8>, InfraError, NixqEnv> {
    need::<JsonSourceService>().flat_map(move |s| s.0.read(&input).narrow())
  }

  #[test]
  fn mock_json_source_reads_configured_file() {
    let mock = MockJsonSource::default();
    mock.set_file("t.json", b"{\"a\":1}");
    let bytes = expect_exit_success(run_test(mock.read(Path::new("t.json"))));
    assert_eq!(bytes, b"{\"a\":1}");
  }

  #[test]
  fn mock_json_source_stdin_and_missing() {
    let mock = MockJsonSource::default();
    mock.set_stdin(b"{}");
    assert_eq!(
      expect_exit_success(run_test(mock.read(Path::new("-")))),
      b"{}"
    );
    assert_exit_fail(
      &run_test(mock.read(Path::new("missing.json"))),
      &InfraError::Io {
        path: "missing.json".into(),
        message: "mock: file not found".into(),
      },
    );
  }

  #[test]
  fn provided_fake_is_what_effects_need() {
    let mock = MockJsonSource::default();
    mock.set_file("t.json", b"[]");
    let bytes = expect_exit_success(run_test(
      read_via_env("t.json".into()).provide(JsonSourceService(Arc::new(mock))),
    ));
    assert_eq!(bytes, b"[]");
  }

  #[test]
  fn live_layer_reads_temp_file_and_fails_on_missing() {
    let dir = std::env::temp_dir().join(format!("nixq-fs-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("temp dir");
    let path = dir.join("v.json");
    std::fs::write(&path, b"{\"ok\":true}").expect("write temp file");
    let read = |p: PathBuf| read_via_env(p).provide_layer(layer::<InfraError>());
    assert_eq!(expect_exit_success(run_test(read(path))), b"{\"ok\":true}");
    let err = expect_exit_failure(run_test(read(dir.join("nope.json"))));
    assert!(matches!(err, InfraError::Io { .. }));
    let _ = std::fs::remove_dir_all(&dir);
  }
}
