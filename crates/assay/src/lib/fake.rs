//! In-memory fakes for tests (`cfg(test)` / the `test-support` feature).
//!
//! [`SyncEval`] is a scripted evaluator: implement `eval_json` (and optionally the file / source
//! variants) and it is a [`NixEval`]. Its effects are built from the scripted answer when the
//! call is made, so a retried effect asks the script again per attempt. [`MemFs`] is an
//! in-memory [`Fs`].

// id_effect_lints: test fakes keep their scripted / in-memory state behind a `Mutex` (shared
// with the effects they hand out, not fiber coordination); `SNAP_SEQ` is a temp-name counter.
#![cfg_attr(
  dylint_lib = "id_effect_lints",
  allow(no_mutex_for_fiber_coordination, no_static_service_globals)
)]

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use id_effect::{Effect, Layer, suspend, sync};
use nixstore::edge::from_result;
use serde_json::Value;

use crate::caps::{
  AssayEnv, FsService, NixEvalService, PathInfoService, SnapshotService, layer_from,
};
use crate::eval::{EvalResult, NixEval};
use crate::files::{DirEntry, EntryKind, Fs};
use crate::outcome::AssayOutcome;
use crate::snapshot::SnapshotStore;
use crate::verdict::InfraError;

/// A scripted, synchronous evaluator.
pub trait SyncEval: Send + Sync {
  /// The answer for `nix eval --expr {expr}`.
  fn eval_json(&self, expr: &str) -> EvalResult;
  /// The answer for evaluating `source` from a file (default: like [`eval_json`](Self::eval_json)).
  fn eval_source_json(&self, source: &str) -> EvalResult {
    self.eval_json(source)
  }
  /// The answer for `nix eval --file {path}` (default: an io error).
  fn eval_file_json(&self, path: &Path) -> EvalResult {
    EvalResult::Err(AssayOutcome::EvalError {
      kind: "io".into(),
      message: format!("fake: no file {}", path.display()),
      span: None,
    })
  }
}

impl<T: SyncEval + ?Sized> NixEval for T {
  fn eval_expr(&self, expr: &str) -> Effect<Value, AssayOutcome> {
    self.eval_json(expr).lift()
  }

  fn eval_file(&self, path: &Path) -> Effect<Value, AssayOutcome> {
    self.eval_file_json(path).lift()
  }

  fn eval_source(&self, source: &str) -> Effect<Value, AssayOutcome> {
    self.eval_source_json(source).lift()
  }
}

/// In-memory evaluator: answers set with [`set`](Self::set) / [`set_file`](Self::set_file);
/// unknown expressions evaluate to `null`.
#[derive(Default)]
pub struct MockNixEval {
  values: Mutex<HashMap<String, EvalResult>>,
  files: Mutex<HashMap<PathBuf, EvalResult>>,
}

impl MockNixEval {
  /// Script the result of evaluating the expression `expr`.
  pub fn set(&self, expr: &str, result: EvalResult) {
    self
      .values
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .insert(expr.into(), result);
  }

  /// Script the result of evaluating the file at `path`.
  pub fn set_file(&self, path: impl Into<PathBuf>, result: EvalResult) {
    self
      .files
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .insert(path.into(), result);
  }
}

impl SyncEval for MockNixEval {
  fn eval_json(&self, expr: &str) -> EvalResult {
    let hit = self
      .values
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .get(expr)
      .cloned();
    if let Some(hit) = hit {
      return hit;
    }
    // Batched eq form: [(left) (right)] — resolve sides independently.
    if let Some((left, right)) = split_eq_pair(expr) {
      let l = self.eval_json(left);
      let r = self.eval_json(right);
      return match (l, r) {
        (EvalResult::Ok(a), EvalResult::Ok(b)) => EvalResult::Ok(Value::Array(vec![a, b])),
        (EvalResult::Err(e), _) | (_, EvalResult::Err(e)) => EvalResult::Err(e),
      };
    }
    EvalResult::Ok(Value::Null)
  }

  fn eval_file_json(&self, path: &Path) -> EvalResult {
    let hit = self
      .files
      .lock()
      .unwrap_or_else(PoisonError::into_inner)
      .get(path)
      .cloned();
    hit.unwrap_or_else(|| {
      EvalResult::Err(AssayOutcome::EvalError {
        kind: "io".into(),
        message: format!("fake: no file {}", path.display()),
        span: None,
      })
    })
  }
}

