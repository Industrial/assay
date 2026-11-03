//! Error types for nixdrv.

use thiserror::Error;

/// Failures of nixdrv commands (reading, parsing, arguments, output) — typed `Fail`, exit 1.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum InfraError {
  /// Reading the input (file or stdin) failed.
  #[error(transparent)]
  Source(#[from] nixq::InfraError),
  /// Input was neither an ATerm `Derive(...)` nor valid derivation JSON.
  #[error(
    "{input} is not a derivation: {message} (expected an ATerm `Derive(...)` or derivation JSON)"
  )]
  Json {
    /// Input name (file path or stdin label).
    input: String,
    /// Underlying JSON decode error message.
    message: String,
  },
  /// ATerm derivation parsing failed.
  #[error("parse error: {0}")]
  Parse(#[from] ParseError),
  /// A command-line argument had an invalid value.
  #[error("invalid {arg}: {message}")]
  InvalidArg {
    /// Offending argument name (e.g. `--digest-hex`).
    arg: String,
    /// Why the value was rejected.
    message: String,
  },
  /// Serialising the command result to JSON failed.
  #[error("cannot render output as JSON: {0}")]
  Render(String),
}

/// Derivation / ATerm parse failures.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ParseError {
  /// Input did not match the expected syntax at `offset`.
  #[error("unexpected token at offset {offset}: {message}")]
  Unexpected {
    /// Byte offset into the input.
    offset: usize,
    /// What was expected or found.
    message: String,
  },
  /// Input ended before the construct was complete.
  #[error("unexpected end of input at offset {offset}")]
  Eof {
    /// Byte offset where input ended.
    offset: usize,
  },
  /// A syntactically readable value is semantically invalid (bad store path, hash, algorithm, ...).
  #[error("invalid {what} at offset {offset}: {message}")]
  Invalid {
    /// Byte offset into the input.
    offset: usize,
    /// Kind of value that was invalid (e.g. `store path`, `hash`).
    what: String,
    /// Why the value was rejected.
    message: String,
  },
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn display_roundtrip() {
    let io = InfraError::from(nixq::InfraError::Io {
      path: "p".into(),
      message: "m".into(),
    });
    assert_eq!(io.to_string(), "io error at p: m");
    let json = InfraError::Json {
      input: "x.drv".into(),
      message: "m".into(),
    };
    assert!(json.to_string().starts_with("x.drv is not a derivation: m"));
    let arg = InfraError::InvalidArg {
      arg: "--digest-hex".into(),
      message: "odd length".into(),
    };
    assert_eq!(arg.to_string(), "invalid --digest-hex: odd length");
    assert!(
      InfraError::Render("r".into())
        .to_string()
        .contains("render")
    );
    let pe = ParseError::Unexpected {
      offset: 1,
      message: "bad".into(),
    };
    assert!(InfraError::Parse(pe).to_string().contains("parse error"));
  }
}
