//! Claim algebra interpreter — `Effect`s returning [`CaseVerdict`].
//!
//! Each claim kind needs only its own capabilities (see [`crate::caps`]): the interpreters take
//! the capability values they use ([`interpret_with`]); [`interpret_claim`], [`eval_claim`],
//! [`snapshot_claim`] and [`path_info_claim`] read them from their environments.

use std::sync::Arc;

use serde_json::Value;

use id_effect::{Effect, fail, need, succeed, sync};
use nixstore::PathInfoStore;

use crate::caps::{
  AssayEnv, EvalEnv, FsService, NixEvalService, PathInfoEnv, PathInfoService, SnapshotEnv,
  SnapshotService,
};
use crate::eval::NixEval;
use crate::files::Fs;
use crate::force::check_forces;
use crate::outcome::AssayOutcome;
use crate::snapshot::SnapshotStore;
use crate::verdict::{CaseVerdict, InfraError, outcome_to_effect};
use nixq::{normalize_value, structural_diff, value_contains_subset, value_has_attrs};

/// A single test claim authored in Nix and interpreted by the runner.
#[derive(Debug, Clone, PartialEq)]
pub enum Claim {
  Eq {
    left_expr: String,
    right_expr: String,
  },
  /// Already-evaluated sides from suite JSON (first-class Nix authoring).
  EqValues {
    left: Value,
    right: Value,
  },
  Throws {
    expr: String,
    pattern: Option<String>,
  },
  Subset {
    expr: String,
    expected_subset: Value,
  },
  SubsetValues {
    actual: Value,
    expected_subset: Value,
  },
  HasAttrs {
    expr: String,
    attrs: Vec<String>,
  },
  HasAttrsValues {
    actual: Value,
    attrs: Vec<String>,
  },
  Drv {
    expr: String,
    project: Vec<String>,
    expected: Value,
  },
  DrvValues {
    actual: Value,
    project: Vec<String>,
    expected: Value,
  },
  Snapshot {
    name: String,
    expr: String,
  },
  Forces {
    expr: String,
    paths: Vec<String>,
  },
  Module {
    imports_expr: String,
    args_expr: String,
    expect: Value,
  },
  /// Run a built-in algebraic law by name.
  Law {
    name: String,
    seed: u64,
  },
  /// Run a built-in property check by name.
  Prop {
    name: String,
    seed: u64,
    trials: Option<u32>,
  },
  /// Assert `expect` is a structural subset of store path-info JSON.
  PathInfo {
    path: String,
    expect: Value,
    referrers: bool,
    closure_size: bool,
  },
}

/// The capability values a run holds (what [`AssayEnv`] provides); each claim kind uses only
/// its own part (see [`interpret_with`]).
#[derive(Clone)]
pub struct ClaimCaps {
  pub eval: Arc<dyn NixEval>,
  pub snapshots: SnapshotStore,
  pub path_info: Arc<dyn PathInfoStore>,
  /// Snapshot goldens (and, for a run, suite files and discovery).
  pub fs: Arc<dyn Fs>,
}

/// Reads the [`ClaimCaps`] out of [`AssayEnv`] (nothing is opened or spawned here).
pub fn claim_caps() -> Effect<ClaimCaps, InfraError, AssayEnv> {
  need::<NixEvalService>().flat_map(|eval: Arc<NixEvalService>| {
    need::<SnapshotService>().flat_map(move |snapshots: Arc<SnapshotService>| {
      need::<PathInfoService>().flat_map(move |path_info: Arc<PathInfoService>| {
        need::<FsService>().map(move |fs: Arc<FsService>| ClaimCaps {
          eval: Arc::clone(&eval.0),
          snapshots: snapshots.0.clone(),
          path_info: Arc::clone(&path_info.0),
          fs: Arc::clone(&fs.0),
        })
      })
    })
  })
}

/// Interpret `claim` using capabilities from [`AssayEnv`].
pub fn interpret_claim(claim: Claim) -> Effect<CaseVerdict, InfraError, AssayEnv> {
  claim_caps().flat_map(move |caps| interpret_with(&caps, claim).any_env())
}

/// A claim that needs only Nix evaluation (or nothing), in [`EvalEnv`]. A `snapshot` /
/// `pathInfo` claim needs more: use [`snapshot_claim`] / [`path_info_claim`].
pub fn eval_claim(claim: Claim) -> Effect<CaseVerdict, InfraError, EvalEnv> {
  need::<NixEvalService>().flat_map(move |eval: Arc<NixEvalService>| {
    interpret_outcome(&eval.0, None, None, claim).any_env()
  })
}

