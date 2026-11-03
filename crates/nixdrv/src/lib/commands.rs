//! Effect programs for nixdrv CLI commands, and the typed outputs they render at the edge.
//!
//! Derivation bytes are read through the [`DrvSource`](crate::caps::DrvSource) capability and
//! parsed once ([`load_derivation`]); ATerm/JSON parsing, projection and store-path math are
//! plain Rust.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use id_effect::{Effect, effect, suspend};
use nixstore::edge::from_result;
use serde::Serialize;
use serde_json::{Map, Value};

use crate::ca::{FileIngestionMethod, fixed_output_path};
use crate::caps::{DrvSourceService, NixdrvEnv};
use crate::error::InfraError;
use crate::model::Derivation;
use crate::parse_aterm::parse_drv_aterm;
use crate::project::project_map;
use crate::store_path::{DEFAULT_STORE_DIR, parse_store_path};

/// Pretty-printed JSON of a command output (no trailing newline).
fn render_json<T: Serialize>(output: &T) -> Result<String, InfraError> {
  serde_json::to_string_pretty(output).map_err(|e| InfraError::Render(e.to_string()))
}

/// One derivation output as `parse` prints it: the bare path, or the path plus its fixed-output
/// hash.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(untagged)]
pub enum OutputReport {
  /// Input-addressed output: printed as the bare path string.
  Path(String),
  // Fields in key order: the printed object keeps declaration order.
  /// Fixed-output output: printed as an object with its hash data and path.
  Fixed {
    /// Expected output hash, if present.
    #[serde(skip_serializing_if = "Option::is_none")]
    hash: Option<String>,
    /// Hash algorithm (printed as `hashAlgo`), if present.
    #[serde(rename = "hashAlgo", skip_serializing_if = "Option::is_none")]
    hash_algo: Option<String>,
    /// Output store path.
    path: String,
  },
}

/// `parse` output: the derivation as JSON, empty fields omitted.
// Fields in key order: the printed object keeps declaration order.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DerivationReport {
  /// Builder arguments.
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub args: Vec<String>,
  /// Builder executable path.
  #[serde(skip_serializing_if = "String::is_empty")]
  pub builder: String,
  /// Builder environment variables.
  #[serde(skip_serializing_if = "BTreeMap::is_empty")]
  pub env: BTreeMap<String, String>,
  /// Input derivations mapped to the output names used from each (`inputDrvs`).
  #[serde(skip_serializing_if = "BTreeMap::is_empty")]
  pub input_drvs: BTreeMap<String, Vec<String>>,
  /// Input source store paths (`inputSrcs`).
  #[serde(skip_serializing_if = "Vec::is_empty")]
  pub input_srcs: Vec<String>,
  /// Derivation name (explicit field or `env.name`).
  #[serde(skip_serializing_if = "Option::is_none")]
  pub name: Option<String>,
  /// Outputs keyed by output name.
  #[serde(skip_serializing_if = "BTreeMap::is_empty")]
  pub outputs: BTreeMap<String, OutputReport>,
  /// Build platform / system.
  #[serde(skip_serializing_if = "String::is_empty")]
  pub platform: String,
}

impl From<&Derivation> for DerivationReport {
  fn from(drv: &Derivation) -> Self {
    let outputs = drv
      .outputs
      .iter()
      .map(|(k, o)| {
        let out = if o.hash_algo.is_some() || o.hash.is_some() {
          OutputReport::Fixed {
            hash: o.hash.clone(),
            hash_algo: o.hash_algo.clone(),
            path: o.path.clone(),
          }
        } else {
          OutputReport::Path(o.path.clone())
        };
        (k.clone(), out)
      })
      .collect();
    Self {
      args: drv.args.clone(),
      builder: drv.builder.clone(),
      env: drv.env.clone(),
      input_drvs: drv.input_drvs.clone(),
      input_srcs: drv.input_srcs.clone(),
      name: drv.name().map(str::to_owned),
      outputs,
      platform: drv.platform.clone(),
    }
  }
}

