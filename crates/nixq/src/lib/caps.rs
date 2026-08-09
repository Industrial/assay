//! Capability keys and providers for nixq DI 3.0.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use id_effect::{caps, mock_capability, provide};

use crate::error::InfraError;

/// Read JSON bytes from a file path or stdin (`-`).
pub trait JsonSource: Send + Sync {
  fn read(&self, input: &Path) -> Result<Vec<u8>, InfraError>;
}

/// Filesystem / stdin JSON source.
#[derive(Debug, Default, Clone, Copy)]
pub struct FsJsonSource;

impl JsonSource for FsJsonSource {
  fn read(&self, input: &Path) -> Result<Vec<u8>, InfraError> {
    if input.as_os_str() == "-" {
      use std::io::Read;
      let mut buf = Vec::new();
      std::io::stdin()
        .read_to_end(&mut buf)
        .map_err(|e| InfraError::Io {
          path: "-".into(),
          message: e.to_string(),
        })?;
      return Ok(buf);
    }
    std::fs::read(input).map_err(|e| InfraError::Io {
      path: input.display().to_string(),
      message: e.to_string(),
    })
  }
}

/// In-memory JSON source for tests (path → bytes).
#[derive(Default)]
pub struct MockJsonSource {
  pub files: std::sync::Mutex<std::collections::HashMap<PathBuf, Vec<u8>>>,
  pub stdin: std::sync::Mutex<Vec<u8>>,
}

impl MockJsonSource {
  pub fn set_file(&self, path: impl Into<PathBuf>, bytes: impl Into<Vec<u8>>) {
    self.files.lock().unwrap().insert(path.into(), bytes.into());
  }

  pub fn set_stdin(&self, bytes: impl Into<Vec<u8>>) {
    *self.stdin.lock().unwrap() = bytes.into();
  }
}

impl JsonSource for MockJsonSource {
  fn read(&self, input: &Path) -> Result<Vec<u8>, InfraError> {
    if input.as_os_str() == "-" {
      return Ok(self.stdin.lock().unwrap().clone());
    }
    self
      .files
      .lock()
      .unwrap()
      .get(input)
      .cloned()
      .ok_or_else(|| InfraError::Io {
        path: input.display().to_string(),
        message: "mock: file not found".into(),
      })
  }
}

pub type JsonSourceKey = Arc<dyn JsonSource>;

pub type NixqEnv = caps!(JsonSourceKey);

#[derive(::id_effect::ProviderSpecDerive)]
#[provides(JsonSourceKey)]
pub struct FsJsonSourceLive;

impl FsJsonSourceLive {
  #[allow(clippy::new_ret_no_self)] // ProviderSpecDerive factory returns capability key
  fn new() -> JsonSourceKey {
    Arc::new(FsJsonSource)
  }
}

mock_capability!(MockJsonSourceLive, JsonSourceKey, "json/mock", || Arc::new(
  MockJsonSource::default()
)
  as JsonSourceKey);

pub fn live_providers() -> [id_effect::ProviderBox; 1] {
  [provide!(FsJsonSourceLive)]
}

#[cfg(test)]
pub fn mock_providers() -> [id_effect::ProviderBox; 1] {
  [provide!(MockJsonSourceLive)]
}

#[cfg(test)]
mod tests {
  use super::*;
  use id_effect::{Cap, Effect, Exit, FromEnv, Needs, build_env, run_test};
  use std::time::Instant;

  #[test]
  fn build_env_materializes_live_caps() {
    let env = build_env(live_providers()).expect("env");
    assert!(env.has::<Cap<JsonSourceKey>>());
  }

  #[test]
  fn run_test_reads_caps() {
    let env = NixqEnv::from_env(build_env(mock_providers()).expect("env"));
    let effect: Effect<bool, (), NixqEnv> = Effect::new(|env| {
      let _src = Needs::<JsonSourceKey>::need(env);
      Ok(true)
    });
    assert_eq!(run_test(effect, env), Exit::Success(true));
  }

  #[test]
  fn mock_json_source_reads_configured_file() {
    let mock = MockJsonSource::default();
    mock.set_file("t.json", b"{\"a\":1}");
    let bytes = mock.read(Path::new("t.json")).expect("read");
    assert_eq!(bytes, b"{\"a\":1}");
  }

  #[test]
  fn mock_json_source_stdin_and_missing() {
    let mock = MockJsonSource::default();
    mock.set_stdin(b"{}");
    assert_eq!(mock.read(Path::new("-")).unwrap(), b"{}");
    let err = mock.read(Path::new("missing.json")).unwrap_err();
    assert!(matches!(err, InfraError::Io { .. }));
  }

  #[test]
  fn fs_json_source_reads_temp_file_and_missing() {
    let dir = std::env::temp_dir().join(format!(
      "nixq-fs-{}-{}",
      std::process::id(),
      Instant::now().elapsed().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("v.json");
    std::fs::write(&path, b"{\"ok\":true}").unwrap();
    let src = FsJsonSource;
    assert_eq!(src.read(&path).unwrap(), b"{\"ok\":true}");
    let err = src.read(&dir.join("nope.json")).unwrap_err();
    assert!(matches!(err, InfraError::Io { .. }));
    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn build_env_materializes_mock_caps() {
    let env = build_env(mock_providers()).expect("env");
    assert!(env.has::<Cap<JsonSourceKey>>());
  }
}