/// A `snapshot` claim, in [`SnapshotEnv`].
pub fn snapshot_claim(name: String, expr: String) -> Effect<CaseVerdict, InfraError, SnapshotEnv> {
  need::<NixEvalService>().flat_map(move |eval: Arc<NixEvalService>| {
    need::<SnapshotService>().flat_map(move |snapshots: Arc<SnapshotService>| {
      need::<FsService>().flat_map(move |fs: Arc<FsService>| {
        interpret_snapshot(&eval.0, &fs.0, snapshots.0.clone(), name, &expr)
          .flat_map(outcome_to_effect)
          .any_env()
      })
    })
  })
}

/// A `pathInfo` claim, in [`PathInfoEnv`].
pub fn path_info_claim(
  path: String,
  expect: Value,
  referrers: bool,
  closure_size: bool,
) -> Effect<CaseVerdict, InfraError, PathInfoEnv> {
  need::<PathInfoService>().flat_map(move |store: Arc<PathInfoService>| {
    interpret_path_info(&store.0, &path, expect, referrers, closure_size)
      .flat_map(outcome_to_effect)
      .any_env()
  })
}

/// Interpret `claim` with `caps`: evaluating claims use `caps.eval`, `snapshot` also
/// `caps.snapshots`, `pathInfo` only `caps.path_info`; value claims, laws and props none.
pub fn interpret_with(caps: &ClaimCaps, claim: Claim) -> Effect<CaseVerdict, InfraError> {
  interpret_outcome(
    &caps.eval,
    Some((&caps.snapshots, &caps.fs)),
    Some(&caps.path_info),
    claim,
  )
}

/// Dispatch on the claim kind. A `snapshot` / `pathInfo` claim without its capability is a
/// runner error.
fn interpret_outcome(
  eval: &Arc<dyn NixEval>,
  snapshots: Option<(&SnapshotStore, &Arc<dyn Fs>)>,
  path_info: Option<&Arc<dyn PathInfoStore>>,
  claim: Claim,
) -> Effect<CaseVerdict, InfraError> {
  let outcome: Effect<AssayOutcome, InfraError> = match claim {
    Claim::Eq {
      left_expr,
      right_expr,
    } => interpret_eq(eval, &left_expr, &right_expr),
    Claim::EqValues { left, right } => pure(move || interpret_eq_values(&left, &right)),
    Claim::Throws { expr, pattern } => interpret_throws(eval, &expr, pattern),
    Claim::Subset {
      expr,
      expected_subset,
    } => eval_then(eval, &expr, move |actual| {
      interpret_subset_values(&actual, &expected_subset)
    }),
    Claim::SubsetValues {
      actual,
      expected_subset,
    } => pure(move || interpret_subset_values(&actual, &expected_subset)),
    Claim::HasAttrs { expr, attrs } => eval_then(eval, &expr, move |value| {
      interpret_has_attrs_values(&value, &attrs)
    }),
    Claim::HasAttrsValues { actual, attrs } => {
      pure(move || interpret_has_attrs_values(&actual, &attrs))
    }
    Claim::Drv {
      expr,
      project,
      expected,
    } => eval_then(eval, &expr, move |v| {
      interpret_drv_values(&v, &project, &expected)
    }),
    Claim::DrvValues {
      actual,
      project,
      expected,
    } => pure(move || interpret_drv_values(&actual, &project, &expected)),
    Claim::Snapshot { name, expr } => match snapshots {
      Some((store, fs)) => interpret_snapshot(eval, fs, store.clone(), name, &expr),
      None => fail(missing_capability("snapshot", "SnapshotService")),
    },
    Claim::Forces { expr, paths } => pure(move || check_forces(&expr, &paths)),
    Claim::Module {
      imports_expr,
      args_expr,
      expect,
    } => interpret_module(eval, &imports_expr, &args_expr, expect),
    Claim::Law { name, seed } => pure(move || crate::laws::run_law_by_name(&name, seed)),
    Claim::Prop { name, seed, trials } => {
      pure(move || crate::prop::run_prop_by_name(&name, seed, trials.unwrap_or(128)))
    }
    Claim::PathInfo {
      path,
      expect,
      referrers,
      closure_size,
    } => match path_info {
      Some(store) => interpret_path_info(store, &path, expect, referrers, closure_size),
      None => fail(missing_capability("pathInfo", "PathInfoService")),
    },
  };
  outcome.flat_map(outcome_to_effect)
}

