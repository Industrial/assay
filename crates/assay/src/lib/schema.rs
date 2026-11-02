//! Suite and claim decoding via id_effect schemas.
//!
//! The suite JSON (`nix eval --json` of a `.assay.nix`, or a `.assay.json` / compat `.json` file)
//! is mirrored by `#[derive(Schema)]` wire structs, one per claim shape, and decoded with
//! `decode_unknown_all`: a malformed suite reports every issue with its path
//! (`cases.<case>.<field>: …`, see [`describe_issues`]).
//!
//! What the derive (structs with named fields over `String` / `bool` / numbers / `Option` /
//! `Vec` / other `HasSchema` types) cannot express is hand-written, and only that:
//! - the leaf codecs [`NixValue`] (any JSON value: a claim's `actual` / `expected` / `expect`;
//!   there is no `serde_json::Value` field type), [`NixSource`] (Nix source: a string as is, any
//!   other JSON rendered as a Nix literal), [`Seed`] (`u64`) and [`Trials`] (`u32`);
//! - the dispatch on the `claim` tag (and on `actual` for value vs expression mode) in
//!   [`HasSchema for Claim`](Claim): the derive has no enums / tagged unions;
//! - [`SuiteCases`], the `cases` map (`record` of claims; its wire is a map, not `Unknown`).
//!
//! Numbers: the serde bridge clamps integers above `i64::MAX`; Nix integers are `i64`, so no
//! suite value is affected (and decoding is unchanged from the hand-written codec).

use std::collections::BTreeMap;
use std::sync::Arc;

use id_effect::Schema;
use id_effect::schema::{
  HasSchema, ParseError, ParseErrors, Unknown, i64_unknown_wire, record, serde_json_from_unknown,
  transform, unknown_from_serde_json,
};
use serde_json::Value;

use crate::claims::Claim;
use crate::compat::{CompatCase, field_to_nix};

/// Every claim kind, for the "expected one of" message.
const CLAIM_KINDS: &str = "eq|throws|subset|hasAttrs|drv|snapshot|forces|module|law|prop|pathInfo";

// ── leaf codecs ─────────────────────────────────────────────────────────────────────────────

/// A schema over the `Unknown` wire from an accumulating decoder and an encoder; the fail-fast
/// decoders report its first issue.
fn unknown_codec<A: 'static>(
  decode_all: impl Fn(&Unknown) -> Result<A, ParseErrors> + Send + Sync + 'static,
  encode: impl Fn(A) -> Unknown + Send + Sync + 'static,
) -> Schema<A, Unknown> {
  let decode_all = Arc::new(decode_all);
  let (owned, borrowed) = (Arc::clone(&decode_all), Arc::clone(&decode_all));
  Schema::make(
    move |u: Unknown| first_issue(owned(&u)),
    encode,
    move |u: &Unknown| first_issue(borrowed(u)),
  )
  .with_decode_unknown_all(move |u| decode_all(u))
}

fn first_issue<A>(decoded: Result<A, ParseErrors>) -> Result<A, ParseError> {
  decoded.map_err(|e| {
    e.issues
      .into_iter()
      .next()
      .unwrap_or_else(|| ParseError::new("", "invalid value"))
  })
}

/// Any JSON value, kept as is.
#[derive(Debug, Clone, PartialEq)]
pub struct NixValue(pub Value);

impl HasSchema for NixValue {
  type A = Self;
  type I = Unknown;
  type E = ();

  fn schema() -> Schema<Self, Unknown> {
    unknown_codec(
      |u| Ok(NixValue(serde_json_from_unknown(u))),
      |v: NixValue| unknown_from_serde_json(v.0),
    )
  }
}

/// Nix source text: a JSON string is the source; any other JSON value is rendered as the Nix
/// literal of that value (see [`field_to_nix`]).
#[derive(Debug, Clone, PartialEq)]
pub struct NixSource(pub String);

