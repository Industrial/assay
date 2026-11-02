//! Suite-/repo-level batched Nix evaluation via `builtins.tryEval`.
//!
//! Process spawn dominates tiny claim exprs. Batching all batchable claims into
//! one `nix eval` collapses O(cases) processes to O(1) while preserving throw
//! isolation for pattern-matched `throws` (those stay process-isolated).

use serde_json::Value;

use std::sync::Arc;

use crate::claims::{Claim, ClaimCaps, interpret_with};
use crate::eval::NixEval;
use crate::outcome::AssayOutcome;
use crate::run::Trace;
use crate::snapshot::SnapshotStore;
use crate::verdict::{CaseVerdict, InfraError, outcome_to_exit};
use id_effect::{Concurrency, Effect, Exit, all, fail, succeed};
use nixq::{normalize_value, structural_diff, value_contains_subset};
use nixstore::edge::from_result;

/// Marker embedded in generated Nix so mocks / debuggers can recognize batches.
pub const BATCH_MARKER: &str = "__assayBatch";

/// Whether `claim` can run inside a tryEval mega-batch.
pub fn is_batchable(claim: &Claim) -> bool {
  match claim {
    // Eq/subset/hasAttrs/snapshot — values are JSON-serializable on the happy path.
    Claim::Eq { .. } | Claim::Subset { .. } | Claim::HasAttrs { .. } | Claim::Snapshot { .. } => {
      true
    }
    // All throws stay process-isolated: Nix 2.31+ `tryEval` catches `throw`/`assert`
    // but NOT primop type errors (e.g. `builtins.add "x" 1`), and pattern throws need stderr.
    Claim::EqValues { .. }
    | Claim::SubsetValues { .. }
    | Claim::HasAttrsValues { .. }
    | Claim::Drv { .. }
    | Claim::DrvValues { .. }
    | Claim::Throws { .. }
    | Claim::Forces { .. }
    | Claim::Module { .. }
    | Claim::Law { .. }
    | Claim::Prop { .. }
    | Claim::PathInfo { .. } => false,
  }
}

#[derive(Debug, Clone)]
enum BatchKind {
  Eq,
  Subset { expected: Value },
  HasAttrs { attrs: Vec<String> },
  Snapshot { snap_name: String },
}

#[derive(Debug, Clone)]
struct BatchSlot {
  name: String,
  kind: BatchKind,
  primary_expr: String,
  secondary_expr: Option<String>,
}

type NamedClaim = (String, Claim);
type NamedExit = (String, Exit<CaseVerdict, InfraError>);

/// Partition cases into batchable slots and claims that must stay isolated.
pub fn partition_cases(cases: Vec<NamedClaim>) -> (Vec<NamedClaim>, Vec<NamedClaim>) {
  let mut batchable = Vec::new();
  let mut isolated = Vec::new();
  for (name, claim) in cases {
    if is_batchable(&claim) {
      batchable.push((name, claim));
    } else {
      isolated.push((name, claim));
    }
  }
  (batchable, isolated)
}

/// The batch slot of a [batchable](is_batchable) claim (`None` for any other).
fn to_slot(name: String, claim: Claim) -> Option<BatchSlot> {
  Some(match claim {
    Claim::Eq {
      left_expr,
      right_expr,
    } => BatchSlot {
      name,
      kind: BatchKind::Eq,
      primary_expr: left_expr,
      secondary_expr: Some(right_expr),
    },
    Claim::Subset {
      expr,
      expected_subset,
    } => BatchSlot {
      name,
      kind: BatchKind::Subset {
        expected: expected_subset,
      },
      primary_expr: expr,
      secondary_expr: None,
    },
    Claim::HasAttrs { expr, attrs } => BatchSlot {
      name,
      kind: BatchKind::HasAttrs { attrs },
      primary_expr: expr,
      secondary_expr: None,
    },
    Claim::Snapshot {
      name: snap_name,
      expr,
    } => BatchSlot {
      name,
      kind: BatchKind::Snapshot { snap_name },
      primary_expr: expr,
      secondary_expr: None,
    },
    _ => return None,
  })
}

