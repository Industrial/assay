//! Effect programs for CLI commands, and the pure renderers for their typed outputs.

use std::path::PathBuf;
use std::sync::Arc;

use id_effect::{Effect, effect};
use serde_json::{Map, Value};

use crate::caps::{NixstoreEnv, PathInfoService};
use crate::error::InfraError;
use crate::model::{PathInfo, QueryOpts, human_bytes};

#[derive(Debug, Clone, Default)]
pub struct PathInfoFlags {
  pub json: bool,
  pub size: bool,
  pub closure_size: bool,
  pub human_readable: bool,
  pub sigs: bool,
  pub referrers: bool,
}

impl PathInfoFlags {
  /// The fields a query has to compute for these flags.
  pub fn query_opts(&self) -> QueryOpts {
    QueryOpts {
      include_referrers: self.referrers,
      include_closure_size: self.closure_size,
    }
  }
}

/// Result of `path-info`: one entry per requested path, in request order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathInfoReport {
  pub infos: Vec<PathInfo>,
}

impl PathInfoReport {
  /// JSON object keyed by store path (Nix `path-info --json` v1 shape).
  pub fn to_json_value(&self) -> Value {
    let mut out = Map::new();
    for info in &self.infos {
      out.insert(info.path.clone(), info.to_json_value());
    }
    Value::Object(out)
  }

  /// Pretty-printed JSON (no trailing newline).
  pub fn render_json(&self) -> String {
    let v = self.to_json_value();
    serde_json::to_string_pretty(&v).unwrap_or_else(|_| v.to_string())
  }

  /// Tab-separated human lines (no trailing newline).
  pub fn render_text(&self, flags: &PathInfoFlags) -> String {
    let size = |n: u64| {
      if flags.human_readable {
        human_bytes(n)
      } else {
        n.to_string()
      }
    };
    let mut lines = Vec::new();
    for info in &self.infos {
      let mut line = info.path.clone();
      if flags.size {
        line.push('\t');
        line.push_str(&size(info.nar_size));
      }
      if flags.closure_size {
        line.push('\t');
        line.push_str(&size(info.closure_size.unwrap_or(info.nar_size)));
      }
      if flags.sigs && !info.signatures.is_empty() {
        line.push('\t');
        line.push_str(&info.signatures.join(" "));
      }
      lines.push(line);
      if flags.referrers
        && let Some(refs) = &info.referrers
      {
        for rref in refs {
          lines.push(format!("  referrer: {rref}"));
        }
      }
    }
    lines.join("\n")
  }

  /// JSON when `flags.json`, else text.
  pub fn render(&self, flags: &PathInfoFlags) -> String {
    if flags.json {
      self.render_json()
    } else {
      self.render_text(flags)
    }
  }
}

/// Query one or more paths; fails on the first unknown path.
pub fn path_info(
  paths: Vec<String>,
  flags: PathInfoFlags,
) -> Effect<PathInfoReport, InfraError, NixstoreEnv> {
  effect!({
    let service = ~PathInfoService;
    let store = Arc::clone(&service.0);
    let opts = flags.query_opts();
    let mut infos = Vec::with_capacity(paths.len());
    for p in &paths {
      let info = ~store.query(p, opts);
      infos.push(info);
    }
    PathInfoReport { infos }
  })
}

/// Convenience: resolve store root from an optional `--store`.
pub fn resolve_store_root(store: Option<PathBuf>) -> PathBuf {
  store.unwrap_or_else(|| PathBuf::from(crate::db::DEFAULT_STORE_ROOT))
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::caps::{MockPathInfoStore, layer};
  use id_effect::{assert_exit_fail, expect_exit_success, run_test};

  const C: &str = "/nix/store/cccccccccccccccccccccccccccccccc-c";

  fn mock_x() -> PathInfoService {
    let mock = MockPathInfoStore::default();
    mock.set(PathInfo {
      path: "/nix/store/x".into(),
      nar_hash: "sha256-AA".into(),
      nar_size: 12,
      deriver: None,
      registration_time: 1,
      ultimate: true,
      signatures: vec![],
      ca: None,
      references: vec![],
      referrers: None,
      closure_size: None,
    });
    PathInfoService(Arc::new(mock))
  }

  #[test]
  fn path_info_json() {
    let flags = PathInfoFlags {
      json: true,
      ..Default::default()
    };
    let report = expect_exit_success(run_test(
      path_info(vec!["/nix/store/x".into()], flags).provide(mock_x()),
    ));
    assert_eq!(report.to_json_value()["/nix/store/x"]["narSize"], 12);
  }

  #[test]
  fn path_info_unknown_path_fails() {
    let exit = run_test(
      path_info(
        vec!["/nix/store/x".into(), "/nix/store/y".into()],
        PathInfoFlags::default(),
      )
      .provide(mock_x()),
    );
    assert_exit_fail(&exit, &InfraError::UnknownPath("/nix/store/y".into()));
  }

  #[test]
  fn path_info_text_sizes_from_fixture() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/minimal");
    let flags = PathInfoFlags {
      size: true,
      closure_size: true,
      human_readable: true,
      ..Default::default()
    };
    let report = expect_exit_success(run_test(
      path_info(vec![C.into()], flags.clone()).provide_layer(layer(root)),
    ));
    assert_eq!(report.render(&flags), format!("{C}\t50B\t50B"));
  }
}
