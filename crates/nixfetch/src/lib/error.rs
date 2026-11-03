//! Error type for nixfetch.

use std::time::Duration;

use thiserror::Error;

/// Failures of nixfetch commands, split by how a caller responds to them.
///
/// The network fetches retry the [transient](Self::is_transient) ones (connection problems,
/// timed-out attempts, 5xx / 408 / 429 answers, failed clones); everything else fails at once.
/// At the edge [`HashMismatch`](Self::HashMismatch) exits 1 and every other failure exits 2
/// ([`exit_code`](Self::exit_code)).
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum InfraError {
  /// A filesystem operation failed; permanent.
  #[error("io error at {path}: {message}")]
  Io {
    /// Path the operation was on.
    path: String,
    /// Underlying I/O error message.
    message: String,
  },
  /// Connecting, TLS or reading the body failed (transient).
  #[error("http error for {url}: {message}")]
  Http {
    /// Requested URL.
    url: String,
    /// Underlying transport error message.
    message: String,
  },
  /// The server answered a non-2xx status (transient for 5xx, 408 and 429).
  #[error("http error for {url}: status {status}")]
  HttpStatus {
    /// Requested URL.
    url: String,
    /// HTTP status code of the answer.
    status: u16,
  },
  /// The request could not be built (e.g. an invalid URL); permanent.
  #[error("invalid http request for {url}: {message}")]
  HttpRequest {
    /// URL the request was for.
    url: String,
    /// Why the request could not be built.
    message: String,
  },
  /// `git clone` failed (transient: usually the network or the remote).
  #[error("git clone of {url} failed: {message}")]
  GitClone {
    /// Repository URL.
    url: String,
    /// Git's error output.
    message: String,
  },
  /// `git checkout <rev>` failed after a successful clone (usually an unknown rev); permanent.
  #[error("git checkout of {rev} failed: {message} (is the rev in the repository?)")]
  GitCheckout {
    /// Revision that was checked out.
    rev: String,
    /// Git's error output.
    message: String,
  },
  /// The `git` process could not be started; permanent.
  #[error("cannot run git: {0} (is git installed and on PATH?)")]
  GitSpawn(String),
  /// One attempt ran longer than its timeout (transient: the next attempt may be faster).
  #[error("{what}: attempt timed out after {after:?}")]
  Timeout {
    /// Operation that timed out (e.g. `GET <url>`).
    what: String,
    /// The per-attempt timeout that elapsed.
    after: Duration,
  },
  /// All attempts together ran longer than the overall timeout; not retried.
  #[error("{what}: gave up after {after:?} (overall timeout)")]
  Deadline {
    /// Operation that was given up on.
    what: String,
    /// The overall timeout that elapsed.
    after: Duration,
  },
  /// An expected hash or a derivation/store-path input could not be parsed; permanent.
  #[error("parse error: {0}")]
  Parse(String),
  /// The computed digest differs from the expected hash (exit code 1).
  #[error("hash mismatch: expected {expected}, got {actual}")]
  HashMismatch {
    /// Expected hash (SRI, or 32-char nix32 when a compressed hash was given).
    expected: String,
    /// Computed hash, in the same spelling as `expected`.
    actual: String,
  },
  /// Serialising the command result to JSON failed.
  #[error("cannot render output as JSON: {0}")]
  Render(String),
}

impl InfraError {
  /// `true` when retrying the same operation may succeed.
  pub fn is_transient(&self) -> bool {
    match self {
      Self::Http { .. } | Self::GitClone { .. } | Self::Timeout { .. } => true,
      Self::HttpStatus { status, .. } => *status >= 500 || matches!(status, 408 | 429),
      _ => false,
    }
  }

  /// The bin's exit code for this failure: 1 for a hash mismatch, 2 for everything else.
  pub fn exit_code(&self) -> u8 {
    match self {
      Self::HashMismatch { .. } => 1,
      _ => 2,
    }
  }
}

impl From<nixdrv::ParseError> for InfraError {
  fn from(e: nixdrv::ParseError) -> Self {
    Self::Parse(e.to_string())
  }
}

/// Failures of the shared edge helpers (`nixstore::edge::print_line` fails with `Io`).
impl From<nixstore::InfraError> for InfraError {
  fn from(e: nixstore::InfraError) -> Self {
    match e {
      nixstore::InfraError::Io { path, message } => Self::Io { path, message },
      other => Self::Render(other.to_string()),
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn http_status(status: u16) -> InfraError {
    InfraError::HttpStatus {
      url: "u".into(),
      status,
    }
  }

  #[test]
  fn display_is_actionable() {
    let io = InfraError::Io {
      path: "p".into(),
      message: "m".into(),
    };
    assert_eq!(io.to_string(), "io error at p: m");
    assert_eq!(http_status(404).to_string(), "http error for u: status 404");
    assert!(
      InfraError::GitSpawn("not found".into())
        .to_string()
        .contains("PATH")
    );
    let timeout = InfraError::Timeout {
      what: "GET u".into(),
      after: Duration::from_secs(30),
    };
    assert_eq!(timeout.to_string(), "GET u: attempt timed out after 30s");
    assert_eq!(
      InfraError::HashMismatch {
        expected: "a".into(),
        actual: "b".into()
      }
      .to_string(),
      "hash mismatch: expected a, got b"
    );
  }

  #[test]
  fn transient_split_routes_retries() {
    for transient in [
      InfraError::Http {
        url: "u".into(),
        message: "connection refused".into(),
      },
      InfraError::GitClone {
        url: "u".into(),
        message: "m".into(),
      },
      InfraError::Timeout {
        what: "w".into(),
        after: Duration::from_secs(1),
      },
      http_status(500),
      http_status(503),
      http_status(408),
      http_status(429),
    ] {
      assert!(transient.is_transient(), "{transient:?}");
    }
    for permanent in [
      http_status(404),
      http_status(403),
      InfraError::HashMismatch {
        expected: "a".into(),
        actual: "b".into(),
      },
      InfraError::Parse("p".into()),
      InfraError::Deadline {
        what: "w".into(),
        after: Duration::from_secs(1),
      },
      InfraError::GitCheckout {
        rev: "r".into(),
        message: "m".into(),
      },
    ] {
      assert!(!permanent.is_transient(), "{permanent:?}");
    }
  }

  #[test]
  fn only_hash_mismatch_exits_1() {
    let mismatch = InfraError::HashMismatch {
      expected: "a".into(),
      actual: "b".into(),
    };
    assert_eq!(mismatch.exit_code(), 1);
    assert_eq!(InfraError::Parse("p".into()).exit_code(), 2);
    assert_eq!(http_status(500).exit_code(), 2);
  }

  #[test]
  fn from_widens_dependency_errors() {
    let pe = nixdrv::ParseError::Eof { offset: 3 };
    assert!(matches!(InfraError::from(pe), InfraError::Parse(_)));
    let io = InfraError::from(nixstore::InfraError::Io {
      path: "<stdout>".into(),
      message: "broken pipe".into(),
    });
    assert_eq!(io.to_string(), "io error at <stdout>: broken pipe");
  }
}
