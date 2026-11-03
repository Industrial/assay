//! Fixed-output store-path projection.

use nixdrv::{DEFAULT_STORE_DIR, FileIngestionMethod, StorePath, fixed_output_path};

use crate::error::InfraError;

/// The fixed-output store path for a sha256 `digest` (`store_dir` defaults to `/nix/store`).
pub fn make_fixed_output_path(
  name: &str,
  recursive: bool,
  digest: &[u8; 32],
  store_dir: Option<&str>,
) -> Result<StorePath, InfraError> {
  let method = if recursive {
    FileIngestionMethod::Recursive
  } else {
    FileIngestionMethod::Flat
  };
  Ok(fixed_output_path(
    name,
    method,
    "sha256",
    digest,
    store_dir.unwrap_or(DEFAULT_STORE_DIR),
  )?)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::ingest::hash_flat_bytes;

  #[test]
  fn store_path_from_hello() {
    let dig = hash_flat_bytes(b"hello");
    let sp = make_fixed_output_path("pkg", false, &dig, None).expect("store path");
    assert_eq!(sp.name, "pkg");
    assert_eq!(sp.hash.len(), 32);
    let rec = make_fixed_output_path("pkg", true, &dig, Some("/tmp/store")).expect("store path");
    assert_ne!(rec.hash, sp.hash);
  }
}
