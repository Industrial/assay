//! Capability keys and providers for nixdrv DI 3.0.

use id_effect::{caps, mock_capability, provide};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::error::InfraError;

/// Read derivation bytes from a file path or stdin (`-`).
pub trait DrvSource: Send + Sync {
  fn read(&self, input: &Path) -> Result<Vec<u8>, InfraError>;
}

/// Filesystem / stdin derivation source.
#[derive(Debug, Default, Clone, Copy)]
pub struct FsDrvSource;

impl DrvSource for FsDrvSource {
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

/// In-memory derivation source for tests (path → bytes).
#[derive(Default)]
pub struct MockDrvSource {
  pub files: std::sync::Mutex<std::collections::HashMap<PathBuf, Vec<u8>>>,
  pub stdin: std::sync::Mutex<Vec<u8>>,
}

impl MockDrvSource {
  pub fn set_file(&self, path: impl Into<PathBuf>, bytes: impl Into<Vec<u8>>) {
    self.files.lock().unwrap().insert(path.into(), bytes.into());
  }

  pub fn set_stdin(&self, bytes: impl Into<Vec<u8>>) {
    *self.stdin.lock().unwrap() = bytes.into();
  }
}

impl DrvSource for MockDrvSource {
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

pub type DrvSourceKey = Arc<dyn DrvSource>;

pub type NixdrvEnv = caps!(DrvSourceKey);

#[derive(::id_effect::ProviderSpecDerive)]
#[provides(DrvSourceKey)]
pub struct FsDrvSourceLive;

impl FsDrvSourceLive {
  #[allow(clippy::new_ret_no_self)] // ProviderSpecDerive factory returns capability key
  fn new() -> DrvSourceKey {
    Arc::new(FsDrvSource)
  }
}

mock_capability!(
  MockDrvSourceLive,
  DrvSourceKey,
  "drv/mock",
  || Arc::new(MockDrvSource::default()) as DrvSourceKey
);

pub fn live_providers() -> [id_effect::ProviderBox; 1] {
  [provide!(FsDrvSourceLive)]
}

#[cfg(test)]
pub fn mock_providers() -> [id_effect::ProviderBox; 1] {
  [provide!(MockDrvSourceLive)]
}

#[cfg(test)]
mod tests {
  use super::*;
  use id_effect::{Cap, Effect, Exit, FromEnv, Needs, build_env, run_test};
  use std::time::Instant;

  #[test]
  fn build_env_materializes_live_caps() {
    let env = build_env(live_providers()).expect("env");
    assert!(env.has::<Cap<DrvSourceKey>>());
  }

  #[test]
  fn run_test_reads_caps() {
    let env = NixdrvEnv::from_env(build_env(mock_providers()).expect("env"));
    let effect: Effect<bool, (), NixdrvEnv> = Effect::new(|env| {
      let _src = Needs::<DrvSourceKey>::need(env);
      Ok(true)
    });
    assert_eq!(run_test(effect, env), Exit::Success(true));
  }

  #[test]
  fn mock_drv_source_reads_configured_file() {
    let mock = MockDrvSource::default();
    mock.set_file("t.drv", b"Derive");
    let bytes = mock.read(Path::new("t.drv")).expect("read");
    assert_eq!(bytes, b"Derive");
  }

  #[test]
  fn mock_drv_source_stdin_and_missing() {
    let mock = MockDrvSource::default();
    mock.set_stdin(b"bytes");
    assert_eq!(mock.read(Path::new("-")).unwrap(), b"bytes");
    let err = mock.read(Path::new("missing.drv")).unwrap_err();
    assert!(matches!(err, InfraError::Io { .. }));
  }

  #[test]
  fn fs_drv_source_reads_temp_file() {
    let dir = std::env::temp_dir().join(format!(
      "nixdrv-fs-{}-{}",
      std::process::id(),
      Instant::now().elapsed().as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("x.drv");
    std::fs::write(&path, b"Derive").unwrap();
    let src = FsDrvSource;
    assert_eq!(src.read(&path).unwrap(), b"Derive");
    let _ = std::fs::remove_dir_all(&dir);
  }

  #[test]
  fn build_env_materializes_mock_caps() {
    let env = build_env(mock_providers()).expect("env");
    assert!(env.has::<Cap<DrvSourceKey>>());
  }
}
