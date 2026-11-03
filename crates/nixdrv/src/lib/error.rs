//! Error types for nixdrv.

use thiserror::Error;

/// Failures of nixdrv commands (reading, parsing, arguments, output) — typed `Fail`, exit 1.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum InfraError {
  /// Reading the input (file or stdin) failed.
  #[error(transparent)]
  Source(#[from] nixq::InfraError),
  #[error(
    "{input} is not a derivation: {message} (expected an ATerm `Derive(...)` or derivation JSON)"
  )]
  Json { input: String, message: String },
  #[error("parse error: {0}")]
  Parse(#[from] ParseError),
  #[error("invalid {arg}: {message}")]
  InvalidArg { arg: String, message: String },
  #[error("cannot render output as JSON: {0}")]
  Render(String),
}

/// Derivation / ATerm parse failures.
#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum ParseError {
  #[error("unexpected token at offset {offset}: {message}")]
  Unexpected { offset: usize, message: String },
  #[error("unexpected end of input at offset {offset}")]
  Eof { offset: usize },
  #[error("invalid {what} at offset {offset}: {message}")]
  Invalid {
    offset: usize,
    what: String,
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
