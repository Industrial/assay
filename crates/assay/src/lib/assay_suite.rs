//! Load Assay-native suites: `{ name, cases = { case = { claim, ... }; }; }`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use id_effect::{Effect, fail, suspend};
use nixstore::edge::from_result;
use serde_json::Value;

use crate::claims::Claim;
use crate::eval::NixEval;
use crate::outcome::AssayOutcome;
use crate::schema::decode_suite_cases;
use crate::verdict::InfraError;

/// Parse an Assay suite JSON value into named claims.
pub fn parse_assay_suite(v: &Value) -> Result<Vec<(String, Claim)>, InfraError> {
  let load = |m: &str| InfraError::SuiteLoad(m.to_string());
  let obj = v
    .as_object()
    .ok_or_else(|| load("assay suite must be an object"))?;
  let cases = obj
    .get("cases")
    .and_then(|c| c.as_object())
    .ok_or_else(|| load("assay suite missing cases object"))?;
  decode_suite_cases(&Value::Object(cases.clone())).map_err(|e| load(&format!("{e:?}")))
}

/// Load a suite from `.assay.nix` (evaluated with `eval`) or from `.json`.
pub fn load_assay_suite(
  eval: Arc<dyn NixEval>,
  path: &Path,
) -> Effect<Vec<(String, Claim)>, InfraError> {
  let path = path.to_path_buf();
  match path.extension().and_then(|e| e.to_str()) {
    Some("json") => suspend(move || from_result(read_json_file(&path))),
    Some("nix") => eval
      .eval_file(&path)
      .map_err(nix_eval_failed)
      .flat_map(|v| from_result(parse_assay_suite(&v))),
    _ => fail(InfraError::SuiteLoad(format!(
      "unsupported assay suite: {}",
      path.display()
    ))),
  }
}

fn read_json_file(path: &PathBuf) -> Result<Vec<(String, Claim)>, InfraError> {
  let at =
    |e: &dyn std::fmt::Display| InfraError::SuiteLoad(format!("read {}: {e}", path.display()));
  let data = std::fs::read_to_string(path).map_err(|e| at(&e))?;
  let v: Value = serde_json::from_str(&data).map_err(|e| at(&e))?;
  parse_assay_suite(&v)
}

/// A failed suite evaluation as a load error.
pub(crate) fn nix_eval_failed(outcome: AssayOutcome) -> InfraError {
  InfraError::SuiteLoad(match outcome {
    AssayOutcome::EvalError { message, .. } => format!("nix eval failed: {message}"),
    other => format!("nix eval failed: {other:?}"),
  })
}

#[cfg(test)]
mod tests {
  use id_effect::{expect_exit_failure, expect_exit_success, run_test};
  use serde_json::json;

  use super::*;
  use crate::eval::EvalResult;
  use crate::fake::MockNixEval;

  fn mock() -> Arc<dyn NixEval> {
    Arc::new(MockNixEval::default())
  }

  #[test]
  fn parse_eq_and_throws() {
    let v = json!({
        "name": "smoke",
        "cases": {
            "add": { "claim": "eq", "expr": "1 + 1", "expected": "2" },
            "boom": { "claim": "throws", "expr": "builtins.throw \"x\"", "pattern": "x" }
        }
    });
    let cases = parse_assay_suite(&v).expect("parse");
    assert_eq!(cases.len(), 2);
    assert!(matches!(cases[0].1, Claim::Eq { .. }) || matches!(cases[1].1, Claim::Eq { .. }));
    assert!(cases.iter().any(|(_, c)| matches!(c, Claim::Throws { .. })));
  }

  #[test]
  fn load_assay_suite_json_roundtrip() {
    let dir = std::env::temp_dir().join(format!("assay-suite-json-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("suite.json");
    std::fs::write(
      &path,
      r#"{"name":"t","cases":{"one":{"claim":"eq","expr":"1","expected":"1"}}}"#,
    )
    .unwrap();
    let cases = expect_exit_success(run_test(load_assay_suite(mock(), &path)));
    assert_eq!(cases.len(), 1);
    let _ = std::fs::remove_dir_all(dir);
  }

  #[test]
  fn load_assay_suite_nix_goes_through_nix_eval() {
    let eval = MockNixEval::default();
    eval.set_file(
      "/s.assay.nix",
      EvalResult::Ok(json!({"cases": {"one": {"claim": "eq", "expr": "1", "expected": "1"}}})),
    );
    let cases = expect_exit_success(run_test(load_assay_suite(
      Arc::new(eval),
      Path::new("/s.assay.nix"),
    )));
    assert_eq!(cases.len(), 1);
    let err = expect_exit_failure(run_test(load_assay_suite(
      mock(),
      Path::new("/x.assay.nix"),
    )));
    assert!(matches!(err, InfraError::SuiteLoad(m) if m.starts_with("nix eval failed")));
  }

  #[test]
  fn load_assay_suite_unsupported_extension() {
    let exit = run_test(load_assay_suite(mock(), Path::new("/tmp/assay-suite.bad")));
    assert!(matches!(
      expect_exit_failure(exit),
      InfraError::SuiteLoad(_)
    ));
  }

  #[test]
  fn parse_assay_suite_invalid_case() {
    let v = json!({
        "name": "t",
        "cases": { "bad": { "claim": "nope" } }
    });
    assert!(parse_assay_suite(&v).is_err());
    assert!(parse_assay_suite(&json!([])).is_err());
    assert!(parse_assay_suite(&json!({"name": "t"})).is_err());
  }

  #[test]
  fn load_assay_suite_missing_file_errors() {
    let path = std::env::temp_dir().join(format!("assay-missing-{}.json", std::process::id()));
    let exit = run_test(load_assay_suite(mock(), &path));
    assert!(matches!(
      expect_exit_failure(exit),
      InfraError::SuiteLoad(_)
    ));
  }
}
