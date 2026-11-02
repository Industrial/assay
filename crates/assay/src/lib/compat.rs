//! nix-unit / `lib.runTests` compatibility: load `{ name = { expr; expected; }; }` suites.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use id_effect::schema::ParseErrors;
use id_effect::{Effect, fail};
use nixstore::edge::from_result;
use serde_json::Value;

use crate::assay_suite::{invalid_suite, nix_eval_failed, read_json_file};
use crate::eval::NixEval;
use crate::files::{EntryKind, Fs};
use crate::schema::decode_compat_cases;
use crate::verdict::InfraError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatCase {
  pub name: String,
  pub expr: String,
  pub expected: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompatSuite {
  pub cases: Vec<CompatCase>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CompatClaim {
  Eq {
    left_expr: String,
    right_expr: String,
  },
}

/// Map a compat case to an equality claim (`expr` must equal `expected`).
pub fn to_claim(case: &CompatCase) -> CompatClaim {
  CompatClaim::Eq {
    left_expr: case.expr.clone(),
    right_expr: case.expected.clone(),
  }
}

/// Parse a JSON object mapping test names to `{ expr, expected }` (cases by name); every decode
/// issue is reported.
pub fn parse_compat_json(v: &Value) -> Result<CompatSuite, ParseErrors> {
  decode_compat_cases(v).map(|cases| CompatSuite { cases })
}

/// Load a compat suite from `.json` (read with `fs`) or `.nix` (nix-unit / runTests shape;
/// evaluated with `eval`, falling back to a `.json` sidecar when that fails).
pub fn load_compat_suite(
  eval: Arc<dyn NixEval>,
  fs: Arc<dyn Fs>,
  path: &Path,
) -> Effect<CompatSuite, InfraError> {
  let path = path.to_path_buf();
  match path.extension().and_then(|e| e.to_str()) {
    Some("json") => load_json_file(&fs, path),
    Some("nix") => load_nix_compat_suite(eval, fs, path),
    _ => fail(InfraError::SuiteLoad(format!(
      "unsupported compat suite format: {}",
      path.display()
    ))),
  }
}

fn decoded(path: &Path, v: &Value) -> Effect<CompatSuite, InfraError> {
  from_result(parse_compat_json(v).map_err(|errors| invalid_suite(path, &errors)))
}

fn load_json_file(fs: &Arc<dyn Fs>, path: PathBuf) -> Effect<CompatSuite, InfraError> {
  read_json_file(fs, &path).flat_map(move |v| decoded(&path, &v))
}

fn sidecar_json_path(nix_path: &Path) -> PathBuf {
  nix_path.with_extension("json")
}

fn load_nix_compat_suite(
  eval: Arc<dyn NixEval>,
  fs: Arc<dyn Fs>,
  path: PathBuf,
) -> Effect<CompatSuite, InfraError> {
  let evaluated = path.clone();
  eval
    .eval_file(&path)
    .map_err(nix_eval_failed)
    .flat_map(move |v| decoded(&evaluated, &v))
    .catch(move |primary| {
      let sidecar = sidecar_json_path(&path);
      fs.kind(&sidecar).flat_map(move |kind| match kind {
        Some(EntryKind::File) => load_json_file(&fs, sidecar),
        _ => fail(primary),
      })
    })
}

fn nix_string_literal(s: &str) -> String {
  let escaped = s.replace('\\', "\\\\").replace('"', "\\\"");
  format!("\"{escaped}\"")
}

fn is_valid_nix_ident(s: &str) -> bool {
  if s.is_empty() {
    return false;
  }
  let mut chars = s.chars();
  match chars.next() {
    Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
    _ => return false,
  }
  chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-' || c == '\'')
}

fn nix_attr_name(s: &str) -> String {
  if is_valid_nix_ident(s) {
    s.to_string()
  } else {
    nix_string_literal(s)
  }
}

/// Convert a JSON field to a Nix expression string (strings pass through as source).
///
/// Used for nix-unit compat suites where `expr` / `expected` strings are Nix source.
pub fn field_to_nix(v: &Value) -> String {
  match v {
    Value::String(s) => s.clone(),
    other => value_to_nix_expr(other),
  }
}

/// Serialize a `nix eval --json` value as a Nix expression (including strings).
pub fn json_value_to_nix(v: &Value) -> String {
  value_to_nix_expr(v)
}

fn value_to_nix_expr(v: &Value) -> String {
  match v {
    Value::Null => "null".into(),
    Value::Bool(b) => b.to_string(),
    Value::Number(n) => n.to_string(),
    Value::String(s) => nix_string_literal(s),
    Value::Array(items) => {
      let inner: Vec<_> = items.iter().map(value_to_nix_expr).collect();
      format!("[{}]", inner.join(" "))
    }
    Value::Object(map) => {
      if map.is_empty() {
        return "{}".into();
      }
      let inner: Vec<_> = map
        .iter()
        .map(|(k, v)| format!("{} = {}", nix_attr_name(k), value_to_nix_expr(v)))
        .collect();
      format!("{{ {}; }}", inner.join("; "))
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::fake::MemFs;
  use crate::live::LiveFs;
  use serde_json::json;

  /// Loads `path` from `fs` with a fake evaluator whose `.nix` evaluation always fails.
  fn load_with(fs: Arc<dyn Fs>, path: &Path) -> Result<CompatSuite, InfraError> {
    let eval: Arc<dyn NixEval> = Arc::new(crate::fake::MockNixEval::default());
    id_effect::run_test(load_compat_suite(eval, fs, path))
      .into_result()
      .map_err(|c| match c {
        id_effect::Cause::Fail(e) => e,
        other => InfraError::Worker(format!("{other:?}")),
      })
  }

  fn load_in(fs: MemFs, path: &str) -> Result<CompatSuite, InfraError> {
    load_with(Arc::new(fs), Path::new(path))
  }

  #[test]
  fn parse_compat_json_five_cases() {
    let v = json!({
        "trim_identity": { "expr": "\"hi\"", "expected": "\"hi\"" },
        "list_len": { "expr": "builtins.length [1 2 3]", "expected": "3" },
        "bool_and": { "expr": "true && false", "expected": "false" },
        "json_number": { "expr": 42, "expected": 42 },
        "json_list": { "expr": [1, 2, 3], "expected": "[1 2 3]" },
    });
    let suite = parse_compat_json(&v).expect("parse");
    assert_eq!(suite.cases.len(), 5);

    let by_name: std::collections::HashMap<_, _> =
      suite.cases.iter().map(|c| (c.name.as_str(), c)).collect();

    assert_eq!(by_name["trim_identity"].expr, "\"hi\"");
    assert_eq!(by_name["list_len"].expr, "builtins.length [1 2 3]");
    assert_eq!(by_name["bool_and"].expected, "false");
    assert_eq!(by_name["json_number"].expr, "42");
    assert_eq!(by_name["json_number"].expected, "42");
    assert_eq!(by_name["json_list"].expr, "[1 2 3]");
  }

  #[test]
  fn to_claim_maps_eq() {
    let case = CompatCase {
      name: "x".into(),
      expr: "1 + 1".into(),
      expected: "2".into(),
    };
    assert_eq!(
      to_claim(&case),
      CompatClaim::Eq {
        left_expr: "1 + 1".into(),
        right_expr: "2".into(),
      }
    );
  }

  #[test]
  fn load_compat_suite_from_fixture_json() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures/compat/suite.json");
    let suite = load_with(Arc::new(LiveFs), &path).expect("load suite.json");
    assert!(suite.cases.len() >= 5);
  }
  #[test]
  fn json_value_to_nix_covers_types() {
    assert_eq!(json_value_to_nix(&json!(null)), "null");
    assert_eq!(json_value_to_nix(&json!(true)), "true");
    assert_eq!(json_value_to_nix(&json!(1)), "1");
    assert_eq!(json_value_to_nix(&json!("hi")), "\"hi\"");
    assert_eq!(json_value_to_nix(&json!([])), "[]");
    assert_eq!(json_value_to_nix(&json!({})), "{}");
  }

  #[test]
  fn load_compat_unsupported_extension() {
    let fs = MemFs::with_files([("/c/assay-bad.ext", "{}")]);
    assert!(load_in(fs, "/c/assay-bad.ext").is_err());
  }

  #[test]
  fn parse_compat_json_rejects_non_object() {
    assert!(parse_compat_json(&json!([])).is_err());
  }

  #[test]
  fn parse_compat_json_rejects_bad_case_shape() {
    assert!(parse_compat_json(&json!({"x": "not-object"})).is_err());
    assert!(parse_compat_json(&json!({"x": {"expected": "1"}})).is_err());
    assert!(parse_compat_json(&json!({"x": {"expr": "1"}})).is_err());
  }

  #[test]
  fn is_valid_nix_ident_and_attr_name() {
    assert!(!is_valid_nix_ident(""));
    assert!(!is_valid_nix_ident("1bad"));
    assert!(is_valid_nix_ident("foo-bar"));
    assert!(is_valid_nix_ident("_x"));
    assert!(!is_valid_nix_ident("has space"));
    assert!(!is_valid_nix_ident("good@bad"));
    assert!(!is_valid_nix_ident("ab.c"));
    assert_eq!(nix_attr_name("valid"), "valid");
    assert_eq!(nix_attr_name("bad key"), "\"bad key\"");
  }

  #[test]
  fn json_value_to_nix_nested_and_escaped() {
    let nested = json!({"a": 1, "bad key": [true, "x\"y"]});
    let rendered = json_value_to_nix(&nested);
    assert!(rendered.contains("a = 1"));
    assert!(rendered.contains("\"bad key\""));
    assert!(rendered.contains("true"));
    assert!(rendered.contains("x\\\"y"));
    assert!(nix_string_literal("a\"b").contains("\\\""));
  }

  #[test]
  fn field_to_nix_accepts_json_literals() {
    assert_eq!(field_to_nix(&json!("expr")), "expr");
    assert_eq!(field_to_nix(&json!(42)), "42");
  }

  #[test]
  fn load_nix_compat_suite_propagates_error_without_sidecar() {
    let fs = MemFs::with_files([("/c/suite.nix", "invalid nix {{{")]);
    let err = load_in(fs, "/c/suite.nix").expect_err("no sidecar");
    assert!(
      matches!(&err, InfraError::SuiteLoad(m) if m.starts_with("nix eval failed")),
      "{err:?}"
    );
  }

  #[test]
  fn load_nix_compat_suite_uses_sidecar_json() {
    let fs = MemFs::with_files([
      ("/c/suite.nix", "invalid nix {{{"),
      (
        "/c/suite.json",
        r#"{"sidecar": {"expr": "1", "expected": "1"}}"#,
      ),
    ]);
    let suite = load_in(fs, "/c/suite.nix").expect("sidecar fallback");
    assert_eq!(suite.cases.len(), 1);
    assert_eq!(suite.cases[0].name, "sidecar");
  }

  #[test]
  fn load_compat_json_reports_every_bad_case() {
    let fs = MemFs::with_files([(
      "/c/bad.json",
      r#"{"x": "not-object", "y": {"expected": "1"}}"#,
    )]);
    let err = load_in(fs, "/c/bad.json").expect_err("invalid");
    assert_eq!(
      err,
      InfraError::SuiteLoad(
        "/c/bad.json: invalid suite, 2 errors:\n  x: expected object\n  y.expr: missing field"
          .into()
      )
    );
  }
}
