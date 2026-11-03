//! Golden snapshot store — read, write, and assert JSON fixtures through the [`Fs`] capability.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use id_effect::{Effect, fail, succeed};
use nixstore::edge::from_result;
use serde_json::Value;

use crate::files::Fs;
use crate::outcome::AssayOutcome;
use crate::verdict::InfraError;

/// Where named JSON snapshot goldens live (`{root}/{name}.json`) and whether to rewrite them.
#[derive(Debug, Clone)]
pub struct SnapshotStore {
  /// Directory holding the golden files.
  pub root: PathBuf,
  /// Rewrite goldens with observed values instead of comparing (`--update-snapshots`).
  pub update_snapshots: bool,
}

impl SnapshotStore {
  /// A store rooted at `root` that compares (does not update) goldens.
  pub fn new(root: impl Into<PathBuf>) -> Self {
    Self {
      root: root.into(),
      update_snapshots: false,
    }
  }

  /// Return a copy with [`SnapshotStore::update_snapshots`] set (for `--update-snapshots`).
  pub fn with_update(mut self, update: bool) -> Self {
    self.update_snapshots = update;
    self
  }

  /// The golden `name` (`None` when it does not exist); errors name the golden's path.
  pub fn read(&self, fs: &Arc<dyn Fs>, name: &str) -> Effect<Option<Value>, InfraError> {
    let (fs, path) = (Arc::clone(fs), self.golden_path(name));
    fs.kind(&path).flat_map(move |kind| match kind {
      None => succeed(None),
      Some(_) => fs.read_text(&path).flat_map(move |text| {
        from_result(
          serde_json::from_str(&text)
            .map(Some)
            .map_err(|e| at(&path, &e)),
        )
      }),
    })
  }

  /// Writes `value` as the golden `name` (pretty JSON plus newline), creating parent dirs.
  pub fn write(&self, fs: &Arc<dyn Fs>, name: &str, value: &Value) -> Effect<(), InfraError> {
    let path = self.golden_path(name);
    match serde_json::to_string_pretty(value) {
      Ok(text) => fs.write_text(&path, format!("{text}\n")),
      Err(e) => fail(at(&path, &e)),
    }
  }

  /// Judges `observed` against the golden `name` (rewriting it when `update` or
  /// [`SnapshotStore::update_snapshots`]). Never fails: an unreadable golden is the `io`
  /// outcome.
  pub fn assert_match(
    &self,
    fs: &Arc<dyn Fs>,
    name: &str,
    observed: Value,
    update: bool,
  ) -> Effect<AssayOutcome, InfraError> {
    let judged = if update || self.update_snapshots {
      self.write(fs, name, &observed).map(|()| AssayOutcome::Pass)
    } else {
      let path = display_path(&self.golden_path(name));
      self.read(fs, name).map(move |golden| match golden {
        None => AssayOutcome::SnapshotMismatch {
          path,
          diff: "golden missing".into(),
        },
        Some(expected) if expected == observed => AssayOutcome::Pass,
        Some(expected) => AssayOutcome::SnapshotMismatch {
          path,
          diff: snapshot_diff(&expected, &observed),
        },
      })
    };
    judged.catch(|e| succeed(io_error(e)))
  }

  fn golden_path(&self, name: &str) -> PathBuf {
    self.root.join(format!("{name}.json"))
  }
}

fn at(path: &Path, e: &dyn std::fmt::Display) -> InfraError {
  InfraError::Io(format!("{}: {e}", path.display()))
}

fn snapshot_diff(expected: &Value, observed: &Value) -> String {
  let diff = nixq::structural_diff(expected, observed);
  if diff.is_empty() {
    format!("expected: {expected:?}\nobserved: {observed:?}")
  } else {
    diff
  }
}

fn display_path(path: &Path) -> String {
  path.display().to_string()
}

fn io_error(e: InfraError) -> AssayOutcome {
  AssayOutcome::EvalError {
    kind: "io".into(),
    message: match e {
      InfraError::Io(message) => message,
      other => other.to_string(),
    },
    span: None,
  }
}

#[cfg(test)]
mod tests {
  use id_effect::{expect_exit_success, run_test};
  use serde_json::json;

  use super::*;
  use crate::fake::MemFs;

  fn store() -> (SnapshotStore, Arc<dyn Fs>) {
    (SnapshotStore::new("/goldens"), Arc::new(MemFs::default()))
  }

  fn read(store: &SnapshotStore, fs: &Arc<dyn Fs>, name: &str) -> Option<Value> {
    expect_exit_success(run_test(store.read(fs, name)))
  }