impl HasSchema for NixSource {
  type A = Self;
  type I = Unknown;
  type E = ();

  fn schema() -> Schema<Self, Unknown> {
    transform(
      NixValue::schema(),
      |v: NixValue| Ok(NixSource(field_to_nix(&v.0))),
      |s: NixSource| NixValue(Value::String(s.0)),
    )
  }
}

/// A non-negative integer seed (encoded saturating at `i64::MAX`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Seed(pub u64);

impl HasSchema for Seed {
  type A = Self;
  type I = Unknown;
  type E = ();

  fn schema() -> Schema<Self, Unknown> {
    transform(
      i64_unknown_wire(),
      |n: i64| {
        u64::try_from(n)
          .map(Seed)
          .map_err(|_| ParseError::new("", "expected a non-negative integer"))
      },
      |s: Seed| i64::try_from(s.0).unwrap_or(i64::MAX),
    )
  }
}

/// A property trial count (`u32`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Trials(pub u32);

impl HasSchema for Trials {
  type A = Self;
  type I = Unknown;
  type E = ();

  fn schema() -> Schema<Self, Unknown> {
    transform(
      i64_unknown_wire(),
      |n: i64| {
        u32::try_from(n)
          .map(Trials)
          .map_err(|_| ParseError::new("", format!("expected an integer in 0..={}", u32::MAX)))
      },
      |t: Trials| i64::from(t.0),
    )
  }
}

// ── claim wire shapes ───────────────────────────────────────────────────────────────────────

/// `eq` in expression mode.
#[derive(Debug, Clone, Schema)]
struct EqWire {
  expr: NixSource,
  expected: NixSource,
}

/// `eq` / `subset` in value mode (the suite evaluated both sides).
#[derive(Debug, Clone, Schema)]
struct ValuesWire {
  actual: NixValue,
  expected: NixValue,
}

#[derive(Debug, Clone, Schema)]
struct ThrowsWire {
  expr: NixSource,
  pattern: Option<NixSource>,
}

#[derive(Debug, Clone, Schema)]
struct SubsetWire {
  expr: NixSource,
  expected: NixValue,
}

#[derive(Debug, Clone, Schema)]
struct HasAttrsWire {
  expr: NixSource,
  attrs: Vec<String>,
}

#[derive(Debug, Clone, Schema)]
struct HasAttrsValuesWire {
  actual: NixValue,
  attrs: Vec<String>,
}

#[derive(Debug, Clone, Schema)]
struct DrvWire {
  expr: NixSource,
  project: Vec<String>,
  expected: NixValue,
}

#[derive(Debug, Clone, Schema)]
struct DrvValuesWire {
  actual: NixValue,
  project: Vec<String>,
  expected: NixValue,
}

#[derive(Debug, Clone, Schema)]
struct SnapshotWire {
  name: String,
  expr: NixSource,
}

#[derive(Debug, Clone, Schema)]
struct ForcesWire {
  expr: NixSource,
  paths: Vec<String>,
}

#[derive(Debug, Clone, Schema)]
struct ModuleWire {
  imports: NixSource,
  args: NixSource,
  expect: NixValue,
}

#[derive(Debug, Clone, Schema)]
struct LawWire {
  name: String,
  seed: Seed,
}

#[derive(Debug, Clone, Schema)]
struct PropWire {
  name: String,
  seed: Seed,
  trials: Option<Trials>,
}

/// `pathInfo`; the closure-size flag is read as `closureSize` or `closure_size`.
#[derive(Debug, Clone, Schema)]
struct PathInfoWire {
  path: String,
  expect: NixValue,
  referrers: Option<bool>,
  #[schema("closureSize")]
  closure_size: Option<bool>,
  #[schema("closure_size")]
  closure_size_snake: Option<bool>,
}

/// Decodes `input` as the wire shape `T`, collecting every issue.
fn wire<T>(input: &Unknown) -> Result<T, ParseErrors>
where
  T: HasSchema<A = T, I = Unknown, E = ()>,
{
  <T as HasSchema>::schema().decode_unknown_all(input)
}