fn missing_capability(claim: &str, service: &str) -> InfraError {
  InfraError::Worker(format!("{claim} claim interpreted without {service}"))
}

/// A pure interpretation, run when the effect runs.
fn pure(f: impl FnOnce() -> AssayOutcome + Send + 'static) -> Effect<AssayOutcome, InfraError> {
  sync(f)
}

/// Evaluate `expr` and judge the value with `judge`; a failed evaluation is the outcome.
fn eval_then(
  eval: &Arc<dyn NixEval>,
  expr: &str,
  judge: impl FnOnce(Value) -> AssayOutcome + Send + 'static,
) -> Effect<AssayOutcome, InfraError> {
  eval.eval_expr(expr).map(judge).catch(succeed)
}

fn interpret_eq(
  eval: &Arc<dyn NixEval>,
  left_expr: &str,
  right_expr: &str,
) -> Effect<AssayOutcome, InfraError> {
  // One nix process for both sides — process spawn dominates tiny exprs.
  let pair_expr = format!("[({left_expr}) ({right_expr})]");
  eval_then(eval, &pair_expr, |pair| match pair {
    Value::Array(arr) if arr.len() == 2 => {
      compare_eq(normalize_value(&arr[0]), normalize_value(&arr[1]))
    }
    other => AssayOutcome::EvalError {
      kind: "eq_pair".into(),
      message: format!("eq pair eval expected 2-element list, got {other}"),
      span: None,
    },
  })
}

fn interpret_eq_values(left: &Value, right: &Value) -> AssayOutcome {
  compare_eq(normalize_value(left), normalize_value(right))
}

fn compare_eq(left: Value, right: Value) -> AssayOutcome {
  if left == right {
    AssayOutcome::Pass
  } else {
    AssayOutcome::Fail {
      claim: "eq".into(),
      left: Some(left.clone()),
      right: Some(right.clone()),
      diff: structural_diff(&left, &right),
    }
  }
}

fn interpret_throws(
  eval: &Arc<dyn NixEval>,
  expr: &str,
  pattern: Option<String>,
) -> Effect<AssayOutcome, InfraError> {
  eval
    .eval_expr(expr)
    .map(|value| AssayOutcome::Fail {
      claim: "throws".into(),
      left: Some(value),
      right: None,
      diff: "expression evaluated successfully; expected throw".into(),
    })
    .catch(move |out| succeed(judge_throw(out, pattern.as_deref())))
}

fn judge_throw(out: AssayOutcome, pattern: Option<&str>) -> AssayOutcome {
  if !is_throw_outcome(&out) {
    return out;
  }
  let Some(pat) = pattern else {
    return AssayOutcome::Pass;
  };
  let message = throw_message(&out);
  if message.contains(pat) {
    AssayOutcome::Pass
  } else {
    AssayOutcome::Fail {
      claim: "throws".into(),
      left: None,
      right: None,
      diff: format!("throw message {message:?} does not contain pattern {pat:?}"),
    }
  }
}

pub fn build_module_eval_expr(imports_expr: &str, args_expr: &str) -> String {
  format!(
    "let lib = (import <nixpkgs> {{}}).lib; eval = lib.evalModules {{ modules = {imports_expr}; specialArgs = {args_expr}; }}; in eval.config"
  )
}

fn interpret_module(
  eval: &Arc<dyn NixEval>,
  imports_expr: &str,
  args_expr: &str,
  expect: Value,
) -> Effect<AssayOutcome, InfraError> {
  let expr = build_module_eval_expr(imports_expr, args_expr);
  eval_then(eval, &expr, move |actual| {
    if value_contains_subset(&actual, &expect) {
      AssayOutcome::Pass
    } else {
      AssayOutcome::Fail {
        claim: "module".into(),
        left: Some(actual.clone()),
        right: Some(expect.clone()),
        diff: structural_diff(&actual, &expect),
      }
    }
  })
}

