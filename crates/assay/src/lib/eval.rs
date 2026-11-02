//! The Nix evaluation capability and the classification of `nix eval` output.
//!
//! **Invariant:** [`NixEval`] is the only way assay reaches `nix` (suite loads, batches and
//! single claims alike); the process adapter is [`LiveNixEval`](crate::live::LiveNixEval).

use std::io;
use std::path::Path;

use id_effect::Effect;
use serde_json::Value;

use crate::outcome::AssayOutcome;

/// Result of evaluating a Nix expression to JSON (what the test fakes are scripted with).
#[derive(Debug, Clone, PartialEq)]
pub enum EvalResult {
  Ok(Value),
  Err(AssayOutcome),
}

impl EvalResult {
  /// The result as an effect that needs nothing.
  pub fn into_effect(self) -> Effect<Value, AssayOutcome> {
    match self {
      Self::Ok(v) => id_effect::succeed(v),
      Self::Err(e) => id_effect::fail(e),
    }
  }
}

/// Evaluates Nix to JSON in isolation. Methods return effects that need nothing (`R = HNil`).
///
/// A failed evaluation fails with its outcome: `EvalError { kind: "throw" }` (nix stderr in
/// `message`), [`AssayOutcome::Recursion`], `nix_missing`, `io` or `json`.
pub trait NixEval: Send + Sync {
  /// `nix eval --impure --expr {expr} --json`.
  fn eval_expr(&self, expr: &str) -> Effect<Value, AssayOutcome>;
  /// `nix eval --impure --file {path} --json` (suite loads).
  fn eval_file(&self, path: &Path) -> Effect<Value, AssayOutcome>;
  /// Evaluates Nix `source` too large for `--expr` (mega-batches exceed `ARG_MAX`).
  fn eval_source(&self, source: &str) -> Effect<Value, AssayOutcome>;
}

/// The outcome for a `nix` that could not be started.
pub(crate) fn nix_spawn_error(err: &io::Error) -> AssayOutcome {
  if err.kind() == io::ErrorKind::NotFound {
    return AssayOutcome::EvalError {
      kind: "nix_missing".to_string(),
      message: "nix executable not found in PATH".to_string(),
      span: None,
    };
  }
  AssayOutcome::EvalError {
    kind: "io".to_string(),
    message: err.to_string(),
    span: None,
  }
}

/// [`nix_spawn_error`] naming the file being evaluated.
pub(crate) fn nix_spawn_error_for_path(err: &io::Error, path: &Path) -> AssayOutcome {
  match nix_spawn_error(err) {
    AssayOutcome::EvalError { kind, .. } if kind == "io" => AssayOutcome::EvalError {
      kind,
      message: format!("run nix eval on {}: {err}", path.display()),
      span: None,
    },
    other => other,
  }
}

/// A finished `nix eval`: its JSON stdout on success, else its classified stderr.
pub(crate) fn eval_output(
  success: bool,
  stdout: &[u8],
  stderr: &[u8],
) -> Result<Value, AssayOutcome> {
  if success {
    return parse_stdout_json(stdout);
  }
  Err(classify_stderr(&String::from_utf8_lossy(stderr)))
}

pub(crate) fn parse_stdout_json(stdout: &[u8]) -> Result<Value, AssayOutcome> {
  serde_json::from_slice(stdout).map_err(|err| AssayOutcome::EvalError {
    kind: "json".to_string(),
    message: err.to_string(),
    span: None,
  })
}

/// Map `nix eval` stderr to the Assay outcome taxonomy.
pub(crate) fn classify_stderr(stderr: &str) -> AssayOutcome {
  if stderr.contains("infinite recursion") {
    return AssayOutcome::Recursion;
  }
  AssayOutcome::EvalError {
    kind: "throw".to_string(),
    message: stderr.to_string(),
    span: None,
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn classify_infinite_recursion() {
    let stderr = "error: infinite recursion encountered\\n";
    assert_eq!(classify_stderr(stderr), AssayOutcome::Recursion);
  }

  #[test]
  fn classify_throw() {
    let stderr = "error: boom\\n";
    assert_eq!(
      classify_stderr(stderr),
      AssayOutcome::EvalError {
        kind: "throw".to_string(),
        message: stderr.to_string(),
        span: None,
      }
    );
  }

  #[test]
  fn parse_stdout_json_roundtrip() {
    use serde_json::json;
    let v = parse_stdout_json(br#"{"a":1}"#).expect("parse");
    assert_eq!(v, json!({"a": 1}));
    assert!(parse_stdout_json(b"not json").is_err());
    assert!(parse_stdout_json(b"").is_err());
  }

  #[test]
  fn eval_output_splits_success_and_stderr() {
    assert_eq!(eval_output(true, b"2", b""), Ok(serde_json::json!(2)));
    assert!(matches!(
      eval_output(false, b"", b"error: boom"),
      Err(AssayOutcome::EvalError { kind, .. }) if kind == "throw"
    ));
  }

  #[test]
  fn nix_spawn_error_not_found() {
    let err = nix_spawn_error(&io::Error::new(io::ErrorKind::NotFound, "nix"));
    match err {
      AssayOutcome::EvalError { kind, .. } => assert_eq!(kind, "nix_missing"),
      other => panic!("expected nix_missing, got {other:?}"),
    }
    let err2 = nix_spawn_error(&io::Error::new(io::ErrorKind::PermissionDenied, "x"));
    match err2 {
      AssayOutcome::EvalError { kind, .. } => assert_eq!(kind, "io"),
      other => panic!("expected io, got {other:?}"),
    }
  }

  #[test]
  fn nix_spawn_error_for_path_variants() {
    let p = Path::new("/no/such/file.nix");
    let err = nix_spawn_error_for_path(&io::Error::new(io::ErrorKind::NotFound, "nix"), p);
    match err {
      AssayOutcome::EvalError { kind, .. } => assert_eq!(kind, "nix_missing"),
      other => panic!("expected nix_missing, got {other:?}"),
    }
    let err2 = nix_spawn_error_for_path(&io::Error::other("x"), p);
    match err2 {
      AssayOutcome::EvalError { kind, message, .. } => {
        assert_eq!(kind, "io");
        assert!(message.contains("/no/such/file.nix"));
      }
      other => panic!("expected io, got {other:?}"),
    }
  }
}