fn wrap_nix(expr: &str) -> String {
  // Raw embed inside a generated .nix file (not `--expr`): multiline/quotes work.
  // tryEval catches `throw`/`assert`. Avoid builtins.toJSON: it rejects strings that
  // refer to store paths (IFC), while `nix eval --impure --json` can serialize them.
  // Primop type errors still abort the mega-batch → per-claim fallback in `run_batch`.
  format!(
    "(let r = builtins.tryEval ({expr}); in {{ ok = r.success; value = if r.success then r.value else null; }})"
  )
}

/// Project attr names only — avoids serializing huge module configs in the batch.
fn wrap_nix_attr_names(expr: &str) -> String {
  format!(
    "(let r = builtins.tryEval ({expr}); in if r.success then {{ ok = true; keys = builtins.attrNames r.value; }} else {{ ok = false; keys = []; }})"
  )
}

/// Build a single Nix expression that evaluates every batchable claim.
fn build_batch_expr(cases: &[(String, Claim)]) -> (String, Vec<BatchSlot>) {
  // Callers pass batchable claims only (`partition_cases`); others have no slot.
  let slots: Vec<BatchSlot> = cases
    .iter()
    .cloned()
    .filter_map(|(n, c)| to_slot(n, c))
    .collect();

  let mut items = Vec::with_capacity(slots.len());
  for (idx, slot) in slots.iter().enumerate() {
    let name_lit = nix_string_literal(&slot.name);
    let kind_lit = match &slot.kind {
      BatchKind::Eq => "eq",
      BatchKind::Subset { .. } => "subset",
      BatchKind::HasAttrs { .. } => "hasAttrs",
      BatchKind::Snapshot { .. } => "snapshot",
    };
    let body = match &slot.kind {
      BatchKind::Eq => {
        let left = wrap_nix(&slot.primary_expr);
        // `to_slot` always sets the right side of an eq slot.
        let right = wrap_nix(slot.secondary_expr.as_deref().unwrap_or("null"));
        format!(
          "{{ i = {idx}; name = {name_lit}; kind = \"{kind_lit}\"; left = {left}; right = {right}; }}"
        )
      }
      BatchKind::HasAttrs { .. } => {
        let primary = wrap_nix_attr_names(&slot.primary_expr);
        format!("{{ i = {idx}; name = {name_lit}; kind = \"{kind_lit}\"; primary = {primary}; }}")
      }
      BatchKind::Subset { .. } | BatchKind::Snapshot { .. } => {
        let primary = wrap_nix(&slot.primary_expr);
        format!("{{ i = {idx}; name = {name_lit}; kind = \"{kind_lit}\"; primary = {primary}; }}")
      }
    };
    items.push(body);
  }

  let expr = format!(
    "({{ {BATCH_MARKER} = true; results = [\n{}\n]; }})",
    items.join("\n")
  );
  (expr, slots)
}

fn nix_string_literal(s: &str) -> String {
  let escaped = s
    .replace('\\', "\\\\")
    .replace('"', "\\\"")
    .replace('$', "\\$");
  format!("\"{escaped}\"")
}

fn decode_try(node: &Value) -> Result<Option<Value>, AssayOutcome> {
  let obj = node.as_object().ok_or_else(|| AssayOutcome::EvalError {
    kind: "batch".into(),
    message: format!("tryEval wrapper not an object: {node}"),
    span: None,
  })?;
  match obj.get("ok").and_then(Value::as_bool) {
    Some(true) => Ok(Some(normalize_value(
      obj.get("value").unwrap_or(&Value::Null),
    ))),
    Some(false) => Ok(None),
    None => Err(AssayOutcome::EvalError {
      kind: "batch".into(),
      message: format!("tryEval wrapper missing ok: {node}"),
      span: None,
    }),
  }
}