impl DerivationReport {
  /// Pretty-printed JSON (no trailing newline).
  pub fn render(&self) -> Result<String, InfraError> {
    render_json(self)
  }
}

/// `project` output: the selected fields (missing ones omitted).
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(transparent)]
pub struct Projection {
  /// Selected field names mapped to their JSON values.
  pub fields: Map<String, Value>,
}

impl Projection {
  /// Pretty-printed JSON object (no trailing newline).
  pub fn render(&self) -> Result<String, InfraError> {
    render_json(self)
  }
}

/// `store-path parse` output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StorePathReport {
  /// Nix-base32 hash part.
  pub hash: String,
  /// Name part.
  pub name: String,
  /// Full path under the default store directory.
  pub path: String,
}

impl StorePathReport {
  /// Pretty-printed JSON object (no trailing newline).
  pub fn render(&self) -> Result<String, InfraError> {
    render_json(self)
  }
}

/// `store-path make-fixed` output: the full store path, printed as a JSON string literal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct FixedPathReport {
  /// Full fixed-output store path.
  pub path: String,
}

impl FixedPathReport {
  /// The path as a quoted JSON string (no trailing newline).
  pub fn render(&self) -> Result<String, InfraError> {
    render_json(self)
  }
}

fn trim_start(bytes: &[u8]) -> &[u8] {
  match bytes.iter().position(|b| !b.is_ascii_whitespace()) {
    Some(i) => &bytes[i..],
    None => &[],
  }
}

fn is_aterm(bytes: &[u8]) -> bool {
  trim_start(bytes).starts_with(b"Derive")
}

/// Parses `bytes` read from `input` as an ATerm `.drv` (when it starts with `Derive`) or as
/// derivation JSON.
pub fn parse_derivation(input: &Path, bytes: &[u8]) -> Result<Derivation, InfraError> {
  if is_aterm(bytes) {
    return parse_drv_aterm(bytes).map_err(InfraError::from);
  }
  let value: Value = serde_json::from_slice(bytes).map_err(|e| InfraError::Json {
    input: input_name(input),
    message: e.to_string(),
  })?;
  Derivation::from_json(&value).map_err(InfraError::from)
}

fn input_name(input: &Path) -> String {
  if input.as_os_str() == "-" {
    "stdin".into()
  } else {
    input.display().to_string()
  }
}

/// `path` split into hash, name and full path.
pub fn store_path_report(path: &str) -> Result<StorePathReport, InfraError> {
  let sp = parse_store_path(path, DEFAULT_STORE_DIR)?;
  Ok(StorePathReport {
    path: sp.full_path(DEFAULT_STORE_DIR),
    hash: sp.hash,
    name: sp.name,
  })
}

fn parse_ingestion_method(method: &str) -> Result<FileIngestionMethod, InfraError> {
  match method {
    "flat" => Ok(FileIngestionMethod::Flat),
    "recursive" => Ok(FileIngestionMethod::Recursive),
    other => Err(InfraError::InvalidArg {
      arg: "--method".into(),
      message: format!("unknown method {other} (expected flat or recursive)"),
    }),
  }
}

/// The fixed-output store path for a known digest (`store_dir` defaults to `/nix/store`).
pub fn fixed_path_report(
  name: &str,
  method: &str,
  hash_algo: &str,
  digest_hex: &str,
  store_dir: Option<&str>,
) -> Result<FixedPathReport, InfraError> {
  let method = parse_ingestion_method(method)?;
  let digest = hex::decode(digest_hex).map_err(|e| InfraError::InvalidArg {
    arg: "--digest-hex".into(),
    message: e.to_string(),
  })?;
  let store_dir = store_dir.unwrap_or(DEFAULT_STORE_DIR);
  let sp = fixed_output_path(name, method, hash_algo, &digest, store_dir)?;
  Ok(FixedPathReport {
    path: sp.full_path(store_dir),
  })
}