/// Parse assay batched-eq wrapper `[({left}) ({right})]`.
pub(crate) fn split_eq_pair(expr: &str) -> Option<(&str, &str)> {
  let trimmed = expr.trim();
  let inner = trimmed.strip_prefix('[')?.strip_suffix(']')?.trim();
  let (left_raw, right_raw) = split_top_level_pair(inner)?;
  let left = unwrap_parens(left_raw.trim())?;
  let right = unwrap_parens(right_raw.trim())?;
  Some((left, right))
}

fn unwrap_parens(s: &str) -> Option<&str> {
  s.strip_prefix('(')?.strip_suffix(')')
}

fn split_top_level_pair(s: &str) -> Option<(&str, &str)> {
  let mut depth = 0i32;
  for (i, ch) in s.char_indices() {
    match ch {
      '(' | '[' | '{' => depth += 1,
      ')' | ']' | '}' => depth -= 1,
      ' ' | '\t' | '\n' if depth == 0 => {
        let left = s[..i].trim();
        let right = s[i..].trim();
        if !left.is_empty() && !right.is_empty() {
          return Some((left, right));
        }
      }
      _ => {}
    }
  }
  None
}

static SNAP_SEQ: AtomicU64 = AtomicU64::new(0);

/// A snapshot store under a fresh, not yet created `$TMPDIR` directory (no goldens).
pub fn temp_snapshot_store() -> SnapshotStore {
  let n = SNAP_SEQ.fetch_add(1, Ordering::Relaxed);
  SnapshotStore::new(
    std::env::temp_dir().join(format!("assay-fake-snap-{}-{n}", std::process::id())),
  )
}

/// An in-memory filesystem: text files by path; directories are implied by the files under
/// them. Each effect reads the state when it runs.
#[derive(Default, Clone)]
pub struct MemFs {
  files: Arc<Mutex<BTreeMap<PathBuf, String>>>,
}

impl MemFs {
  /// A filesystem holding `files`.
  pub fn with_files<P: Into<PathBuf>, S: Into<String>>(
    files: impl IntoIterator<Item = (P, S)>,
  ) -> Self {
    let fs = Self::default();
    fs.lock()
      .extend(files.into_iter().map(|(p, s)| (p.into(), s.into())));
    fs
  }

  /// The text of the file at `path`, if any.
  pub fn file(&self, path: impl AsRef<Path>) -> Option<String> {
    self.lock().get(path.as_ref()).cloned()
  }

  fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<PathBuf, String>> {
    self.files.lock().unwrap_or_else(PoisonError::into_inner)
  }

  fn kind_of(&self, path: &Path) -> Option<EntryKind> {
    let files = self.lock();
    if files.contains_key(path) {
      Some(EntryKind::File)
    } else if files.keys().any(|f| f != path && f.starts_with(path)) {
      Some(EntryKind::Dir)
    } else {
      None
    }
  }

  fn list(&self, dir: &Path) -> Result<Vec<DirEntry>, InfraError> {
    if self.kind_of(dir) != Some(EntryKind::Dir) {
      return Err(InfraError::Io(format!(
        "{}: not a directory",
        dir.display()
      )));
    }
    // Child path -> whether it is a directory.
    let mut children = BTreeMap::new();
    for file in self.lock().keys() {
      let Ok(rel) = file.strip_prefix(dir) else {
        continue;
      };
      let mut parts = rel.components();
      if let Some(first) = parts.next() {
        let is_dir = parts.next().is_some();
        *children.entry(dir.join(first)).or_insert(false) |= is_dir;
      }
    }
    Ok(
      children
        .into_iter()
        .map(|(path, is_dir)| DirEntry { path, is_dir })
        .collect(),
    )
  }
}

impl Fs for MemFs {
  fn read_text(&self, path: &Path) -> Effect<String, InfraError> {
    let (fs, path) = (self.clone(), path.to_path_buf());
    suspend(move || {
      from_result(
        fs.file(&path)
          .ok_or_else(|| InfraError::Io(format!("{}: not found", path.display()))),
      )
    })
  }

  fn write_text(&self, path: &Path, text: String) -> Effect<(), InfraError> {
    let (fs, path) = (self.clone(), path.to_path_buf());
    sync(move || {
      fs.lock().insert(path, text);
    })
  }