fn slot_outcome(slot: &BatchSlot, row: &Value, store: &SnapshotStore) -> AssayOutcome {
  let kind = row.get("kind").and_then(Value::as_str).unwrap_or("");
  match (&slot.kind, kind) {
    (BatchKind::Eq, "eq") => {
      let left = match decode_try(row.get("left").unwrap_or(&Value::Null)) {
        Ok(Some(v)) => v,
        Ok(None) => {
          return AssayOutcome::EvalError {
            kind: "throw".into(),
            message: "left side of eq failed to evaluate".into(),
            span: None,
          };
        }
        Err(e) => return e,
      };
      let right = match decode_try(row.get("right").unwrap_or(&Value::Null)) {
        Ok(Some(v)) => v,
        Ok(None) => {
          return AssayOutcome::EvalError {
            kind: "throw".into(),
            message: "right side of eq failed to evaluate".into(),
            span: None,
          };
        }
        Err(e) => return e,
      };
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
    (BatchKind::Subset { expected }, "subset") => {
      match decode_try(row.get("primary").unwrap_or(&Value::Null)) {
        Ok(Some(actual)) => {
          if value_contains_subset(&actual, expected) {
            AssayOutcome::Pass
          } else {
            AssayOutcome::Fail {
              claim: "subset".into(),
              left: Some(actual.clone()),
              right: Some(expected.clone()),
              diff: structural_diff(&actual, expected),
            }
          }
        }
        Ok(None) => AssayOutcome::EvalError {
          kind: "throw".into(),
          message: "subset expr failed to evaluate".into(),
          span: None,
        },
        Err(e) => e,
      }
    }
    (BatchKind::HasAttrs { attrs }, "hasAttrs") => {
      let primary = row.get("primary").unwrap_or(&Value::Null);
      let ok = primary.get("ok").and_then(Value::as_bool);
      match ok {
        Some(true) => {
          let keys: Vec<String> = primary
            .get("keys")
            .and_then(Value::as_array)
            .map(|arr| {
              arr
                .iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
            })
            .unwrap_or_default();
          let missing: Vec<&str> = attrs
            .iter()
            .filter(|a| !keys.iter().any(|k| k == *a))
            .map(String::as_str)
            .collect();
          if missing.is_empty() {
            AssayOutcome::Pass
          } else {
            AssayOutcome::Fail {
              claim: "hasAttrs".into(),
              left: Some(Value::Array(keys.into_iter().map(Value::String).collect())),
              right: None,
              diff: format!("missing attrs: {missing:?}"),
            }
          }
        }
        Some(false) => AssayOutcome::EvalError {
          kind: "throw".into(),
          message: "hasAttrs expr failed to evaluate".into(),
          span: None,
        },
        None => AssayOutcome::EvalError {
          kind: "batch".into(),
          message: format!("hasAttrs wrapper missing ok: {primary}"),
          span: None,
        },
      }
    }
    (BatchKind::Snapshot { snap_name }, "snapshot") => {
      match decode_try(row.get("primary").unwrap_or(&Value::Null)) {
        Ok(Some(actual)) => store.assert_match(snap_name, &actual, store.update_snapshots),
        Ok(None) => AssayOutcome::EvalError {
          kind: "throw".into(),
          message: "snapshot expr failed to evaluate".into(),
          span: None,
        },
        Err(e) => e,
      }
    }
    _ => AssayOutcome::EvalError {
      kind: "batch".into(),
      message: format!(
        "batch row kind mismatch for {}: slot={:?} row={kind}",
        slot.name, slot.kind
      ),
      span: None,
    },
  }
}

/// Evaluates the mega-batch `expr`: from a file first (mega-batches easily exceed exec
/// `ARG_MAX` via `--expr`), then as `--expr`; fails with both errors.
fn eval_batch_expr(eval: &Arc<dyn NixEval>, expr: &str) -> Effect<Value, InfraError> {
  let (retry, expr_again) = (Arc::clone(eval), expr.to_owned());
  eval.eval_source(expr).catch(move |first| {
    retry
      .eval_expr(&expr_again)
      .catch(move |second| fail(batch_failure(&first, &second)))
  })
}

fn batch_failure(first: &AssayOutcome, second: &AssayOutcome) -> InfraError {
  InfraError::Worker(match (first, second) {
    (AssayOutcome::EvalError { message, .. }, AssayOutcome::EvalError { message: m2, .. }) => {
      format!("batch eval failed: {message}; fallback: {m2}")
    }
    (AssayOutcome::EvalError { message, .. }, other) => {
      format!("batch eval failed: {message}; fallback: {other:?}")
    }
    (other, _) => format!("batch eval failed: {other:?}"),
  })
}

/// One isolated interpretation per claim (the `--no-batch` path), in input order.
fn run_batch_fallback(
  cases: Vec<NamedClaim>,
  caps: ClaimCaps,
) -> Effect<Vec<NamedExit>, InfraError> {
  let runs = cases.into_iter().map(move |(name, claim)| {
    // Prefer the isolated interpreter (same path as `--no-batch`) so IFC /
    // store-path edge cases that break mega-batch still pass per claim.
    interpret_with(&caps, claim)
      .exit::<InfraError>()
      .map(move |exit| (name, exit))
  });
  all(runs.collect::<Vec<_>>(), Concurrency::Sequential)
}

/// Evaluate all batchable claims in one nix process; return verdicts in input order. When the
/// batch itself fails (primop type errors escape `tryEval` on Nix 2.31+), every claim runs
/// isolated instead. `trace` receives progress notes (`ASSAY_TRACE`).
pub fn run_batch(
  cases: Vec<NamedClaim>,
  caps: ClaimCaps,
  trace: Trace,
) -> Effect<Vec<NamedExit>, InfraError> {
  if cases.is_empty() {
    return succeed(Vec::new());
  }
  let (expr, slots) = build_batch_expr(&cases);
  trace.note(format!(
    "build_batch ({} slots, {} bytes)",
    slots.len(),
    expr.len()
  ));
  let store = caps.snapshots.clone();
  let on_fail = trace.clone();
  eval_batch_expr(&caps.eval, &expr)
    .map(Ok)
    .catch(move |batch_err| succeed(Err(batch_err)))
    .flat_map(move |evaluated| match evaluated {
      Ok(value) => {
        trace.note("eval_batch".into());
        from_result(batch_results(&value, &slots, &store))
      }
      Err(_batch_err) => {
        on_fail.note("eval_batch FAIL -> fallback".into());
        run_batch_fallback(cases, caps)
      }
    })
}

/// The verdict of every slot from the batch's `results` array.
fn batch_results(
  value: &Value,
  slots: &[BatchSlot],
  store: &SnapshotStore,
) -> Result<Vec<NamedExit>, InfraError> {
  let results = value
    .get("results")
    .and_then(Value::as_array)
    .ok_or_else(|| {
      InfraError::Worker(format!(
        "batch eval missing results array (marker {BATCH_MARKER})"
      ))
    })?;

  if results.len() != slots.len() {
    return Err(InfraError::Worker(format!(
      "batch result count {} != slot count {}",
      results.len(),
      slots.len()
    )));
  }

  Ok(
    slots
      .iter()
      .zip(results.iter())
      .map(|(slot, row)| {
        let outcome = slot_outcome(slot, row, store);
        (slot.name.clone(), outcome_to_exit(outcome))
      })
      .collect(),
  )
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::eval::EvalResult;
  use crate::live::LiveNixEval;
  use crate::snapshot::SnapshotStore;
  use id_effect::run_test;
  use std::path::PathBuf;

  /// [`run_batch`] over `eval` and `store`, run to completion.
  fn run_batch_with(
    cases: &[NamedClaim],
    eval: impl NixEval + 'static,
    store: &SnapshotStore,
  ) -> Result<Vec<NamedExit>, InfraError> {
    let caps = ClaimCaps {
      eval: Arc::new(eval),
      snapshots: store.clone(),
      path_info: Arc::new(nixstore::MockPathInfoStore::default()),
    };
    run_test(run_batch(cases.to_vec(), caps, Trace::default()))
      .into_result()
      .map_err(|c| match c {
        id_effect::Cause::Fail(e) => e,
        other => InfraError::Worker(format!("{other:?}")),
      })
  }

  #[test]
  fn partition_splits_pattern_throws() {
    let cases = vec![
      (
        "a".into(),
        Claim::Eq {
          left_expr: "1".into(),
          right_expr: "1".into(),
        },
      ),
      (
        "b".into(),
        Claim::Throws {
          expr: "builtins.throw \"x\"".into(),
          pattern: Some("x".into()),
        },
      ),
      (
        "c".into(),
        Claim::Throws {
          expr: "builtins.throw \"y\"".into(),
          pattern: None,
        },
      ),
    ];
    let (batch, iso) = partition_cases(cases);
    assert_eq!(batch.len(), 1);
    assert_eq!(iso.len(), 2);
    assert!(iso.iter().any(|(n, _)| n == "b"));
    assert!(iso.iter().any(|(n, _)| n == "c"));
  }

  #[test]
  fn build_batch_expr_contains_marker_and_names() {
    let cases = vec![(
      "add".into(),
      Claim::Eq {
        left_expr: "1 + 1".into(),
        right_expr: "2".into(),
      },
    )];
    let (expr, slots) = build_batch_expr(&cases);
    assert!(expr.contains(BATCH_MARKER));
    assert!(expr.contains("tryEval"));
    assert!(!expr.contains("toJSON"));
    assert!(expr.contains("r.success"));
    assert_eq!(slots.len(), 1);
    assert_eq!(slots[0].name, "add");
  }

  use crate::fake::{MockNixEval, SyncEval};
  use serde_json::json;

  struct MockBatchJsonEval {
    inner: MockNixEval,
    results: Value,
  }

  impl SyncEval for MockBatchJsonEval {
    fn eval_json(&self, expr: &str) -> EvalResult {
      if expr.contains(BATCH_MARKER) {
        return EvalResult::Ok(self.results.clone());
      }
      self.inner.eval_json(expr)
    }
  }

  fn sample_slot(name: &str, kind: BatchKind) -> BatchSlot {
    BatchSlot {
      name: name.into(),
      kind,
      primary_expr: "p".into(),
      secondary_expr: None,
    }
  }

  #[test]
  fn run_batch_empty_returns_empty() {
    let store = SnapshotStore::new(PathBuf::from("/tmp/assay-batch-empty"));
    let outs = run_batch_with(&[], MockNixEval::default(), &store).expect("empty");
    assert!(outs.is_empty());
  }

  #[test]
  fn decode_try_ok_and_throw_paths() {
    let ok = decode_try(&json!({"ok": true, "value": 1})).expect("decode");
    assert_eq!(ok, Some(json!(1)));
    let throw = decode_try(&json!({"ok": false})).expect("decode");
    assert_eq!(throw, None);
    assert!(decode_try(&json!("nope")).is_err());
    assert!(decode_try(&json!({})).is_err());
  }

  #[test]
  fn slot_outcome_eq_fail_and_throw_paths() {
    let store = SnapshotStore::new(PathBuf::from("/tmp/assay-slot"));
    let slot = sample_slot("eq", BatchKind::Eq);
    let fail_row = json!({
        "kind": "eq",
        "left": {"ok": true, "value": 1},
        "right": {"ok": true, "value": 2}
    });
    assert!(matches!(
      slot_outcome(&slot, &fail_row, &store),
      AssayOutcome::Fail { .. }
    ));

    let left_throw =
      json!({"kind": "eq", "left": {"ok": false}, "right": {"ok": true, "value": 1}});
    assert!(matches!(
      slot_outcome(&slot, &left_throw, &store),
      AssayOutcome::EvalError { .. }
    ));

    let mismatch = json!({"kind": "subset", "primary": {"ok": true, "value": {}}});
    assert!(matches!(
      slot_outcome(&slot, &mismatch, &store),
      AssayOutcome::EvalError { .. }
    ));
  }

  #[test]
  fn slot_outcome_subset_hasattrs_snapshot_paths() {
    let store = SnapshotStore::new(PathBuf::from("/tmp/assay-slot2"));
    let subset = sample_slot(
      "s",
      BatchKind::Subset {
        expected: json!({"a": 1}),
      },
    );
    let subset_ok = json!({"kind": "subset", "primary": {"ok": true, "value": {"a": 1, "b": 2}}});
    assert_eq!(
      slot_outcome(&subset, &subset_ok, &store),
      AssayOutcome::Pass
    );

    let subset_fail = json!({"kind": "subset", "primary": {"ok": true, "value": {"a": 9}}});
    assert!(matches!(
      slot_outcome(&subset, &subset_fail, &store),
      AssayOutcome::Fail { .. }
    ));

    let has = sample_slot(
      "h",
      BatchKind::HasAttrs {
        attrs: vec!["a".into()],
      },
    );
    let has_ok = json!({"kind": "hasAttrs", "primary": {"ok": true, "keys": ["a"]}});
    assert_eq!(slot_outcome(&has, &has_ok, &store), AssayOutcome::Pass);
    let has_missing = json!({"kind": "hasAttrs", "primary": {"ok": true, "keys": []}});
    assert!(matches!(
      slot_outcome(&has, &has_missing, &store),
      AssayOutcome::Fail { .. }
    ));

    let snap = sample_slot(
      "snap",
      BatchKind::Snapshot {
        snap_name: "__missing__".into(),
      },
    );
    let snap_row = json!({"kind": "snapshot", "primary": {"ok": true, "value": {"x": 1}}});
    assert!(matches!(
      slot_outcome(&snap, &snap_row, &store),
      AssayOutcome::SnapshotMismatch { .. }
    ));
  }

  #[test]
  fn build_batch_expr_covers_all_kinds() {
    let cases = vec![
      (
        "eq".into(),
        Claim::Eq {
          left_expr: "1".into(),
          right_expr: "2".into(),
        },
      ),
      (
        "sub".into(),
        Claim::Subset {
          expr: "v".into(),
          expected_subset: json!({}),
        },
      ),
      (
        "has".into(),
        Claim::HasAttrs {
          expr: "v".into(),
          attrs: vec!["a".into()],
        },
      ),
      (
        "snap".into(),
        Claim::Snapshot {
          name: "g".into(),
          expr: "v".into(),
        },
      ),
    ];
    let (expr, slots) = build_batch_expr(&cases);
    assert!(expr.contains("subset"));
    assert!(expr.contains("hasAttrs"));
    assert!(expr.contains("attrNames"));
    assert_eq!(slots.len(), 4);
    assert!(nix_string_literal(r#"a"b\$"#).contains("\\"));
  }

  #[test]
  fn run_batch_mock_json_paths() {
    let cases = vec![
      (
        "eq_ok".into(),
        Claim::Eq {
          left_expr: "1".into(),
          right_expr: "1".into(),
        },
      ),
      (
        "eq_fail".into(),
        Claim::Eq {
          left_expr: "1".into(),
          right_expr: "2".into(),
        },
      ),
    ];
    let results = json!({
        BATCH_MARKER: true,
        "results": [
            {"kind": "eq", "left": {"ok": true, "value": 1}, "right": {"ok": true, "value": 1}},
            {"kind": "eq", "left": {"ok": true, "value": 1}, "right": {"ok": true, "value": 2}},
        ]
    });
    let eval = MockBatchJsonEval {
      inner: MockNixEval::default(),
      results,
    };
    let store = SnapshotStore::new(PathBuf::from("/tmp/assay-batch-mock"));
    let outs = run_batch_with(&cases, eval, &store).expect("batch");
    assert_eq!(outs.len(), 2);
    assert!(matches!(outs[0].1, Exit::Success(CaseVerdict::Pass)));
    assert!(matches!(
      outs[1].1,
      Exit::Success(CaseVerdict::AssertFail { .. })
    ));
  }

  struct FailingBatchEval(MockNixEval);

  impl SyncEval for FailingBatchEval {
    fn eval_json(&self, expr: &str) -> EvalResult {
      if expr.contains(BATCH_MARKER) {
        return EvalResult::Err(AssayOutcome::EvalError {
          kind: "batch".into(),
          message: "forced batch failure".into(),
          span: None,
        });
      }
      self.0.eval_json(expr)
    }
  }

  #[test]
  fn run_batch_fallback_runs_isolated_interpreter() {
    let mock = MockNixEval::default();
    mock.set("a", EvalResult::Ok(json!(1)));
    let cases = vec![(
      "eq".into(),
      Claim::Eq {
        left_expr: "a".into(),
        right_expr: "a".into(),
      },
    )];
    let store = SnapshotStore::new(PathBuf::from("/tmp/assay-batch-fallback"));
    let outs = run_batch_with(&cases, FailingBatchEval(mock), &store).expect("fallback");
    assert!(matches!(outs[0].1, Exit::Success(CaseVerdict::Pass)));
  }

  #[test]
  fn is_batchable_covers_non_batchable_variants() {
    assert!(!is_batchable(&Claim::EqValues {
      left: json!(1),
      right: json!(1)
    }));
    assert!(!is_batchable(&Claim::Forces {
      expr: "x".into(),
      paths: vec![]
    }));
    assert!(!is_batchable(&Claim::Module {
      imports_expr: "[]".into(),
      args_expr: "{}".into(),
      expect: json!({})
    }));
    assert!(!is_batchable(&Claim::Prop {
      name: "always_pass".into(),
      seed: 1,
      trials: None
    }));
  }

  #[test]
  #[ignore = "requires nix in PATH"]
  fn live_batch_eq_and_bare_throws() {
    let cases = vec![
      (
        "ok".into(),
        Claim::Eq {
          left_expr: "1 + 1".into(),
          right_expr: "2".into(),
        },
      ),
      (
        "multi".into(),
        Claim::Eq {
          left_expr: "let\n  x = \"foo\" + \"bar\";\nin x".into(),
          right_expr: "\"foobar\"".into(),
        },
      ),
      (
        "bad".into(),
        Claim::Eq {
          left_expr: "1".into(),
          right_expr: "2".into(),
        },
      ),
    ];
    let store = SnapshotStore::new(PathBuf::from("/tmp/assay-batch-test-goldens"));
    let outs = run_batch_with(&cases, LiveNixEval, &store).expect("batch");
    assert_eq!(outs.len(), 3);
    assert!(matches!(outs[0].1, Exit::Success(CaseVerdict::Pass)));
    assert!(matches!(outs[1].1, Exit::Success(CaseVerdict::Pass)));
    assert!(matches!(
      outs[2].1,
      Exit::Success(CaseVerdict::AssertFail { .. })
    ));
  }

  #[test]
  fn slot_outcome_eq_right_throw_and_subset_decode_err() {
    let store = SnapshotStore::new(PathBuf::from("/tmp/assay-slot-extra"));
    let eq = sample_slot("eq", BatchKind::Eq);
    let right_throw = json!({
        "kind": "eq",
        "left": {"ok": true, "value": 1},
        "right": {"ok": false}
    });
    assert!(matches!(
      slot_outcome(&eq, &right_throw, &store),
      AssayOutcome::EvalError { .. }
    ));

    let subset = sample_slot(
      "s",
      BatchKind::Subset {
        expected: json!({"a": 1}),
      },
    );
    let bad_decode = json!({"kind": "subset", "primary": "nope"});
    assert!(matches!(
      slot_outcome(&subset, &bad_decode, &store),
      AssayOutcome::EvalError { .. }
    ));

    let subset_throw = json!({"kind": "subset", "primary": {"ok": false}});
    assert!(matches!(
      slot_outcome(&subset, &subset_throw, &store),
      AssayOutcome::EvalError { .. }
    ));

    let has = sample_slot(
      "h",
      BatchKind::HasAttrs {
        attrs: vec!["a".into()],
      },
    );
    let has_false = json!({"kind": "hasAttrs", "primary": {"ok": false}});
    assert!(matches!(
      slot_outcome(&has, &has_false, &store),
      AssayOutcome::EvalError { .. }
    ));
    let has_none = json!({"kind": "hasAttrs", "primary": {"oops": true}});
    assert!(matches!(
      slot_outcome(&has, &has_none, &store),
      AssayOutcome::EvalError { .. }
    ));

    let snap = sample_slot(
      "snap",
      BatchKind::Snapshot {
        snap_name: "g".into(),
      },
    );
    let snap_throw = json!({"kind": "snapshot", "primary": {"ok": false}});
    assert!(matches!(
      slot_outcome(&snap, &snap_throw, &store),
      AssayOutcome::EvalError { .. }
    ));
  }

  /// Answers batch sources with `source`, batch `--expr`s with `expr`, others via `inner`.
  struct ScriptedBatch {
    inner: MockNixEval,
    source: EvalResult,
    expr: EvalResult,
  }

  impl SyncEval for ScriptedBatch {
    fn eval_json(&self, expr: &str) -> EvalResult {
      if expr.contains(BATCH_MARKER) {
        return self.expr.clone();
      }
      self.inner.eval_json(expr)
    }
    fn eval_source_json(&self, source: &str) -> EvalResult {
      if source.contains(BATCH_MARKER) {
        return self.source.clone();
      }
      self.inner.eval_json(source)
    }
  }

  fn eval_err(kind: &str, message: &str) -> EvalResult {
    EvalResult::Err(AssayOutcome::EvalError {
      kind: kind.into(),
      message: message.into(),
      span: None,
    })
  }

  fn one_eq() -> Vec<NamedClaim> {
    vec![(
      "eq".into(),
      Claim::Eq {
        left_expr: "1".into(),
        right_expr: "1".into(),
      },
    )]
  }

  fn one_row() -> Value {
    json!({ BATCH_MARKER: true, "results": [
      {"kind": "eq", "left": {"ok": true, "value": 1}, "right": {"ok": true, "value": 1}}
    ]})
  }

  #[test]
  fn malformed_batch_results_fail_the_batch() {
    let store = SnapshotStore::new(PathBuf::from("/tmp/assay-batch-missing-results"));
    let missing = ScriptedBatch {
      inner: MockNixEval::default(),
      source: EvalResult::Ok(json!({ BATCH_MARKER: true })),
      expr: eval_err("throw", "unused"),
    };
    let err = run_batch_with(&one_eq(), missing, &store).unwrap_err();
    assert!(matches!(err, InfraError::Worker(m) if m.contains("missing results")));
    let mismatch = ScriptedBatch {
      inner: MockNixEval::default(),
      source: EvalResult::Ok(json!({ BATCH_MARKER: true, "results": [] })),
      expr: eval_err("throw", "unused"),
    };
    let err = run_batch_with(&one_eq(), mismatch, &store).unwrap_err();
    assert!(matches!(err, InfraError::Worker(m) if m.contains("result count")));
  }

  #[test]
  fn failed_batch_file_eval_retries_as_expr() {
    let store = SnapshotStore::new(PathBuf::from("/tmp/assay-file-fallback-ok"));
    let eval = ScriptedBatch {
      inner: MockNixEval::default(),
      source: eval_err("throw", "file eval failed"),
      expr: EvalResult::Ok(one_row()),
    };
    let outs = run_batch_with(&one_eq(), eval, &store).expect("batch");
    assert!(matches!(outs[0].1, Exit::Success(CaseVerdict::Pass)));
  }

  #[test]
  fn failed_batch_runs_every_claim_isolated() {
    let store = SnapshotStore::new(PathBuf::from("/tmp/assay-dual-fail"));
    for (source, expr) in [
      (eval_err("throw", "a"), eval_err("throw", "b")),
      (
        EvalResult::Err(AssayOutcome::Recursion),
        eval_err("io", "b"),
      ),
    ] {
      let eval = ScriptedBatch {
        inner: MockNixEval::default(),
        source,
        expr,
      };
      // The isolated eq `[(1) (1)]` evaluates to `[null null]` on the mock: equal, so a pass.
      let outs = run_batch_with(&one_eq(), eval, &store).expect("fallback");
      assert!(matches!(outs[0].1, Exit::Success(CaseVerdict::Pass)));
    }
  }

  #[test]
  fn batch_failure_messages_name_both_errors() {
    let throw = |m: &str| AssayOutcome::EvalError {
      kind: "throw".into(),
      message: m.into(),
      span: None,
    };
    let both = batch_failure(&throw("a"), &throw("b"));
    assert_eq!(
      both,
      InfraError::Worker("batch eval failed: a; fallback: b".into())
    );
    let rec = batch_failure(&AssayOutcome::Recursion, &throw("b"));
    assert!(matches!(rec, InfraError::Worker(m) if m.contains("Recursion")));
  }

  #[test]
  fn batch_trace_notes_build_and_eval() {
    let notes = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let sink = Arc::clone(&notes);
    let trace = Trace::to(move |n| sink.lock().unwrap().push(n.to_string()));
    let eval = ScriptedBatch {
      inner: MockNixEval::default(),
      source: EvalResult::Ok(one_row()),
      expr: eval_err("throw", "unused"),
    };
    let caps = ClaimCaps {
      eval: Arc::new(eval),
      snapshots: SnapshotStore::new(PathBuf::from("/tmp/assay-batch-trace")),
      path_info: Arc::new(nixstore::MockPathInfoStore::default()),
    };
    let _ = run_test(run_batch(one_eq(), caps, trace));
    let notes = notes.lock().unwrap();
    assert!(notes[0].starts_with("build_batch (1 slots"));
    assert_eq!(notes[1], "eval_batch");
  }
}