fn interpret_subset_values(actual: &Value, expected_subset: &Value) -> AssayOutcome {
  if value_contains_subset(actual, expected_subset) {
    AssayOutcome::Pass
  } else {
    AssayOutcome::Fail {
      claim: "subset".into(),
      left: Some(actual.clone()),
      right: Some(expected_subset.clone()),
      diff: structural_diff(actual, expected_subset),
    }
  }
}

fn interpret_has_attrs_values(value: &Value, attrs: &[String]) -> AssayOutcome {
  if value_has_attrs(value, attrs) {
    AssayOutcome::Pass
  } else {
    AssayOutcome::Fail {
      claim: "hasAttrs".into(),
      left: Some(value.clone()),
      right: None,
      diff: format!("missing attrs among {:?}", attrs),
    }
  }
}

fn interpret_drv_values(
  actual: &Value,
  project_fields: &[String],
  expected: &Value,
) -> AssayOutcome {
  let drv = match nixdrv::Derivation::from_json(actual) {
    Ok(d) => d,
    Err(e) => {
      return AssayOutcome::EvalError {
        kind: "drv".into(),
        message: e.to_string(),
        span: None,
      };
    }
  };
  let fields: Vec<&str> = project_fields.iter().map(String::as_str).collect();
  let projected = nixdrv::project(&drv, &fields);
  if value_contains_subset(&projected, expected) {
    AssayOutcome::Pass
  } else {
    let diff = structural_diff(&projected, expected);
    AssayOutcome::Fail {
      claim: "drv".into(),
      left: Some(projected),
      right: Some(expected.clone()),
      diff,
    }
  }
}

fn interpret_path_info(
  store: &Arc<dyn PathInfoStore>,
  path: &str,
  expect: Value,
  referrers: bool,
  closure_size: bool,
) -> Effect<AssayOutcome, InfraError> {
  let opts = nixstore::QueryOpts {
    include_referrers: referrers,
    include_closure_size: closure_size,
  };
  store
    .query(path, opts)
    .map(move |info| {
      let actual = info.to_json_value();
      if value_contains_subset(&actual, &expect) {
        AssayOutcome::Pass
      } else {
        let diff = structural_diff(&actual, &expect);
        AssayOutcome::Fail {
          claim: "pathInfo".into(),
          left: Some(actual),
          right: Some(expect),
          diff,
        }
      }
    })
    .catch(|e| {
      succeed(AssayOutcome::EvalError {
        kind: "pathInfo".into(),
        message: e.to_string(),
        span: None,
      })
    })
}

fn interpret_snapshot(
  eval: &Arc<dyn NixEval>,
  fs: &Arc<dyn Fs>,
  store: SnapshotStore,
  name: String,
  expr: &str,
) -> Effect<AssayOutcome, InfraError> {
  let fs = Arc::clone(fs);
  eval
    .eval_expr(expr)
    .map(Ok)
    .catch(|failed| succeed(Err(failed)))
    .flat_map(move |evaluated| match evaluated {
      Ok(v) => store.assert_match(&fs, &name, normalize_value(&v), store.update_snapshots),
      Err(outcome) => succeed(outcome),
    })
}

fn is_throw_outcome(out: &AssayOutcome) -> bool {
  matches!(
    out,
    AssayOutcome::EvalError { .. } | AssayOutcome::Recursion
  )
}