fn issue(path: &str, message: impl Into<String>) -> ParseErrors {
  ParseErrors::one(ParseError::new(path, message))
}

fn decode_claim(input: &Unknown) -> Result<Claim, ParseErrors> {
  let Unknown::Object(obj) = input else {
    return Err(issue("", "expected a claim object"));
  };
  let tag = match obj.get("claim") {
    Some(Unknown::String(tag)) => tag.as_str(),
    Some(_) => {
      return Err(issue(
        "claim",
        format!("expected a string, one of {CLAIM_KINDS}"),
      ));
    }
    None => {
      return Err(issue(
        "claim",
        format!("missing field, one of {CLAIM_KINDS}"),
      ));
    }
  };
  // Value mode: the suite evaluated the subject itself (`actual`) instead of passing source.
  let values = obj.contains_key("actual");
  match tag {
    "eq" if values => wire::<ValuesWire>(input).map(|w| Claim::EqValues {
      left: w.actual.0,
      right: w.expected.0,
    }),
    "eq" => wire::<EqWire>(input).map(|w| Claim::Eq {
      left_expr: w.expr.0,
      right_expr: w.expected.0,
    }),
    "throws" => wire::<ThrowsWire>(input).map(|w| Claim::Throws {
      expr: w.expr.0,
      pattern: w.pattern.map(|p| p.0),
    }),
    "subset" if values => wire::<ValuesWire>(input).map(|w| Claim::SubsetValues {
      actual: w.actual.0,
      expected_subset: w.expected.0,
    }),
    "subset" => wire::<SubsetWire>(input).map(|w| Claim::Subset {
      expr: w.expr.0,
      expected_subset: w.expected.0,
    }),
    "hasAttrs" if values => wire::<HasAttrsValuesWire>(input).map(|w| Claim::HasAttrsValues {
      actual: w.actual.0,
      attrs: w.attrs,
    }),
    "hasAttrs" => wire::<HasAttrsWire>(input).map(|w| Claim::HasAttrs {
      expr: w.expr.0,
      attrs: w.attrs,
    }),
    "drv" if values => wire::<DrvValuesWire>(input).map(|w| Claim::DrvValues {
      actual: w.actual.0,
      project: w.project,
      expected: w.expected.0,
    }),
    "drv" => wire::<DrvWire>(input).map(|w| Claim::Drv {
      expr: w.expr.0,
      project: w.project,
      expected: w.expected.0,
    }),
    "snapshot" => wire::<SnapshotWire>(input).map(|w| Claim::Snapshot {
      name: w.name,
      expr: w.expr.0,
    }),
    "forces" => wire::<ForcesWire>(input).map(|w| Claim::Forces {
      expr: w.expr.0,
      paths: w.paths,
    }),
    "module" => wire::<ModuleWire>(input).map(|w| Claim::Module {
      imports_expr: w.imports.0,
      args_expr: w.args.0,
      expect: w.expect.0,
    }),
    "law" => wire::<LawWire>(input).map(|w| Claim::Law {
      name: w.name,
      seed: w.seed.0,
    }),
    "prop" => wire::<PropWire>(input).map(|w| Claim::Prop {
      name: w.name,
      seed: w.seed.0,
      trials: w.trials.map(|t| t.0),
    }),
    "pathInfo" => wire::<PathInfoWire>(input).map(|w| Claim::PathInfo {
      path: w.path,
      expect: w.expect.0,
      referrers: w.referrers.unwrap_or(false),
      closure_size: w.closure_size.or(w.closure_size_snake).unwrap_or(false),
    }),
    other => Err(issue(
      "claim",
      format!("unknown claim {other:?}, expected one of {CLAIM_KINDS}"),
    )),
  }
}