/// Reads `input` through the [`DrvSource`](crate::caps::DrvSource) and parses it.
pub fn load_derivation(input: PathBuf) -> Effect<Derivation, InfraError, NixdrvEnv> {
  effect!({
    let service = ~DrvSourceService;
    let source = Arc::clone(&service.0);
    let bytes = ~source.read(&input);
    ~from_result(parse_derivation(&input, &bytes))
  })
}

/// `parse`: the derivation in `input` as JSON.
pub fn parse(input: PathBuf) -> Effect<DerivationReport, InfraError, NixdrvEnv> {
  effect!({
    let drv = ~load_derivation(input);
    DerivationReport::from(&drv)
  })
}

/// `project`: the selected `fields` of the derivation in `input`.
pub fn project_fields(
  input: PathBuf,
  fields: Vec<String>,
) -> Effect<Projection, InfraError, NixdrvEnv> {
  effect!({
    let drv = ~load_derivation(input);
    let refs: Vec<&str> = fields.iter().map(String::as_str).collect();
    Projection {
      fields: project_map(&drv, &refs),
    }
  })
}

/// `store-path parse`: needs nothing.
pub fn store_path_parse(path: String) -> Effect<StorePathReport, InfraError> {
  suspend(move || from_result(store_path_report(&path)))
}

