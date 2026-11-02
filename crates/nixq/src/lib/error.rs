//! Error types for nixq.

use thiserror::Error;

/// Failures of nixq commands (I/O, JSON parse, attrpath, store) — typed `Fail`, never a
/// predicate outcome.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum InfraError {
  #[error("io error at {path}: {message}")]
  Io { path: String, message: String },
  #[error("json parse error in {path}: {message} (input must be one JSON document)")]
  Json { path: String, message: String },
  #[error(transparent)]
  Path(#[from] PathError),
  #[error(transparent)]
  Store(#[from] nixstore::InfraError),
}

/// Attrpath parse / resolve failures.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum PathError {
  #[error("invalid attrpath: {0}")]
  Invalid(String),
  #[error("path not found: {0}")]
  NotFound(String),
}

/// Predicate CLI outcome (exit 0 / 1).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredicateResult {
  True,
  False,
}

impl PredicateResult {
  pub fn as_bool(self) -> bool {
    matches!(self, Self::True)
  }

  pub fn from_bool(v: bool) -> Self {
    if v { Self::True } else { Self::False }
  }

  /// Process exit code: `0` when true, `1` when false.
  pub fn exit_code(self) -> u8 {
    match self {
      Self::True => 0,
      Self::False => 1,
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn predicate_and_display() {
    assert!(PredicateResult::True.as_bool());
    assert!(!PredicateResult::False.as_bool());
    assert_eq!(PredicateResult::from_bool(true), PredicateResult::True);
    assert_eq!(PredicateResult::from_bool(false), PredicateResult::False);
    assert_eq!(PredicateResult::True.exit_code(), 0);
    assert_eq!(PredicateResult::False.exit_code(), 1);
    let io = InfraError::Io {
      path: "p".into(),
      message: "m".into(),
    };
    assert!(io.to_string().contains("io error"));
    let json = InfraError::Json {
      path: "in.json".into(),
      message: "x".into(),
    };
    assert!(json.to_string().contains("json parse error in in.json"));
    assert_eq!(
      InfraError::from(PathError::NotFound("a".into())).to_string(),
      "path not found: a"
    );
    assert!(
      InfraError::from(nixstore::InfraError::UnknownPath("/nix/store/x".into()))
        .to_string()
        .contains("unknown store path")
    );
    assert!(
      PathError::Invalid("i".into())
        .to_string()
        .contains("invalid")
    );
    assert!(
      PathError::NotFound("n".into())
        .to_string()
        .contains("not found")
    );
  }
}