/// `wire` encoded with its `claim` tag; the `optional` keys are dropped when unset (`null`).
fn tagged<T>(tag: &str, wire: T, optional: &[&str]) -> Unknown
where
  T: HasSchema<A = T, I = Unknown, E = ()>,
{
  let mut fields = match <T as HasSchema>::schema().encode(wire) {
    Unknown::Object(fields) => fields,
    other => BTreeMap::from([("value".to_owned(), other)]),
  };
  fields.retain(|key, value| !(optional.contains(&key.as_str()) && *value == Unknown::Null));
  fields.insert("claim".into(), Unknown::String(tag.into()));
  Unknown::Object(fields)
}

fn encode_claim(claim: Claim) -> Unknown {
  let src = NixSource;
  let val = NixValue;
  match claim {
    Claim::Eq {
      left_expr,
      right_expr,
    } => tagged(
      "eq",
      EqWire {
        expr: src(left_expr),
        expected: src(right_expr),
      },
      &[],
    ),
    Claim::EqValues { left, right } => tagged(
      "eq",
      ValuesWire {
        actual: val(left),
        expected: val(right),
      },
      &[],
    ),
    Claim::Throws { expr, pattern } => tagged(
      "throws",
      ThrowsWire {
        expr: src(expr),
        pattern: pattern.map(src),
      },
      &["pattern"],
    ),
    Claim::Subset {
      expr,
      expected_subset,
    } => tagged(
      "subset",
      SubsetWire {
        expr: src(expr),
        expected: val(expected_subset),
      },
      &[],
    ),
    Claim::SubsetValues {
      actual,
      expected_subset,
    } => tagged(
      "subset",
      ValuesWire {
        actual: val(actual),
        expected: val(expected_subset),
      },
      &[],
    ),
    Claim::HasAttrs { expr, attrs } => tagged(
      "hasAttrs",
      HasAttrsWire {
        expr: src(expr),
        attrs,
      },
      &[],
    ),
    Claim::HasAttrsValues { actual, attrs } => tagged(
      "hasAttrs",
      HasAttrsValuesWire {
        actual: val(actual),
        attrs,
      },
      &[],
    ),
    Claim::Drv {
      expr,
      project,
      expected,
    } => tagged(
      "drv",
      DrvWire {
        expr: src(expr),
        project,
        expected: val(expected),
      },
      &[],
    ),
    Claim::DrvValues {
      actual,
      project,
      expected,
    } => tagged(
      "drv",
      DrvValuesWire {
        actual: val(actual),
        project,
        expected: val(expected),
      },
      &[],
    ),
    Claim::Snapshot { name, expr } => tagged(
      "snapshot",
      SnapshotWire {
        name,
        expr: src(expr),
      },
      &[],
    ),
    Claim::Forces { expr, paths } => tagged(
      "forces",
      ForcesWire {
        expr: src(expr),
        paths,
      },
      &[],
    ),
    Claim::Module {
      imports_expr,
      args_expr,
      expect,
    } => tagged(
      "module",
      ModuleWire {
        imports: src(imports_expr),
        args: src(args_expr),
        expect: val(expect),
      },
      &[],
    ),
    Claim::Law { name, seed } => tagged(
      "law",
      LawWire {
        name,
        seed: Seed(seed),
      },
      &[],
    ),
    Claim::Prop { name, seed, trials } => tagged(
      "prop",
      PropWire {
        name,
        seed: Seed(seed),
        trials: trials.map(Trials),
      },
      &["trials"],
    ),
    Claim::PathInfo {
      path,
      expect,
      referrers,
      closure_size,
    } => tagged(
      "pathInfo",
      PathInfoWire {
        path,
        expect: val(expect),
        referrers: referrers.then_some(true),
        closure_size: closure_size.then_some(true),
        closure_size_snake: None,
      },
      &["referrers", "closureSize", "closure_size"],
    ),
  }
}

impl HasSchema for Claim {
  type A = Claim;
  type I = Unknown;
  type E = ();

