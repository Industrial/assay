//! Flat SHA256 hashing (file bytes are read through [`PathIo`](crate::PathIo)).

use sha2::{Digest, Sha256};

/// SHA256 of `bytes` (Nix `outputHashMode = "flat"`).
pub fn hash_flat_bytes(bytes: &[u8]) -> [u8; 32] {
  Sha256::digest(bytes).into()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn empty_and_hello() {
    assert_eq!(
      hex::encode(hash_flat_bytes(b"")),
      "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );
    assert_eq!(
      hex::encode(hash_flat_bytes(b"hello")),
      "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    );
  }
}
