//! Effect programs for CLI commands, and the pure renderers for their typed outputs.
//!
//! Input bytes become a [`serde_json::Value`] at one parse boundary ([`load_json`]); the query
//! logic behind each command is plain Rust over that value.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use id_effect::{Effect, effect};
use nixstore::edge::from_result;
use nixstore::{NixstoreEnv, PathInfoFlags, PathInfoReport};
use serde_json::Value;

use crate::caps::{JsonSourceService, NixqEnv};
use crate::diff::{structural_diff, values_equal};
use crate::error::{InfraError, PathError, PredicateResult};
use crate::force_path::force_paths;
use crate::normalize::normalize_value;
use crate::path::{AttrPath, get_at_path, parse_attrpath};
use crate::predicate::{value_contains_subset, value_has_attrs};

/// One JSON document as output (`get`, `normalize`).
#[derive(Debug, Clone, PartialEq)]
pub struct JsonReport {
  pub value: Value,
}

impl JsonReport {
  /// Pretty-printed JSON (no trailing newline).
  pub fn render(&self) -> String {
    serde_json::to_string_pretty(&self.value).unwrap_or_else(|_| self.value.to_string())
  }
}

/// Result of `diff`: equal after normalization, or the structural diff lines.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiffReport {
  Equal,
  Differs(String),
}

impl DiffReport {
  /// The diff text to print (`None` when equal).
  pub fn render(&self) -> Option<&str> {
    match self {
      Self::Equal => None,
      Self::Differs(diff) => Some(diff),
    }
  }

  /// Process exit code: `0` when equal, `1` when the documents differ.
  pub fn exit_code(&self) -> u8 {
    match self {
      Self::Equal => 0,
      Self::Differs(_) => 1,
    }
  }
}

/// Parses `bytes` read from `input` as one JSON document.
pub fn parse_json(input: &Path, bytes: &[u8]) -> Result<Value, InfraError> {
  serde_json::from_slice(bytes).map_err(|e| InfraError::Json {
    path: input.display().to_string(),
    message: e.to_string(),
  })
}

/// The value at `path` in `value`.
pub fn lookup(value: &Value, path: &AttrPath) -> Result<Value, PathError> {
  get_at_path(value, path)
    .cloned()
    .ok_or_else(|| PathError::NotFound(path.display()))
}

/// Structural diff of `left` against `right` (empty / [`DiffReport::Equal`] when equal after
/// normalization).
pub fn diff_report(left: &Value, right: &Value) -> DiffReport {
  if values_equal(left, right) {
    return DiffReport::Equal;
  }
  let diff = structural_diff(&normalize_value(left), &normalize_value(right));
  if diff.is_empty() {
    DiffReport::Equal
  } else {
    DiffReport::Differs(diff)
  }
}

/// Reads `input` through the [`JsonSource`](crate::caps::JsonSource) and parses it.
pub fn load_json(input: PathBuf) -> Effect<Value, InfraError, NixqEnv> {
  effect!({
    let service = ~JsonSourceService;
    let source = Arc::clone(&service.0);
    let bytes = ~source.read(&input);
    ~from_result(parse_json(&input, &bytes))
  })
}

/// `get`: the value at `attrpath`; fails when the path is invalid or absent.
pub fn get(input: PathBuf, attrpath: String) -> Effect<JsonReport, InfraError, NixqEnv> {
  effect!({
    let value = ~load_json(input);
    let path = ~from_result(parse_attrpath(&attrpath).map_err(InfraError::from));
    let found = ~from_result(lookup(&value, &path).map_err(InfraError::from));
    JsonReport { value: found }
  })
}

/// `has-attrs`: true when the document is an object with every attr in `attrs`.
pub fn has_attrs(
  input: PathBuf,
  attrs: Vec<String>,
) -> Effect<PredicateResult, InfraError, NixqEnv> {
  effect!({
    let value = ~load_json(input);
    PredicateResult::from_bool(value_has_attrs(&value, &attrs))
  })
}