  fn schema() -> Schema<Self::A, Self::I, Self::E> {
    unknown_codec(decode_claim, encode_claim)
  }
}

// ── suites ──────────────────────────────────────────────────────────────────────────────────

/// A suite's `cases`: case name -> claim.
#[derive(Debug, Clone, PartialEq)]
pub struct SuiteCases(pub BTreeMap<String, Claim>);

impl HasSchema for SuiteCases {
  type A = Self;
  type I = Unknown;
  type E = ();

  fn schema() -> Schema<Self, Unknown> {
    let cases = record(Claim::schema());
    let ast = cases.ast().clone();
    unknown_codec(
      move |u| cases.decode_unknown_all(u).map(SuiteCases),
      |c: SuiteCases| Unknown::Object(c.0.into_iter().map(|(k, c)| (k, encode_claim(c))).collect()),
    )
    .with_ast(ast)
  }
}

/// An assay suite: `{ name, cases = { <case> = <claim>; }; }` (`name` and other keys unread).
#[derive(Debug, Clone, Schema)]
pub struct AssaySuite {
  pub cases: SuiteCases,
}

/// One nix-unit / `lib.runTests` case: `{ expr, expected }` (both Nix source).
#[derive(Debug, Clone, Schema)]
struct CompatCaseWire {
  expr: NixSource,
  expected: NixSource,
}

/// Decode a claim from a JSON value (fail-fast: the first issue).
pub fn decode_claim_json(value: &Value) -> Result<Claim, ParseError> {
  Claim::schema().decode_unknown(&unknown_from_serde_json(value.clone()))
}

/// Encode a claim to a JSON value.
pub fn encode_claim_json(claim: &Claim) -> Value {
  serde_json_from_unknown(&Claim::schema().encode(claim.clone()))
}

/// Decode an assay suite into its named claims (by case name), reporting every issue.
pub fn decode_suite(value: &Value) -> Result<Vec<(String, Claim)>, ParseErrors> {
  AssaySuite::schema()
    .decode_unknown_all(&unknown_from_serde_json(value.clone()))
    .map(|suite| suite.cases.0.into_iter().collect())
}

/// Decode a nix-unit / `lib.runTests` suite (`{ <case> = { expr; expected; }; }`), reporting
/// every issue.
pub fn decode_compat_cases(value: &Value) -> Result<Vec<CompatCase>, ParseErrors> {
  record(CompatCaseWire::schema())
    .decode_unknown_all(&unknown_from_serde_json(value.clone()))
    .map(|cases| {
      cases
        .into_iter()
        .map(|(name, case)| CompatCase {
          name,
          expr: case.expr.0,
          expected: case.expected.0,
        })
        .collect()
    })
}

/// `invalid suite, N error(s):` and one indented `path: message` line per issue.
pub fn describe_issues(errors: &ParseErrors) -> String {
  let n = errors.issues.len();
  let mut out = format!("invalid suite, {n} error{}:", if n == 1 { "" } else { "s" });
  for issue in &errors.issues {
    let path = if issue.path.is_empty() {
      "(suite)"
    } else {
      issue.path.as_str()
    };
    out.push_str(&format!("\n  {path}: {}", issue.message));
  }
  out
}

#[cfg(test)]
mod tests {
  use super::*;
  use serde_json::json;

  fn round_trip(claim: Claim) {
    let encoded = encode_claim_json(&claim);
    let decoded = decode_claim_json(&encoded).expect("decode round-trip");
    assert_eq!(decoded, claim);
    let all = Claim::schema()
      .decode_unknown_all(&unknown_from_serde_json(encoded))
      .expect("decode_unknown_all round-trip");
    assert_eq!(all, claim);
  }

  fn decode(value: Value) -> Claim {
    decode_claim_json(&value).expect("decode")
  }

