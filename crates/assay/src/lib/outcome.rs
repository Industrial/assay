//! Assay outcome taxonomy — first-class Pass/Fail/EvalError and related exits.

use serde::{Deserialize, Serialize};

/// Result of running one assay case (legacy form; see [`crate::verdict`] for the `Exit` model).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum AssayOutcome {
  /// The claim held.
  Pass,
  /// The claim's assertion did not hold.
  Fail {
    /// Claim kind that failed (e.g. `eq`, `subset`).
    claim: String,
    /// Actual value, when there is one to show.
    left: Option<serde_json::Value>,
    /// Expected value, when there is one to show.
    right: Option<serde_json::Value>,
    /// Human-readable description of the mismatch.
    diff: String,
  },
  /// Evaluation or runner error; `kind` selects how it maps to a verdict (e.g. `throw`,
  /// `panic`, `io`, `suite_load`, `nix_missing`).
  EvalError {
    /// Error kind.
    kind: String,
    /// Error message.
    message: String,
    /// Source location, when known.
    span: Option<String>,
  },
  /// Evaluation hit infinite recursion.
  Recursion,
  /// The case exceeded its time limit.
  Timeout,
  /// A property check found a counterexample.
  Counterexample {
    /// Seed that reproduces the failure.
    seed: u64,
    /// Shrunk failing input.
    shrunk: serde_json::Value,
  },
  /// The observed value differs from (or lacks) its golden snapshot.
  SnapshotMismatch {
    /// Path of the golden file.
    path: String,
    /// Description of the difference.
    diff: String,
  },
  /// The case leaked a resource (treated as a defect).
  ResourceLeak,
}

/// Run a case closure, mapping panics to `EvalError { kind: "panic" }`.
pub fn run_case<F>(f: F) -> AssayOutcome
where
  F: FnOnce() -> AssayOutcome,
{
  match std::panic::catch_unwind(std::panic::AssertUnwindSafe(f)) {
    Ok(outcome) => outcome,
    Err(payload) => AssayOutcome::EvalError {
      kind: "panic".to_string(),
      message: panic_payload_message(&payload),
      span: None,
    },
  }
}

fn panic_payload_message(payload: &Box<dyn std::any::Any + Send>) -> String {
  if let Some(message) = payload.downcast_ref::<&str>() {
    return (*message).to_string();
  }
  if let Some(message) = payload.downcast_ref::<String>() {
    return message.clone();
  }
  "unknown panic".to_string()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn run_case_pass() {
    assert_eq!(run_case(|| AssayOutcome::Pass), AssayOutcome::Pass);
  }

  #[test]
  fn run_case_panic_becomes_eval_error() {
    let outcome = run_case(|| {
      panic!("boom");
    });
    assert_eq!(
      outcome,
      AssayOutcome::EvalError {
        kind: "panic".to_string(),
        message: "boom".to_string(),
        span: None,
      }
    );
  }

  #[test]
  fn run_case_string_panic_payload() {
    let outcome = run_case(|| panic!("{}", "string panic"));
    match outcome {
      AssayOutcome::EvalError { kind, message, .. } => {
        assert_eq!(kind, "panic");
        assert_eq!(message, "string panic");
      }
      other => panic!("expected panic eval error, got {other:?}"),
    }
  }

  #[test]
  fn run_case_unknown_panic_payload() {
    let outcome = run_case(|| {
      std::panic::panic_any(42i32);
    });
    assert_eq!(
      outcome,
      AssayOutcome::EvalError {
        kind: "panic".to_string(),
        message: "unknown panic".to_string(),
        span: None,
      }
    );
  }
}