/// `subset`: true when the `expected` document is a structural subset of `input`.
pub fn subset(input: PathBuf, expected: PathBuf) -> Effect<PredicateResult, InfraError, NixqEnv> {
  effect!({
    let actual = ~load_json(input);
    let expect = ~load_json(expected);
    PredicateResult::from_bool(value_contains_subset(&actual, &expect))
  })
}

/// `force-path`: true when every attrpath in `paths` is valid and resolves.
pub fn force_path(
  input: PathBuf,
  paths: Vec<String>,
) -> Effect<PredicateResult, InfraError, NixqEnv> {
  effect!({
    let value = ~load_json(input);
    PredicateResult::from_bool(force_paths(&value, &paths).is_ok())
  })
}

/// `normalize`: the normalized document.
pub fn normalize(input: PathBuf) -> Effect<JsonReport, InfraError, NixqEnv> {
  effect!({
    let value = ~load_json(input);
    JsonReport {
      value: normalize_value(&value),
    }
  })
}

/// `diff`: structural diff of `input` against `right`.
pub fn diff(input: PathBuf, right: PathBuf) -> Effect<DiffReport, InfraError, NixqEnv> {
  effect!({
    let left = ~load_json(input);
    let right = ~load_json(right);
    diff_report(&left, &right)
  })
}