  /// `path: message` of every issue.
  fn issues(errors: &ParseErrors) -> Vec<String> {
    errors
      .issues
      .iter()
      .map(|e| format!("{}: {}", e.path, e.message))
      .collect()
  }

  #[test]
  fn eq_claim_round_trips() {
    round_trip(Claim::Eq {
      left_expr: "1 + 1".into(),
      right_expr: "2".into(),
    });
  }

  #[test]
  fn eq_values_claim_round_trips() {
    round_trip(Claim::EqValues {
      left: json!(2),
      right: json!(2),
    });
    round_trip(Claim::EqValues {
      left: json!(null),
      right: json!({"a": [1, 2.5, "x"]}),
    });
  }

  #[test]
  fn throws_claim_round_trips_with_pattern() {
    round_trip(Claim::Throws {
      expr: "builtins.throw \"x\"".into(),
      pattern: Some("x".into()),
    });
  }

  #[test]
  fn throws_without_pattern_round_trips() {
    round_trip(Claim::Throws {
      expr: "builtins.throw \"x\"".into(),
      pattern: None,
    });
    let encoded = encode_claim_json(&Claim::Throws {
      expr: "x".into(),
      pattern: None,
    });
    assert!(encoded.get("pattern").is_none());
  }

  #[test]
  fn subset_claim_round_trips() {
    round_trip(Claim::Subset {
      expr: "{ a = 1; }".into(),
      expected_subset: json!({"a": 1}),
    });
    round_trip(Claim::SubsetValues {
      actual: json!({"a": 1}),
      expected_subset: json!({"a": 1}),
    });
  }

  #[test]
  fn has_attrs_claims_round_trip() {
    round_trip(Claim::HasAttrs {
      expr: "{ a = 1; }".into(),
      attrs: vec!["a".into()],
    });
    round_trip(Claim::HasAttrsValues {
      actual: json!({"a": 1}),
      attrs: vec!["a".into()],
    });
  }

  #[test]
  fn drv_claims_round_trip() {
    round_trip(Claim::Drv {
      expr: "pkgs.hello.drvPath".into(),
      project: vec!["name".into()],
      expected: json!({"name": "hello"}),
    });
    round_trip(Claim::DrvValues {
      actual: json!({
          "type": "derivation",
          "name": "hello-2.12.3",
          "outPath": "/nix/store/abc-hello-2.12.3"
      }),
      project: vec!["name".into(), "outPath".into()],
      expected: json!({"name": "hello-2.12.3"}),
    });
  }

  #[test]
  fn snapshot_forces_module_round_trip() {
    round_trip(Claim::Snapshot {
      name: "snap".into(),
      expr: "1".into(),
    });
    round_trip(Claim::Forces {
      expr: "x".into(),
      paths: vec!["p".into()],
    });
    round_trip(Claim::Module {
      imports_expr: "[]".into(),
      args_expr: "{}".into(),
      expect: json!({"ok": true}),
    });
  }

  #[test]
  fn law_and_prop_claims_round_trip() {
    round_trip(Claim::Law {
      name: "merge_idempotent".into(),
      seed: 42,
    });
    round_trip(Claim::Prop {
      name: "always_pass".into(),
      seed: 7,
      trials: Some(64),
    });
    round_trip(Claim::Prop {
      name: "always_pass".into(),
      seed: 8,
      trials: None,
    });
  }

  #[test]
  fn path_info_claim_round_trips() {
    round_trip(Claim::PathInfo {
      path: "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-a".into(),
      expect: json!({"narSize": 100}),
      referrers: true,
      closure_size: true,
    });
    let plain = Claim::PathInfo {
      path: "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-a".into(),
      expect: json!({}),
      referrers: false,
      closure_size: false,
    };
    round_trip(plain.clone());
    let encoded = encode_claim_json(&plain);
    assert!(encoded.get("referrers").is_none());
    assert!(encoded.get("closureSize").is_none());
    assert!(encoded.get("closure_size").is_none());
  }