fn throw_message(out: &AssayOutcome) -> String {
  match out {
    AssayOutcome::EvalError { message, .. } => message.clone(),
    AssayOutcome::Recursion => "infinite recursion".into(),
    _ => String::new(),
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use id_effect::{Exit, Layer, hlist, run_test};

  use super::*;
  use crate::eval::EvalResult;
  use crate::fake::{MockNixEval, fake_layer};

  use crate::verdict::CaseVerdict;

  fn run_claim_test<F>(setup: F, claim: Claim) -> Exit<CaseVerdict, InfraError>
  where
    F: FnOnce(&MockNixEval),
  {
    let mock = MockNixEval::default();
    setup(&mock);
    run_test(interpret_claim(claim).provide_layer(fake_layer(mock)))
  }

  #[test]
  fn eq_passes_when_normalized_values_match() {
    let claim = Claim::Eq {
      left_expr: "a".into(),
      right_expr: "b".into(),
    };
    let exit = run_claim_test(
      |eval| {
        eval.set("a", EvalResult::Ok(serde_json::json!({"x": 1})));
        eval.set("b", EvalResult::Ok(serde_json::json!({"x": 1})));
      },
      claim,
    );
    assert!(matches!(exit, Exit::Success(CaseVerdict::Pass)));
  }

  #[test]
  fn eq_fails_with_diff_when_values_differ() {
    let claim = Claim::Eq {
      left_expr: "a".into(),
      right_expr: "b".into(),
    };
    let exit = run_claim_test(
      |eval| {
        eval.set("a", EvalResult::Ok(serde_json::json!({"x": 1})));
        eval.set("b", EvalResult::Ok(serde_json::json!({"x": 2})));
      },
      claim,
    );
    assert!(matches!(
      exit,
      Exit::Success(CaseVerdict::AssertFail { .. })
    ));
  }

  #[test]
  fn throws_passes_on_eval_error_without_pattern() {
    let claim = Claim::Throws {
      expr: "bad".into(),
      pattern: None,
    };
    let exit = run_claim_test(
      |eval| {
        eval.set(
          "bad",
          EvalResult::Err(AssayOutcome::EvalError {
            kind: "type".into(),
            message: "boom".into(),
            span: None,
          }),
        );
      },
      claim,
    );
    assert!(matches!(exit, Exit::Success(CaseVerdict::Pass)));
  }

  #[test]
  fn throws_fails_when_expression_succeeds() {
    let claim = Claim::Throws {
      expr: "ok".into(),
      pattern: None,
    };
    let exit = run_claim_test(
      |eval| eval.set("ok", EvalResult::Ok(serde_json::json!(1))),
      claim,
    );
    assert!(matches!(
      exit,
      Exit::Success(CaseVerdict::AssertFail { .. })
    ));
  }

  #[test]
  fn throws_pattern_must_match_message() {
    let pass = Claim::Throws {
      expr: "bad".into(),
      pattern: Some("foo".into()),
    };
    let pass_exit = run_claim_test(
      |eval| {
        eval.set(
          "bad",
          EvalResult::Err(AssayOutcome::EvalError {
            kind: "type".into(),
            message: "undefined variable foo".into(),
            span: None,
          }),
        );
      },
      pass,
    );
    assert!(matches!(pass_exit, Exit::Success(CaseVerdict::Pass)));

    let fail = Claim::Throws {
      expr: "bad".into(),
      pattern: Some("bar".into()),
    };
    let fail_exit = run_claim_test(
      |eval| {
        eval.set(
          "bad",
          EvalResult::Err(AssayOutcome::EvalError {
            kind: "type".into(),
            message: "undefined variable foo".into(),
            span: None,
          }),
        );
      },
      fail,
    );
    assert!(matches!(
      fail_exit,
      Exit::Success(CaseVerdict::AssertFail { .. })
    ));
  }

  #[test]
  fn subset_requires_recursive_object_containment() {
    let pass = Claim::Subset {
      expr: "v".into(),
      expected_subset: serde_json::json!({"a": {"b": 1}}),
    };
    let pass_exit = run_claim_test(
      |eval| {
        eval.set(
          "v",
          EvalResult::Ok(serde_json::json!({"a": {"b": 1}, "c": 2})),
        )
      },
      pass,
    );
    assert!(matches!(pass_exit, Exit::Success(CaseVerdict::Pass)));

    let fail = Claim::Subset {
      expr: "v".into(),
      expected_subset: serde_json::json!({"a": {"b": 9}}),
    };
    let fail_exit = run_claim_test(
      |eval| {
        eval.set(
          "v",
          EvalResult::Ok(serde_json::json!({"a": {"b": 1}, "c": 2})),
        )
      },
      fail,
    );
    assert!(matches!(
      fail_exit,
      Exit::Success(CaseVerdict::AssertFail { .. })
    ));
  }

  #[test]
  fn has_attrs_requires_object_keys() {
    let pass = Claim::HasAttrs {
      expr: "v".into(),
      attrs: vec!["a".into(), "b".into()],
    };
    let pass_exit = run_claim_test(
      |eval| eval.set("v", EvalResult::Ok(serde_json::json!({"a": 1, "b": 2}))),
      pass,
    );
    assert!(matches!(pass_exit, Exit::Success(CaseVerdict::Pass)));

    let fail = Claim::HasAttrs {
      expr: "v".into(),
      attrs: vec!["z".into()],
    };
    let fail_exit = run_claim_test(
      |eval| eval.set("v", EvalResult::Ok(serde_json::json!({"a": 1, "b": 2}))),
      fail,
    );
    assert!(matches!(
      fail_exit,
      Exit::Success(CaseVerdict::AssertFail { .. })
    ));
  }

  #[test]
  fn snapshot_missing_golden_is_snapshot_mismatch() {
    let claim = Claim::Snapshot {
      name: "__assay_no_such_golden__".into(),
      expr: "v".into(),
    };
    let exit = run_claim_test(
      |eval| eval.set("v", EvalResult::Ok(serde_json::json!({"x": 1}))),
      claim,
    );
    assert!(matches!(
      exit,
      Exit::Success(CaseVerdict::SnapshotMismatch { .. })
    ));
  }

  #[test]
  fn forces_delegates_to_force_module_and_fails() {
    let claim = Claim::Forces {
      expr: "x".into(),
      paths: vec!["a".into()],
    };
    let exit = run_claim_test(|_eval| {}, claim);
    assert!(matches!(
      exit,
      Exit::Success(CaseVerdict::AssertFail { .. })
    ));
  }

  #[test]
  fn module_passes_when_config_contains_expect_subset() {
    let imports = "[ ]";
    let args = "{}";
    let expr = build_module_eval_expr(imports, args);
    let claim = Claim::Module {
      imports_expr: imports.into(),
      args_expr: args.into(),
      expect: serde_json::json!({ "assay": { "tiny": { "message": "hello" } } }),
    };
    let exit = run_claim_test(
      |eval| {
        eval.set(
          &expr,
          EvalResult::Ok(serde_json::json!({
              "assay": { "tiny": { "enable": true, "message": "hello" } },
              "other": "ignored"
          })),
        );
      },
      claim,
    );
    assert!(matches!(exit, Exit::Success(CaseVerdict::Pass)));
  }

  #[test]
  fn module_fails_when_config_missing_expect_subset() {
    let imports = "[ ]";
    let args = "{}";
    let expr = build_module_eval_expr(imports, args);
    let claim = Claim::Module {
      imports_expr: imports.into(),
      args_expr: args.into(),
      expect: serde_json::json!({ "assay": { "tiny": { "message": "hello" } } }),
    };
    let exit = run_claim_test(
      |eval| {
        eval.set(
          &expr,
          EvalResult::Ok(serde_json::json!({ "assay": { "tiny": { "message": "nope" } } })),
        );
      },
      claim,
    );
    assert!(matches!(
      exit,
      Exit::Success(CaseVerdict::AssertFail { .. })
    ));
  }

  #[test]
  fn module_propagates_eval_error() {
    let imports = "[ ]";
    let args = "{}";
    let expr = build_module_eval_expr(imports, args);
    let claim = Claim::Module {
      imports_expr: imports.into(),
      args_expr: args.into(),
      expect: serde_json::json!({}),
    };
    let exit = run_claim_test(
      |eval| {
        eval.set(
          &expr,
          EvalResult::Err(AssayOutcome::EvalError {
            kind: "throw".into(),
            message: "module eval failed".into(),
            span: None,
          }),
        );
      },
      claim,
    );
    assert!(matches!(exit, Exit::Success(CaseVerdict::EvalThrow { .. })));
  }
  #[test]
  fn eq_values_pass_without_nix() {
    let exit = run_claim_test(
      |_eval| {},
      Claim::EqValues {
        left: serde_json::json!(1),
        right: serde_json::json!(1),
      },
    );
    assert!(matches!(exit, Exit::Success(CaseVerdict::Pass)));
  }

  #[test]
  fn eq_values_fail_when_different() {
    let exit = run_claim_test(
      |_eval| {},
      Claim::EqValues {
        left: serde_json::json!(1),
        right: serde_json::json!(2),
      },
    );
    assert!(matches!(
      exit,
      Exit::Success(CaseVerdict::AssertFail { .. })
    ));
  }

  #[test]
  fn subset_values_and_hasattrs_values() {
    let sub = run_claim_test(
      |_eval| {},
      Claim::SubsetValues {
        actual: serde_json::json!({"a": 1, "b": 2}),
        expected_subset: serde_json::json!({"a": 1}),
      },
    );
    assert!(matches!(sub, Exit::Success(CaseVerdict::Pass)));

    let has = run_claim_test(
      |_eval| {},
      Claim::HasAttrsValues {
        actual: serde_json::json!({"a": 1}),
        attrs: vec!["a".into()],
      },
    );
    assert!(matches!(has, Exit::Success(CaseVerdict::Pass)));
  }

  #[test]
  fn law_and_prop_claims_run() {
    let law = run_claim_test(
      |_eval| {},
      Claim::Law {
        name: "merge_idempotent".into(),
        seed: 1,
      },
    );
    assert!(matches!(
      law,
      Exit::Success(CaseVerdict::Pass) | Exit::Success(CaseVerdict::AssertFail { .. })
    ));

    let prop = run_claim_test(
      |_eval| {},
      Claim::Prop {
        name: "always_pass".into(),
        seed: 1,
        trials: Some(4),
      },
    );
    assert!(matches!(prop, Exit::Success(CaseVerdict::Pass)));
  }

  /// `law` / `prop` claims decoded from arbitrary suite wires (`from_schema` over the claim
  /// schema) and interpreted as effects (`prop_check`): every built-in passes for any seed.
  mod property {
    use id_effect::HasSchema;
    use id_effect::schema::Unknown;
    use id_effect::testing::property::{from_schema, prop_check, unknown_object};
    use proptest::prelude::*;
    use proptest::sample::select;
    use proptest::test_runner::TestCaseError;

    use super::*;
    use crate::laws::BUILTIN_LAW_NAMES;
    use crate::prop::BUILTIN_PROP_NAMES;

    fn tag(kind: &'static str) -> BoxedStrategy<Unknown> {
      Just(Unknown::String(kind.into())).boxed()
    }

    fn name(names: &'static [&'static str]) -> BoxedStrategy<Unknown> {
      select(names)
        .prop_map(|n| Unknown::String(n.into()))
        .boxed()
    }

    fn seed() -> BoxedStrategy<Unknown> {
      (0..=i64::MAX).prop_map(Unknown::I64).boxed()
    }

    fn law_or_prop_wire() -> impl Strategy<Value = Unknown> {
      prop_oneof![
        unknown_object([
          ("claim", tag("law")),
          ("name", name(BUILTIN_LAW_NAMES)),
          ("seed", seed())
        ]),
        unknown_object([
          ("claim", tag("prop")),
          ("name", name(BUILTIN_PROP_NAMES)),
          ("seed", seed()),
          ("trials", (0..64i64).prop_map(Unknown::I64).boxed())
        ]),
      ]
    }

    proptest! {
      #![proptest_config(ProptestConfig::with_cases(64))]

      #[test]
      fn builtin_law_and_prop_claims_pass_for_any_seed(
        claim in from_schema(Claim::schema(), law_or_prop_wire())
      ) {
        let is_law_or_prop = matches!(claim, Claim::Law { .. } | Claim::Prop { .. });
        prop_assert!(is_law_or_prop, "decoded {:?}", claim);
        let verdict = prop_check(
          interpret_claim(claim)
            .provide_layer(fake_layer(MockNixEval::default()))
            .map_err(|e| TestCaseError::fail(format!("{e:?}"))),
        )?;
        prop_assert_eq!(verdict, CaseVerdict::Pass);
      }
    }
  }

  #[test]
  fn interpret_eq_bad_pair_length_returns_eq_pair_error() {
    let exit = run_claim_test(
      |eval| {
        eval.set("[(1) (2)]", EvalResult::Ok(serde_json::json!([1])));
      },
      Claim::Eq {
        left_expr: "1".into(),
        right_expr: "2".into(),
      },
    );
    match exit {
      Exit::Success(CaseVerdict::EvalThrow { kind, .. }) => {
        assert_eq!(kind, "eq_pair");
      }
      other => panic!("expected eq_pair eval throw, got {other:?}"),
    }
  }

  #[test]
  fn drv_values_passes_on_projected_subset() {
    let drv_json = serde_json::json!({
        "type": "derivation",
        "name": "hello-2.12.3",
        "outPath": "/nix/store/abc-hello-2.12.3",
        "drvPath": "/nix/store/xyz.drv"
    });
    let pass = run_claim_test(
      |_eval| {},
      Claim::DrvValues {
        actual: drv_json.clone(),
        project: vec!["name".into(), "outPath".into()],
        expected: serde_json::json!({"name": "hello-2.12.3"}),
      },
    );
    assert!(matches!(pass, Exit::Success(CaseVerdict::Pass)));

    let fail = run_claim_test(
      |_eval| {},
      Claim::DrvValues {
        actual: drv_json,
        project: vec!["name".into()],
        expected: serde_json::json!({"name": "wrong-name"}),
      },
    );
    assert!(matches!(
      fail,
      Exit::Success(CaseVerdict::AssertFail { .. })
    ));
  }

  #[test]
  fn drv_values_rejects_invalid_derivation_json() {
    let exit = run_claim_test(
      |_eval| {},
      Claim::DrvValues {
        actual: serde_json::json!("not-a-drv"),
        project: vec!["name".into()],
        expected: serde_json::json!({}),
      },
    );
    match exit {
      Exit::Success(CaseVerdict::EvalThrow { kind, .. }) => {
        assert_eq!(kind, "drv");
      }
      other => panic!("expected drv eval throw, got {other:?}"),
    }
  }

  #[test]
  fn drv_expr_mode_delegates_to_eval() {
    let drv_json = serde_json::json!({
        "type": "derivation",
        "name": "hello-2.12.3",
        "outPath": "/nix/store/abc-hello-2.12.3",
        "drvPath": "/nix/store/xyz.drv"
    });
    let exit = run_claim_test(
      |eval| eval.set("drv_expr", EvalResult::Ok(drv_json)),
      Claim::Drv {
        expr: "drv_expr".into(),
        project: vec!["name".into()],
        expected: serde_json::json!({"name": "hello-2.12.3"}),
      },
    );
    assert!(matches!(exit, Exit::Success(CaseVerdict::Pass)));
  }

  #[test]
  fn interpret_throws_propagates_non_throw_eval_error() {
    let exit = run_claim_test(
      |eval| {
        eval.set("expr", EvalResult::Err(AssayOutcome::Timeout));
      },
      Claim::Throws {
        expr: "expr".into(),
        pattern: None,
      },
    );
    assert!(matches!(exit, Exit::Failure(_)));
  }

  #[test]
  fn path_info_subset_pass_fail() {
    use nixstore::{MockPathInfoStore, PathInfo};
    let mock = Arc::new(MockPathInfoStore::default());
    mock.set(PathInfo {
      path: "/nix/store/x-a".into(),
      nar_hash: "sha256-AA".into(),
      nar_size: 42,
      deriver: None,
      registration_time: 1,
      ultimate: true,
      signatures: vec![],
      ca: None,
      references: vec!["/nix/store/y-b".into()],
      referrers: None,
      closure_size: None,
    });
    let mock: Arc<dyn nixstore::PathInfoStore> = mock;
    let query = |n: u64| {
      path_info_claim(
        "/nix/store/x-a".into(),
        serde_json::json!({ "narSize": n }),
        false,
        false,
      )
      .provide_layer(Layer::<PathInfoEnv, InfraError>::succeed(hlist![
        PathInfoService(Arc::clone(&mock))
      ]))
    };
    assert!(matches!(
      run_test(query(42)),
      Exit::Success(CaseVerdict::Pass)
    ));
    assert!(matches!(
      run_test(query(99)),
      Exit::Success(CaseVerdict::AssertFail { .. })
    ));
  }

  #[test]
  fn path_info_without_a_store_database_is_a_case_error() {
    let store: Arc<dyn nixstore::PathInfoStore> = Arc::new(crate::live::LazyPathInfoStore::new(
      "/nonexistent/assay-store",
    ));
    let claim = path_info_claim("/nix/store/x-a".into(), serde_json::json!({}), false, false)
      .provide_layer(Layer::<PathInfoEnv, InfraError>::succeed(hlist![
        PathInfoService(store)
      ]));
    match run_test(claim) {
      Exit::Success(CaseVerdict::EvalThrow { kind, .. }) => assert_eq!(kind, "pathInfo"),
      other => panic!("expected a pathInfo eval error, got {other:?}"),
    }
  }

  #[test]
  fn eval_claim_needs_only_nix_eval() {
    let mock = MockNixEval::default();
    mock.set("1", EvalResult::Ok(serde_json::json!(1)));
    let layer = Layer::<EvalEnv, InfraError>::succeed(hlist![NixEvalService(Arc::new(mock))]);
    let claim = Claim::Eq {
      left_expr: "1".into(),
      right_expr: "1".into(),
    };
    assert!(matches!(
      run_test(eval_claim(claim).provide_layer(layer)),
      Exit::Success(CaseVerdict::Pass)
    ));
  }
}