/// `path-info`: delegates to [`nixstore::path_info`] (provide `nixstore::layer` at the edge).
pub fn path_info(
  paths: Vec<String>,
  flags: PathInfoFlags,
) -> Effect<PathInfoReport, InfraError, NixstoreEnv> {
  nixstore::path_info(paths, flags).map_err(InfraError::from)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::caps::MockJsonSource;
  use id_effect::{assert_exit_fail, expect_exit_failure, expect_exit_success, run_test};

  fn source(files: &[(&str, &str)]) -> JsonSourceService {
    let mock = MockJsonSource::default();
    for (path, body) in files {
      mock.set_file(*path, body.as_bytes());
    }
    JsonSourceService(Arc::new(mock))
  }

  fn run<A: Send + 'static>(
    program: Effect<A, InfraError, NixqEnv>,
    files: &[(&str, &str)],
  ) -> id_effect::Exit<A, InfraError> {
    run_test(program.provide(source(files)))
  }

  #[test]
  fn get_reads_attrpath() {
    let report = expect_exit_success(run(
      get("in.json".into(), "a.b".into()),
      &[("in.json", r#"{"a":{"b":2}}"#)],
    ));
    assert_eq!(report.value, serde_json::json!(2));
    assert_eq!(report.render(), "2");
  }

  #[test]
  fn get_fails_on_missing_or_invalid_path() {
    let files = [("obj.json", r#"{"a":1}"#)];
    assert_exit_fail(
      &run(get("obj.json".into(), "missing".into()), &files),
      &InfraError::Path(PathError::NotFound("$.missing".into())),
    );
    assert!(matches!(
      expect_exit_failure(run(get("obj.json".into(), ".".into()), &files)),
      InfraError::Path(PathError::Invalid(_))
    ));
  }

  #[test]
  fn has_attrs_predicate() {
    let files = [("in.json", r#"{"a":1,"b":2}"#)];
    assert_eq!(
      expect_exit_success(run(has_attrs("in.json".into(), vec!["a".into()]), &files)),
      PredicateResult::True
    );
    assert_eq!(
      expect_exit_success(run(has_attrs("in.json".into(), vec!["z".into()]), &files)),
      PredicateResult::False
    );
  }

  #[test]
  fn subset_reads_both_documents() {
    let files = [
      ("a.json", r#"{"a":1,"c":3}"#),
      ("e.json", r#"{"a":1}"#),
      ("b.json", r#"{"b":1}"#),
    ];
    assert_eq!(
      expect_exit_success(run(subset("a.json".into(), "e.json".into()), &files)),
      PredicateResult::True
    );
    assert_eq!(
      expect_exit_success(run(subset("a.json".into(), "b.json".into()), &files)),
      PredicateResult::False
    );
  }

  #[test]
  fn force_path_and_normalize() {
    let files = [(
      "d.json",
      r#"{"outPath":"/nix/store/x","name":"x","builder":"b"}"#,
    )];
    assert_eq!(
      expect_exit_success(run(
        force_path("d.json".into(), vec!["outPath".into()]),
        &files
      )),
      PredicateResult::True
    );
    assert_eq!(
      expect_exit_success(run(
        force_path("d.json".into(), vec!["outPath.x".into()]),
        &files
      )),
      PredicateResult::False
    );
    let report = expect_exit_success(run(normalize("d.json".into()), &files));
    assert_eq!(report.value["type"], "derivation");
    assert_eq!(report.value["outPath"], "/nix/store/x");
  }

  #[test]
  fn diff_equal_and_unequal() {
    let files = [
      ("l.json", r#"{"a":1}"#),
      ("r.json", r#"{"a":1}"#),
      ("r2.json", r#"{"a":2}"#),
    ];
    let equal = expect_exit_success(run(diff("l.json".into(), "r.json".into()), &files));
    assert_eq!(equal, DiffReport::Equal);
    assert_eq!((equal.render(), equal.exit_code()), (None, 0));
    let differs = expect_exit_success(run(diff("l.json".into(), "r2.json".into()), &files));
    assert!(differs.render().is_some_and(|d| d.contains("~ $.a")));
    assert_eq!(differs.exit_code(), 1);
  }

  #[test]
  fn bad_json_and_missing_input_fail() {
    let files = [("bad.json", "not-json")];
    assert!(matches!(
      expect_exit_failure(run(load_json("bad.json".into()), &files)),
      InfraError::Json { ref path, .. } if path == "bad.json"
    ));
    assert!(matches!(
      expect_exit_failure(run(normalize("absent.json".into()), &files)),
      InfraError::Io { .. }
    ));
  }

  #[test]
  fn json_report_renders_pretty() {
    let report = JsonReport {
      value: serde_json::json!({"b": 2, "a": [3, 1]}),
    };
    assert_eq!(
      report.render(),
      "{\n  \"a\": [\n    3,\n    1\n  ],\n  \"b\": 2\n}"
    );
  }

  #[test]
  fn diff_report_is_pure() {
    let a = serde_json::json!({"a": 1});
    assert_eq!(diff_report(&a, &a), DiffReport::Equal);
    assert!(matches!(
      diff_report(&a, &serde_json::json!({"a": 2})),
      DiffReport::Differs(_)
    ));
  }
}

#[cfg(test)]
mod path_info_tests {
  use super::*;
  use id_effect::{assert_exit_fail, expect_exit_success, run_test};
  use nixstore::{MockPathInfoStore, PathInfoService};

  const A: &str = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-a";

  #[test]
  fn path_info_fixture_json() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../nixstore/fixtures/minimal");
    let flags = PathInfoFlags {
      json: true,
      closure_size: true,
      referrers: true,
      ..Default::default()
    };
    let report = expect_exit_success(run_test(
      path_info(vec![A.into()], flags).provide_layer(nixstore::layer(root)),
    ));
    let v = report.to_json_value();
    assert_eq!(v[A]["narSize"], 100);
    assert_eq!(v[A]["closureSize"], 350);
  }

  #[test]
  fn path_info_unknown_path_widens_store_error() {
    let store = PathInfoService(Arc::new(MockPathInfoStore::default()));
    assert_exit_fail(
      &run_test(path_info(vec![A.into()], PathInfoFlags::default()).provide(store)),
      &InfraError::Store(nixstore::InfraError::UnknownPath(A.into())),
    );
  }
}
