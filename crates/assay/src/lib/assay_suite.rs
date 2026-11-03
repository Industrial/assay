//! Load Assay-native suites: `{ name, cases = { case = { claim, ... }; }; }`.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use id_effect::schema::ParseErrors;
use id_effect::{Effect, fail};
use nixstore::edge::from_result;
use serde_json::Value;

use crate::claims::Claim;
use crate::eval::NixEval;
use crate::files::{Fs, reword_io};
use crate::outcome::AssayOutcome;
use crate::schema::{decode_suite, describe_issues};
use crate::verdict::InfraError;

/// Parse an Assay suite JSON value into named claims (by case name); every decode issue is
/// reported.
pub fn parse_assay_suite(v: &Value) -> Result<Vec<(String, Claim)>, ParseErrors> {
  decode_suite(v)
}

/// A suite at `path` that did not decode, as a load error listing every issue.
pub fn invalid_suite(path: &Path, errors: &ParseErrors) -> InfraError {
  InfraError::SuiteLoad(format!("{}: {}", path.display(), describe_issues(errors)))
}

/// Load a suite from `.assay.nix` (evaluated with `eval`) or from `.json` (read with `fs`).
pub fn load_assay_suite(
  eval: Arc<dyn NixEval>,
  fs: Arc<dyn Fs>,
  path: &Path,
) -> Effect<Vec<(String, Claim)>, InfraError> {
  let path = path.to_path_buf();
  match path.extension().and_then(|e| e.to_str()) {
    Some("json") => read_json_file(&fs, &path).flat_map(move |v| decoded(&path, &v)),
    Some("nix") => eval
      .eval_file(&path)
      .map_err(nix_eval_failed)
      .flat_map(move |v| decoded(&path, &v)),
    _ => fail(InfraError::SuiteLoad(format!(
      "unsupported assay suite: {}",
      path.display()
    ))),
  }
}

fn decoded(path: &Path, v: &Value) -> Effect<Vec<(String, Claim)>, InfraError> {
  from_result(parse_assay_suite(v).map_err(|errors| invalid_suite(path, &errors)))
}

/// The JSON at `path`; unreadable or malformed files are load errors.
pub(crate) fn read_json_file(fs: &Arc<dyn Fs>, path: &Path) -> Effect<Value, InfraError> {
  let path: PathBuf = path.to_path_buf();
  fs.read_text(&path)
    .map_err(|e| reword_io(e, |m| InfraError::SuiteLoad(format!("read {m}"))))
    .flat_map(move |data| {
      from_result(
        serde_json::from_str(&data)
          .map_err(|e| InfraError::SuiteLoad(format!("read {}: {e}", path.display()))),
      )
    })
}

/// A failed suite evaluation as a load error.
pub(crate) fn nix_eval_failed(outcome: AssayOutcome) -> InfraError {
  InfraError::SuiteLoad(match outcome {
    AssayOutcome::EvalError { message, .. } => format!("nix eval failed: {message}"),
    other => format!("nix eval failed: {other:?}"),
  })
}

#[cfg(test)]
// Tests parse JSON fixtures directly; production decoding goes through `Schema`.
#[cfg_attr(dylint_lib = "id_effect_lints", allow(schema_parsing_at_io_boundary))]
mod tests {
  use id_effect::{expect_exit_failure, expect_exit_success, run_test};
  use serde_json::json;

  use super::*;
  use crate::eval::EvalResult;
  use crate::fake::{MemFs, MockNixEval};
  use crate::live::LiveFs;

  const MALFORMED: &str = include_str!("../../tests/fixtures/malformed.assay.json");

  fn mock() -> Arc<dyn NixEval> {
    Arc::new(MockNixEval::default())
  }

  fn mem(files: &[(&str, &str)]) -> Arc<dyn Fs> {
    Arc::new(MemFs::with_files(files.iter().copied()))
  }

  fn load_err(eval: Arc<dyn NixEval>, fs: Arc<dyn Fs>, path: &str) -> String {
    match expect_exit_failure(run_test(load_assay_suite(eval, fs, Path::new(path)))) {
      InfraError::SuiteLoad(message) => message,
      other => panic!("expected SuiteLoad, got {other:?}"),
    }
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
    assert!(matches!(cases[0].1, Claim::Eq { .. }));
    assert!(matches!(cases[1].1, Claim::Throws { .. }));
  }

  #[test]
  fn load_assay_suite_json_reads_through_fs() {
    let fs = mem(&[(
      "/s/suite.assay.json",
      r#"{"name":"t","cases":{"one":{"claim":"eq","expr":"1","expected":"1"}}}"#,
    )]);
    let cases = expect_exit_success(run_test(load_assay_suite(
      mock(),
      fs,
      Path::new("/s/suite.assay.json"),
    )));
    assert_eq!(cases.len(), 1);
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
      mem(&[]),
      Path::new("/s.assay.nix"),
    )));
    assert_eq!(cases.len(), 1);
    let err = load_err(mock(), mem(&[]), "/x.assay.nix");
    assert!(err.starts_with("nix eval failed"), "{err}");
  }

  #[test]
  fn load_assay_suite_unsupported_extension() {
    assert_eq!(
      load_err(mock(), mem(&[]), "/tmp/assay-suite.bad"),
      "unsupported assay suite: /tmp/assay-suite.bad"
    );
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
  fn load_assay_suite_missing_or_corrupt_file_errors() {
    assert_eq!(
      load_err(mock(), mem(&[]), "/missing.json"),
      "read /missing.json: not found"
    );
    let err = load_err(mock(), mem(&[("/bad.json", "{")]), "/bad.json");
    assert!(err.starts_with("read /bad.json: "), "{err}");
  }

  /// A malformed `.nix` suite lists every issue, each with its path, after the suite path.
  #[test]
  fn malformed_nix_suite_reports_every_issue() {
    let eval = MockNixEval::default();
    eval.set_file(
      "/m.assay.nix",
      EvalResult::Ok(serde_json::from_str(MALFORMED).expect("fixture")),
    );
    let err = load_err(Arc::new(eval), mem(&[]), "/m.assay.nix");
    assert!(
      err.starts_with("/m.assay.nix: invalid suite, 7 errors:\n  cases.badAttrs.attrs.1: "),
      "{err}"
    );
    assert_eq!(err.lines().count(), 8, "{err}");
  }

  /// The fixture on disk, through the live filesystem: the user-visible message.
  #[test]
  fn malformed_json_fixture_through_live_fs() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/malformed.assay.json");
    let shown = path.display().to_string();
    let err = load_err(mock(), Arc::new(LiveFs), &shown);
    assert_eq!(
      err,
      format!(
        "{shown}: invalid suite, 7 errors:\n  \
         cases.badAttrs.attrs.1: expected string\n  \
         cases.badKind.claim: unknown claim \"equals\", expected one of \
         eq|throws|subset|hasAttrs|drv|snapshot|forces|module|law|prop|pathInfo\n  \
         cases.badProp.name: missing field\n  \
         cases.badProp.seed: expected i64\n  \
         cases.badProp.trials: expected an integer in 0..=4294967295\n  \
         cases.badSeed.seed: expected a non-negative integer\n  \
         cases.noExpected.expected: missing field"
      )
    );
  }
}
