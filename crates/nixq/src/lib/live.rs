//! Live adapter: the [`JsonSource`] over files and stdin. This is the only module that touches
//! the filesystem.

use std::io::Read;
use std::path::Path;

use id_effect::{Effect, suspend};
use nixstore::edge::from_result;

use crate::caps::JsonSource;
use crate::error::InfraError;

/// Filesystem / stdin JSON source (the live adapter).
#[derive(Debug, Default, Clone, Copy)]
pub struct FsJsonSource;

impl JsonSource for FsJsonSource {
  fn read(&self, input: &Path) -> Effect<Vec<u8>, InfraError> {
    let input = input.to_path_buf();
    suspend(move || from_result(read_input(&input)))
  }
}

/// Reads `input` (stdin for `-`) to the end.
fn read_input(input: &Path) -> Result<Vec<u8>, InfraError> {
  if input.as_os_str() == "-" {
    let mut buf = Vec::new();
    return std::io::stdin()
      .read_to_end(&mut buf)
      .map(|_| buf)
      .map_err(|e| InfraError::Io {
        path: "-".into(),
        message: e.to_string(),
      });
  }
  std::fs::read(input).map_err(|e| InfraError::Io {
    path: input.display().to_string(),
    message: e.to_string(),
  })
}
