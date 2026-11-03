//! Error types for nixstore.

use thiserror::Error;

/// Infra failures — map to Fail / non-zero exit.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum InfraError {
  /// An I/O operation failed (e.g. writing stdout).
  #[error("io error at {path}: {message}")]
  Io {
    /// The file or stream involved (`<stdout>` for output).
    path: String,
    /// The underlying I/O error.
    message: String,
  },
  /// Opening or querying the store database failed.
  #[error("sqlite error: {0}")]
  Sqlite(String),
  /// No `var/nix/db/db.sqlite` under the store root; carries the path looked up.
  #[error("database not found at {0} (point the store root at a Nix store, e.g. --store /nix)")]
  DbMissing(String),
  /// The store path is not registered in `ValidPaths`.
  #[error("unknown store path: {0} (not registered in the store database)")]
  UnknownPath(String),
  /// A `ValidPaths.hash` value is not `algo:hex`.
  #[error("invalid hash in database: {0}")]
  InvalidHash(String),
  /// JSON (de)serialization failed.
  #[error("json error: {0}")]
  Json(String),
  /// The store database does not have the expected schema.
  #[error("schema error: {0}")]
  Schema(String),
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn display_variants() {
    assert!(
      InfraError::Sqlite("x".into())
        .to_string()
        .contains("sqlite")
    );
    assert!(
      InfraError::DbMissing("/tmp/db".into())
        .to_string()
        .contains("not found")
    );
    assert!(
      InfraError::UnknownPath("/nix/store/x".into())
        .to_string()
        .contains("unknown")
    );
    assert!(
      InfraError::InvalidHash("bad".into())
        .to_string()
        .contains("hash")
    );
    assert!(InfraError::Json("j".into()).to_string().contains("json"));
    assert!(
      InfraError::Schema("s".into())
        .to_string()
        .contains("schema")
    );
    let io = InfraError::Io {
      path: "p".into(),
      message: "m".into(),
    };
    assert!(io.to_string().contains("io error"));
  }
}