  #[test]
  fn dsl_value_mode_claims_decode() {
    assert_eq!(
      decode(json!({"claim": "eq", "actual": 2, "expected": 2})),
      Claim::EqValues {
        left: json!(2),
        right: json!(2)
      }
    );
    assert_eq!(
      decode(json!({"claim": "subset", "actual": {"a": 1, "b": 2}, "expected": {"a": 1}})),
      Claim::SubsetValues {
        actual: json!({"a": 1, "b": 2}),
        expected_subset: json!({"a": 1}),
      }
    );
    assert_eq!(
      decode(json!({"claim": "hasAttrs", "actual": {"a": 1}, "attrs": ["a"]})),
      Claim::HasAttrsValues {
        actual: json!({"a": 1}),
        attrs: vec!["a".into()],
      }
    );
    assert!(matches!(
      decode(json!({"claim": "drv", "actual": {}, "project": [], "expected": {}})),
      Claim::DrvValues { .. }
    ));
  }

  #[test]
  fn json_literals_in_source_fields_become_nix_source() {
    assert_eq!(
      decode(json!({"claim": "eq", "expr": 42, "expected": [1, "a"]})),
      Claim::Eq {
        left_expr: "42".into(),
        right_expr: "[1 \"a\"]".into(),
      }
    );
    assert_eq!(
      decode(json!({"claim": "throws", "expr": "x", "pattern": null})),
      Claim::Throws {
        expr: "x".into(),
        pattern: None
      }
    );
  }

  #[test]
  fn path_info_reads_both_closure_size_spellings() {
    let snake = decode(json!({
        "claim": "pathInfo", "path": "/nix/store/x-a", "expect": {}, "closure_size": true
    }));
    assert!(matches!(
      snake,
      Claim::PathInfo {
        closure_size: true,
        referrers: false,
        ..
      }
    ));
    let camel = decode(json!({
        "claim": "pathInfo", "path": "/nix/store/x-a", "expect": {}, "closureSize": true,
        "referrers": true
    }));
    assert!(matches!(
      camel,
      Claim::PathInfo {
        closure_size: true,
        referrers: true,
        ..
      }
    ));
  }

  #[test]
  fn law_seed_max_encodes_as_i64_max() {
    let encoded = encode_claim_json(&Claim::Law {
      name: "merge_identity".into(),
      seed: u64::MAX,
    });
    assert_eq!(encoded["seed"], json!(i64::MAX));
  }

  #[test]
  fn claim_errors_name_the_field() {
    let err = |v: Value| {
      let e = decode_claim_json(&v).expect_err("invalid claim");
      format!("{}: {}", e.path, e.message)
    };
    assert_eq!(
      err(json!({"claim": "nope"})),
      format!("claim: unknown claim \"nope\", expected one of {CLAIM_KINDS}")
    );
    assert_eq!(
      err(json!({"claim": 1})),
      format!("claim: expected a string, one of {CLAIM_KINDS}")
    );
    assert_eq!(
      err(json!({"expr": "1"})),
      format!("claim: missing field, one of {CLAIM_KINDS}")
    );
    assert_eq!(err(json!("not-object")), ": expected a claim object");
    assert_eq!(err(json!({"claim": "eq"})), "expr: missing field");
    assert_eq!(
      err(json!({"claim": "law", "name": "x"})),
      "seed: missing field"
    );
    assert_eq!(
      err(json!({"claim": "prop", "name": "x", "seed": -1})),
      "seed: expected a non-negative integer"
    );
    assert_eq!(
      err(json!({"claim": "prop", "name": "x", "seed": 1, "trials": 9_999_999_999i64})),
      "trials: expected an integer in 0..=4294967295"
    );
    assert_eq!(
      err(json!({"claim": "prop", "name": "x", "seed": 1, "trials": "lots"})),
      "trials: expected i64"
    );
    assert_eq!(
      err(json!({"claim": "hasAttrs", "expr": "x", "attrs": [1]})),
      "attrs.0: expected string"
    );
    assert_eq!(
      err(json!({"claim": "subset", "actual": {}})),
      "expected: missing field"
    );
  }