  fn write(store: &SnapshotStore, fs: &Arc<dyn Fs>, name: &str, value: &Value) {
    expect_exit_success(run_test(store.write(fs, name, value)));
  }

  fn judge(store: &SnapshotStore, fs: &Arc<dyn Fs>, name: &str, observed: Value) -> AssayOutcome {
    expect_exit_success(run_test(store.assert_match(fs, name, observed, false)))
  }

  #[test]
  fn read_missing_returns_none() {
    let (store, fs) = store();
    assert!(read(&store, &fs, "missing").is_none());
  }

  #[test]
  fn write_then_read_round_trips() {
    let (store, fs) = store();
    let value = json!({"a": 1, "b": [2, 3]});
    write(&store, &fs, "roundtrip", &value);
    assert_eq!(read(&store, &fs, "roundtrip"), Some(value));
  }

  #[test]
  fn write_is_pretty_json_with_newline() {
    let (store, fs) = store();
    write(&store, &fs, "pretty", &json!({"a": 1}));
    let text = expect_exit_success(run_test(fs.read_text(Path::new("/goldens/pretty.json"))));
    assert_eq!(text, "{\n  \"a\": 1\n}\n");
  }

  #[test]
  fn write_nested_golden() {
    let (store, fs) = store();
    let value = json!({"nested": true});
    write(&store, &fs, "nested/deep/golden", &value);
    assert_eq!(read(&store, &fs, "nested/deep/golden"), Some(value));
  }

  #[test]
  fn assert_match_passes_when_equal() {
    let (store, fs) = store();
    let value = json!({"x": 1});
    write(&store, &fs, "eq", &value);
    assert_eq!(judge(&store, &fs, "eq", value), AssayOutcome::Pass);
  }

  #[test]
  fn assert_match_missing_is_snapshot_mismatch() {
    let (store, fs) = store();
    assert_eq!(
      judge(&store, &fs, "absent", json!(1)),
      AssayOutcome::SnapshotMismatch {
        path: "/goldens/absent.json".into(),
        diff: "golden missing".into(),
      }
    );
  }

  #[test]
  fn assert_match_differs_reports_structural_diff() {
    let (store, fs) = store();
    write(&store, &fs, "diff", &json!({"a": 1}));
    match judge(&store, &fs, "diff", json!({"a": 2})) {
      AssayOutcome::SnapshotMismatch { diff, .. } => assert!(diff.contains("$.a")),
      other => panic!("expected SnapshotMismatch, got {other:?}"),
    }
  }

  #[test]
  fn assert_match_update_writes_and_passes() {
    let (store, fs) = store();
    let value = json!({"fresh": true});
    let out = expect_exit_success(run_test(store.assert_match(
      &fs,
      "new",
      value.clone(),
      true,
    )));
    assert_eq!(out, AssayOutcome::Pass);
    assert_eq!(read(&store, &fs, "new"), Some(value));
  }

  #[test]
  fn snapshot_diff_uses_debug_when_values_equal() {
    let v = json!({"k": 1});
    let diff = snapshot_diff(&v, &v);
    assert!(diff.contains("expected:"));
    assert!(diff.contains("observed:"));
  }

  #[test]
  fn assert_match_corrupt_golden_returns_io_error() {
    let (store, fs) = store();
    expect_exit_success(run_test(
      fs.write_text(Path::new("/goldens/corrupt.json"), "not-json".into()),
    ));
    match judge(&store, &fs, "corrupt", json!(2)) {
      AssayOutcome::EvalError { kind, message, .. } => {
        assert_eq!(kind, "io");
        assert!(message.starts_with("/goldens/corrupt.json: "), "{message}");
      }
      other => panic!("expected io error, got {other:?}"),
    }
  }

  #[test]
  fn with_update_flag_drives_assert_match() {
    let (store, fs) = store();
    let store = store.with_update(true);
    let value = json!({"flag": true});
    assert_eq!(
      judge(&store, &fs, "flagged", value.clone()),
      AssayOutcome::Pass
    );
    assert_eq!(read(&store, &fs, "flagged"), Some(value));
  }

  #[test]
  fn io_error_keeps_io_messages() {
    assert_eq!(
      io_error(InfraError::Io("p: denied".into())),
      AssayOutcome::EvalError {
        kind: "io".into(),
        message: "p: denied".into(),
        span: None,
      }
    );
    assert!(matches!(
      io_error(InfraError::Worker("w".into())),
      AssayOutcome::EvalError { message, .. } if message == "runner error: w"
    ));
  }
}