/// `store-path make-fixed`: needs nothing.
pub fn store_path_make_fixed(
  name: String,
  method: String,
  hash_algo: String,
  digest_hex: String,
  store_dir: Option<String>,
) -> Effect<FixedPathReport, InfraError> {
  suspend(move || {
    from_result(fixed_path_report(
      &name,
      &method,
      &hash_algo,
      &digest_hex,
      store_dir.as_deref(),
    ))
  })
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::model::DerivationOutput;
  use id_effect::{Exit, assert_exit_fail, expect_exit_failure, expect_exit_success, run_test};
  use nixq::MockJsonSource;

  const SIMPLE_DRV: &str = include_str!("../../fixtures/simple.drv");
  const SIMPLE_JSON: &str = include_str!("../../fixtures/simple.json");
  const SIMPLE_OUT: &str = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-simple-1.0";

  fn source(files: &[(&str, &[u8])], stdin: &[u8]) -> DrvSourceService {
    let mock = MockJsonSource::default();
    for (path, body) in files {
      mock.set_file(*path, *body);
    }
    mock.set_stdin(stdin);
    DrvSourceService(Arc::new(mock))
  }

  fn run<A: Send + 'static>(
    program: Effect<A, InfraError, NixdrvEnv>,
    files: &[(&str, &[u8])],
  ) -> Exit<A, InfraError> {
    run_test(program.provide(source(files, b"")))
  }

  #[test]
  fn parse_aterm_fixture() {
    let report = expect_exit_success(run(
      parse("simple.drv".into()),
      &[("simple.drv", SIMPLE_DRV.as_bytes())],
    ));
    assert_eq!(report.name.as_deref(), Some("simple-1.0"));
    assert_eq!(report.platform, "x86_64-linux");
    assert_eq!(
      report.outputs.get("out"),
      Some(&OutputReport::Path(SIMPLE_OUT.into()))
    );
  }

  #[test]
  fn parse_json_fixture_matches_aterm() {
    let files: &[(&str, &[u8])] = &[
      ("simple.drv", SIMPLE_DRV.as_bytes()),
      ("simple.json", SIMPLE_JSON.as_bytes()),
    ];
    let aterm = expect_exit_success(run(parse("simple.drv".into()), files));
    let json = expect_exit_success(run(parse("simple.json".into()), files));
    assert_eq!(aterm, json);
  }

  #[test]
  fn parse_reads_stdin_dash() {
    let report = expect_exit_success(run_test(
      parse("-".into()).provide(source(&[], SIMPLE_JSON.as_bytes())),
    ));
    assert_eq!(report.name.as_deref(), Some("simple-1.0"));
  }

  #[test]
  fn parse_failures_are_typed() {
    let files: &[(&str, &[u8])] = &[("bad.json", b"not json")];
    assert!(matches!(
      expect_exit_failure(run(parse("bad.json".into()), files)),
      InfraError::Json { ref input, .. } if input == "bad.json"
    ));
    assert_exit_fail(
      &run(parse("absent.drv".into()), files),
      &InfraError::Source(nixq::InfraError::Io {
        path: "absent.drv".into(),
        message: "mock: file not found".into(),
      }),
    );
  }

  #[test]
  fn project_fields_reads_and_projects() {
    let projection = expect_exit_success(run(
      project_fields("simple.drv".into(), vec!["name".into(), "outPath".into()]),
      &[("simple.drv", SIMPLE_DRV.as_bytes())],
    ));
    assert_eq!(projection.fields["name"], "simple-1.0");
    assert_eq!(projection.fields["outPath"], SIMPLE_OUT);
  }

  #[test]
  fn store_path_parse_ok_and_invalid() {
    let path = "/nix/store/00000000000000000000000000000000-hello-1.0";
    let report = expect_exit_success(run_test(store_path_parse(path.into())));
    assert_eq!(
      report,
      StorePathReport {
        hash: "00000000000000000000000000000000".into(),
        name: "hello-1.0".into(),
        path: path.into(),
      }
    );
    assert!(matches!(
      expect_exit_failure(run_test(store_path_parse("not-a-path".into()))),
      InfraError::Parse(_)
    ));
  }

  #[test]
  fn store_path_make_fixed_returns_path() {
    let report = expect_exit_success(run_test(store_path_make_fixed(
      "pkg".into(),
      "flat".into(),
      "sha256".into(),
      "11".repeat(32),
      None,
    )));
    assert!(report.path.starts_with("/nix/store/"));
    assert!(report.path.ends_with("-pkg"));
  }

  #[test]
  fn store_path_make_fixed_rejects_bad_args() {
    let make = |method: &str, digest: &str| {
      expect_exit_failure(run_test(store_path_make_fixed(
        "pkg".into(),
        method.into(),
        "sha256".into(),
        digest.into(),
        None,
      )))
    };
    assert!(matches!(
      make("nar", &"11".repeat(32)),
      InfraError::InvalidArg { ref arg, .. } if arg == "--method"
    ));
    assert!(matches!(
      make("flat", "zz"),
      InfraError::InvalidArg { ref arg, .. } if arg == "--digest-hex"
    ));
  }

  #[test]
  fn renders_match_cli_output() {
    let fixed = FixedPathReport {
      path: "/nix/store/x-foo".into(),
    };
    assert_eq!(fixed.render(), Ok("\"/nix/store/x-foo\"".into()));
    let sp = StorePathReport {
      hash: "h".into(),
      name: "n".into(),
      path: "p".into(),
    };
    assert_eq!(
      sp.render(),
      Ok("{\n  \"hash\": \"h\",\n  \"name\": \"n\",\n  \"path\": \"p\"\n}".into())
    );
    let empty = Projection { fields: Map::new() };
    assert_eq!(empty.render(), Ok("{}".into()));
  }

  #[test]
  fn derivation_report_output_hashes() {
    let mut drv = Derivation::default();
    drv.outputs.insert(
      "out".into(),
      DerivationOutput {
        path: "/nix/store/x".into(),
        hash_algo: Some("sha256".into()),
        hash: None,
      },
    );
    let rendered = DerivationReport::from(&drv).render();
    assert_eq!(
      rendered,
      Ok(
        "{\n  \"outputs\": {\n    \"out\": {\n      \"hashAlgo\": \"sha256\",\n      \"path\": \"/nix/store/x\"\n    }\n  }\n}"
          .into()
      )
    );
  }

  #[test]
  fn parse_derivation_rejects_bad_json() {
    assert!(parse_derivation(Path::new("-"), b"not json").is_err());
  }
}