  #[test]
  fn suite_decodes_cases_by_name() {
    let v = json!({
        "name": "fixture",
        "cases": {
            "law": { "claim": "law", "name": "merge_identity", "seed": 1 },
            "add": { "claim": "eq", "expr": "1 + 1", "expected": "2" }
        }
    });
    let cases = decode_suite(&v).expect("decode suite");
    let names: Vec<_> = cases.iter().map(|(n, _)| n.as_str()).collect();
    assert_eq!(names, ["add", "law"]);
    assert!(matches!(cases[0].1, Claim::Eq { .. }));
    assert!(matches!(cases[1].1, Claim::Law { .. }));
  }

  #[test]
  fn suite_shape_errors() {
    let errs = |v: Value| issues(&decode_suite(&v).expect_err("invalid suite"));
    assert_eq!(errs(json!([])), [": expected object"]);
    assert_eq!(errs(json!({"name": "t"})), ["cases: missing field"]);
    assert_eq!(
      errs(json!({"cases": []})),
      ["cases: expected object for record"]
    );
  }

  /// `tests/fixtures/malformed.assay.json`: every broken case is reported, with its path.
  #[test]
  fn malformed_suite_reports_every_error_with_its_path() {
    let fixture: Value =
      serde_json::from_str(include_str!("../../tests/fixtures/malformed.assay.json"))
        .expect("fixture is JSON");
    let errors = decode_suite(&fixture).expect_err("malformed suite");
    assert_eq!(
      issues(&errors),
      [
        "cases.badAttrs.attrs.1: expected string".to_owned(),
        format!("cases.badKind.claim: unknown claim \"equals\", expected one of {CLAIM_KINDS}"),
        "cases.badProp.name: missing field".to_owned(),
        "cases.badProp.seed: expected i64".to_owned(),
        "cases.badProp.trials: expected an integer in 0..=4294967295".to_owned(),
        "cases.badSeed.seed: expected a non-negative integer".to_owned(),
        "cases.noExpected.expected: missing field".to_owned(),
      ]
    );
    let described = describe_issues(&errors);
    assert!(described.starts_with("invalid suite, 7 errors:\n  cases.badAttrs.attrs.1: "));
  }

  #[test]
  fn describe_issues_names_the_root() {
    let one = ParseErrors::one(ParseError::new("", "expected object"));
    assert_eq!(
      describe_issues(&one),
      "invalid suite, 1 error:\n  (suite): expected object"
    );
  }

  #[test]
  fn compat_cases_decode_and_report_every_error() {
    let cases = decode_compat_cases(&json!({
        "b": { "expr": 42, "expected": "42" },
        "a": { "expr": "1 + 1", "expected": "2" }
    }))
    .expect("compat");
    assert_eq!(
      cases,
      [
        CompatCase {
          name: "a".into(),
          expr: "1 + 1".into(),
          expected: "2".into()
        },
        CompatCase {
          name: "b".into(),
          expr: "42".into(),
          expected: "42".into()
        },
      ]
    );
    let errors = decode_compat_cases(&json!({
        "x": "not-object",
        "y": { "expected": "1" },
        "z": { "expr": "1" }
    }))
    .expect_err("invalid");
    assert_eq!(
      issues(&errors),
      [
        "x: expected object",
        "y.expr: missing field",
        "z.expected: missing field"
      ]
    );
  }

  #[test]
  fn claim_schema_has_schema_trait() {
    let wire = Claim::schema().encode(Claim::Law {
      name: "n".into(),
      seed: 1,
    });
    assert!(matches!(wire, Unknown::Object(_)));
    assert!(matches!(
      SuiteCases::schema().encode(SuiteCases(BTreeMap::new())),
      Unknown::Object(_)
    ));
  }
}
