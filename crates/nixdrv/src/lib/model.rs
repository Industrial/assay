//! Derivation model — shared between ATerm and JSON parsers.

use std::collections::BTreeMap;

/// One named output of a derivation (an entry of the ATerm `outputs` list).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DerivationOutput {
  /// Output store path (may be empty for not-yet-computed / floating outputs).
  pub path: String,
  /// Fixed-output hash algorithm (e.g. `sha256`, `r:sha256`); `None` for input-addressed outputs.
  pub hash_algo: Option<String>,
  /// Fixed-output expected hash; `None` for input-addressed outputs.
  pub hash: Option<String>,
}

/// A parsed Nix derivation (`.drv`), independent of ATerm or JSON source format.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Derivation {
  /// Outputs keyed by output name (`out`, `dev`, ...).
  pub outputs: BTreeMap<String, DerivationOutput>,
  /// Input derivation paths mapped to the output names used from each.
  pub input_drvs: BTreeMap<String, Vec<String>>,
  /// Input source store paths (non-derivation inputs).
  pub input_srcs: Vec<String>,
  /// Build platform / system (e.g. `x86_64-linux`).
  pub platform: String,
  /// Path of the builder executable.
  pub builder: String,
  /// Arguments passed to the builder.
  pub args: Vec<String>,
  /// Builder environment variables.
  pub env: BTreeMap<String, String>,
  /// Explicit derivation name (JSON form); falls back to `env.name` via [`Derivation::name`].
  pub name: Option<String>,
}

impl Derivation {
  /// Path of the `out` output, falling back to the `out` environment variable.
  pub fn default_out_path(&self) -> Option<&str> {
    self
      .outputs
      .get("out")
      .map(|o| o.path.as_str())
      .or_else(|| self.env.get("out").map(String::as_str))
  }

  /// Derivation name: the explicit `name` field, else the `name` environment variable.
  pub fn name(&self) -> Option<&str> {
    self
      .name
      .as_deref()
      .or_else(|| self.env.get("name").map(String::as_str))
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sample() -> Derivation {
    let mut outputs = BTreeMap::new();
    outputs.insert(
      "out".into(),
      DerivationOutput {
        path: "/nix/store/abc-out".into(),
        hash_algo: None,
        hash: None,
      },
    );
    let mut env = BTreeMap::new();
    env.insert("name".into(), "hello".into());
    Derivation {
      outputs,
      input_drvs: BTreeMap::new(),
      input_srcs: vec![],
      platform: "x86_64-linux".into(),
      builder: "/bin/sh".into(),
      args: vec![],
      env,
      name: None,
    }
  }

  #[test]
  fn default_out_path_and_name_from_env() {
    let drv = sample();
    assert_eq!(drv.default_out_path(), Some("/nix/store/abc-out"));
    assert_eq!(drv.name(), Some("hello"));
  }

  #[test]
  fn name_prefers_explicit_field() {
    let mut drv = sample();
    drv.name = Some("explicit".into());
    assert_eq!(drv.name(), Some("explicit"));
  }
}