  fn kind(&self, path: &Path) -> Effect<Option<EntryKind>, InfraError> {
    let (fs, path) = (self.clone(), path.to_path_buf());
    sync(move || fs.kind_of(&path))
  }

  fn read_dir(&self, path: &Path) -> Effect<Vec<DirEntry>, InfraError> {
    let (fs, path) = (self.clone(), path.to_path_buf());
    suspend(move || from_result(fs.list(&path)))
  }
}

/// [`AssayEnv`] over `eval`, a [`temp_snapshot_store`], an empty in-memory path-info store and
/// an empty [`MemFs`].
pub fn fake_layer<E: Send + 'static>(eval: impl NixEval + 'static) -> Layer<AssayEnv, E> {
  fake_layer_with_fs(eval, MemFs::default())
}

/// [`fake_layer`] over the filesystem `fs`.
pub fn fake_layer_with_fs<E: Send + 'static>(
  eval: impl NixEval + 'static,
  fs: impl Fs + 'static,
) -> Layer<AssayEnv, E> {
  layer_from(
    NixEvalService(Arc::new(eval)),
    SnapshotService(temp_snapshot_store()),
    PathInfoService(Arc::new(nixstore::MockPathInfoStore::default())),
    FsService(Arc::new(fs)),
  )
}

#[cfg(test)]
mod tests {
  use id_effect::{expect_exit_failure, expect_exit_success, run_test};

  use super::*;

  fn throw(message: &str) -> EvalResult {
    EvalResult::Err(AssayOutcome::EvalError {
      kind: "throw".into(),
      message: message.into(),
      span: None,
    })
  }

  #[test]
  fn mock_nix_eval_returns_configured_values() {
    let mock = MockNixEval::default();
    mock.set("x", EvalResult::Ok(serde_json::json!(42)));
    assert_eq!(
      expect_exit_success(run_test(mock.eval_expr("x"))),
      serde_json::json!(42)
    );
    assert_eq!(mock.eval_json("missing"), EvalResult::Ok(Value::Null));
  }

  #[test]
  fn mock_nix_eval_batched_eq_pair_and_errors() {
    let mock = MockNixEval::default();
    mock.set("1", EvalResult::Ok(serde_json::json!(1)));
    mock.set("2", EvalResult::Ok(serde_json::json!(2)));
    mock.set("bad", throw("boom"));
    assert_eq!(
      mock.eval_json("[ (1) (2) ]"),
      EvalResult::Ok(serde_json::json!([1, 2]))
    );
    assert!(matches!(
      mock.eval_json("[ (bad) (1) ]"),
      EvalResult::Err(_)
    ));
    assert!(matches!(
      mock.eval_json("[ (1) (bad) ]"),
      EvalResult::Err(_)
    ));
  }

  #[test]
  fn mock_nix_eval_files() {
    let mock = MockNixEval::default();
    mock.set_file("/s.nix", EvalResult::Ok(serde_json::json!({"a": 1})));
    assert_eq!(
      expect_exit_success(run_test(mock.eval_file(Path::new("/s.nix")))),
      serde_json::json!({"a": 1})
    );
    assert!(matches!(
      expect_exit_failure(run_test(mock.eval_file(Path::new("/other.nix")))),
      AssayOutcome::EvalError { .. }
    ));
  }

  #[test]
  fn split_eq_pair_parsing_edge_cases() {
    assert_eq!(split_eq_pair("[ (1) (2) ]"), Some(("1", "2")));
    assert!(split_eq_pair("not").is_none());
    assert!(split_eq_pair("[ (only) ]").is_none());
    assert!(split_eq_pair("[ 1 2 ]").is_none());
    assert!(split_eq_pair("[ 1 ) ( 2 ]").is_none());
    assert!(unwrap_parens("(no-close").is_none());
  }

  #[test]
  fn split_top_level_pair_respects_nesting() {
    assert_eq!(
      split_top_level_pair("(a (b c)) (d)"),
      Some(("(a (b c))", "(d)"))
    );
    assert_eq!(split_top_level_pair("[a] {b}"), Some(("[a]", "{b}")));
    assert!(split_top_level_pair("single").is_none());
    assert!(split_top_level_pair("   ").is_none());
    assert!(split_top_level_pair("foo ").is_none());
    assert!(split_top_level_pair(" foo").is_none());
  }
}
