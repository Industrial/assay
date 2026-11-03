//! Live adapters: `nix eval` through `tokio::process`, the local filesystem and the lazily
//! opened store database. This is the only module that spawns processes, touches files or opens
//! the store.
//!
//! # Where the work runs
//!
//! `run_main` polls the root fiber outside the tokio runtime context, while forked fibers (every
//! parallel case, every timeout racer) run on tokio worker threads. So `nix` never runs inline in
//! a fiber: the process future runs as a task on the fiber's executor (`on_executor`), where
//! `tokio::process` has its runtime. Interrupting the effect (a case timeout, Ctrl-C) drops the
//! future inside the task, which kills the `nix` child (`kill_on_drop`).

// id_effect_lints: this is the I/O boundary. Its async helpers run only inside `on_executor`
// tasks; `settle` / `on_executor` / `blocking` pass any `E` through unchanged (no `From` to
// apply); the statics are temp-name counters, not services.
#![cfg_attr(
  dylint_lib = "id_effect_lints",
  allow(
    no_async_fn_in_effect_crate,
    effect_error_bound_must_use_from,
    no_static_service_globals
  )
)]

use std::future::{Future, poll_fn};
use std::path::{Path, PathBuf};
use std::pin::{Pin, pin};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};
use std::task::Poll;

use id_effect::{Ctx, Effect, HNil, die, fail, succeed, suspend};
use nixstore::{PathInfo, PathInfoStore, QueryOpts, SqlitePathInfoStore};
use serde_json::Value;
use tokio::sync::oneshot;

use crate::eval::{NixEval, eval_output, nix_spawn_error, nix_spawn_error_for_path};
use crate::files::{DirEntry, EntryKind, Fs};
use crate::outcome::AssayOutcome;
use crate::verdict::InfraError;

/// The outcome a task hands back: `None` when it ended without one (it panicked).
fn settle<A, E>(out: Option<Result<A, E>>) -> Effect<A, E>
where
  A: Send + 'static,
  E: Send + 'static,
{
  match out {
    Some(Ok(a)) => succeed(a),
    Some(Err(e)) => fail(e),
    None => die("assay: an executor task (nix or fs) ended without a result"),
  }
}

/// Runs the future built by `make` as a task on the fiber's executor and awaits its result.
/// `make` runs inside the task (so inside the tokio runtime context on the default runtime).
/// When this effect is interrupted, the task drops the future.
fn on_executor<A, E, Fut>(make: impl FnOnce() -> Fut + Send + 'static) -> Effect<A, E>
where
  A: Send + 'static,
  E: Send + 'static,
  Fut: Future<Output = Result<A, E>> + Send + 'static,
{
  Effect::from_ctx(move |ctx: Ctx<HNil>| {
    let (done_tx, done_rx) = oneshot::channel();
    // Dropped with the awaiting future (on interrupt) or after the result arrived.
    let (cancel_tx, mut cancel_rx) = oneshot::channel::<()>();
    ctx.fiber().executor().spawn(Box::pin(async move {
      let mut work = pin!(make());
      let out = poll_fn(|cx| {
        if Pin::new(&mut cancel_rx).poll(cx).is_ready() {
          return Poll::Ready(None);
        }
        work.as_mut().poll(cx).map(Some)
      })
      .await;
      if let Some(out) = out {
        let _ = done_tx.send(out);
      }
    }));
    async move {
      let out = done_rx.await.ok();
      drop(cancel_tx);
      Ok::<_, E>(out)
    }
  })
  .flat_map(settle)
}

// ── nix ─────────────────────────────────────────────────────────────────────────────────────

/// The `nix` binary on `PATH`: one fresh `nix eval --impure --json` process per call.
#[derive(Debug, Default, Clone, Copy)]
pub struct LiveNixEval;

/// What to evaluate.
enum Target {
  Expr(String),
  File(PathBuf),
  /// The batch source, and where the task records the temp dir it creates for it.
  Source(String, Arc<OnceLock<PathBuf>>),
}

impl NixEval for LiveNixEval {
  fn eval_expr(&self, expr: &str) -> Effect<Value, AssayOutcome> {
    let target = Target::Expr(expr.to_owned());
    on_executor(move || nix_eval(target))
  }

  fn eval_file(&self, path: &Path) -> Effect<Value, AssayOutcome> {
    let target = Target::File(path.to_path_buf());
    on_executor(move || nix_eval(target))
  }

  fn eval_source(&self, source: &str) -> Effect<Value, AssayOutcome> {
    let created = Arc::new(OnceLock::new());
    let target = Target::Source(source.to_owned(), Arc::clone(&created));
    // The executor task may still be winding down when an interrupt ends the run, so the
    // fiber removes the temp dir too: `ensuring` runs on every exit, before `run_main` returns.
    on_executor(move || nix_eval(target)).ensuring(suspend(move || {
      if let Some(dir) = created.get() {
        let _ = std::fs::remove_dir_all(dir);
      }
      succeed(())
    }))
  }
}

async fn nix_eval(target: Target) -> Result<Value, AssayOutcome> {
  match target {
    Target::Expr(expr) => run_nix(&["--expr".as_ref(), expr.as_ref()], None).await,
    Target::File(path) => run_nix(&["--file".as_ref(), path.as_os_str()], Some(&path)).await,
    Target::Source(source, created) => {
      let dir = TempDir::create().await?;
      let _ = created.set(dir.0.clone());
      let path = dir.0.join("batch.nix");
      // `dir` is removed on drop: after this match, or when the future is cancelled.
      match tokio::fs::write(&path, source).await {
        Ok(()) => run_nix(&["--file".as_ref(), path.as_os_str()], Some(&path)).await,
        Err(e) => Err(io_outcome(&e.to_string())),
      }
    }
  }
}

/// `nix eval --impure {args} --json`; `path` names the evaluated file in spawn errors.
async fn run_nix(args: &[&std::ffi::OsStr], path: Option<&Path>) -> Result<Value, AssayOutcome> {
  let out = tokio::process::Command::new("nix")
    .args(["eval", "--impure"])
    .args(args)
    .arg("--json")
    .stdin(Stdio::null())
    .kill_on_drop(true)
    .output()
    .await
    .map_err(|e| match path {
      Some(p) => nix_spawn_error_for_path(&e, p),
      None => nix_spawn_error(&e),
    })?;
  eval_output(out.status.success(), &out.stdout, &out.stderr)
}

fn io_outcome(message: &str) -> AssayOutcome {
  AssayOutcome::EvalError {
    kind: "io".into(),
    message: message.into(),
    span: None,
  }
}

/// Uniquifies temp dir names within this process (no clock involved).
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

/// A fresh `$TMPDIR/assay-batch-{pid}-{n}` directory for one batch file.
struct TempDir(PathBuf);

impl TempDir {
  async fn create() -> Result<Self, AssayOutcome> {
    let pid = std::process::id();
    loop {
      let n = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
      let dir = std::env::temp_dir().join(format!("assay-batch-{pid}-{n}"));
      match tokio::fs::create_dir(&dir).await {
        Ok(()) => return Ok(Self(dir)),
        // Left over by an earlier process with the same pid: try the next name.
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(e) => return Err(io_outcome(&format!("create {}: {e}", dir.display()))),
      }
    }
  }
}

/// Removal lives in `Drop` so the directory also goes when the eval future is dropped
/// mid-flight (an interrupt or timeout cancels it before any post-await cleanup runs).
impl Drop for TempDir {
  fn drop(&mut self) {
    let _ = std::fs::remove_dir_all(&self.0);
  }
}

// ── filesystem ──────────────────────────────────────────────────────────────────────────────

/// Runs the blocking `f` on the executor's blocking pool and awaits its result.
fn blocking<A, E>(f: impl FnOnce() -> Result<A, E> + Send + 'static) -> Effect<A, E>
where
  A: Send + 'static,
  E: Send + 'static,
{
  Effect::from_ctx(move |ctx: Ctx<HNil>| {
    let (tx, rx) = oneshot::channel();
    ctx.fiber().executor().spawn_blocking(Box::new(move || {
      let _ = tx.send(f());
    }));
    async move { Ok::<_, E>(rx.await.ok()) }
  })
  .flat_map(settle)
}

/// The local filesystem (`std::fs` on the executor's blocking pool).
#[derive(Debug, Default, Clone, Copy)]
pub struct LiveFs;

fn fs_error(path: &Path, e: &std::io::Error) -> InfraError {
  InfraError::Io(format!("{}: {e}", path.display()))
}

fn list_dir(dir: &Path) -> Result<Vec<DirEntry>, InfraError> {
  let mut entries = Vec::new();
  for entry in std::fs::read_dir(dir).map_err(|e| fs_error(dir, &e))? {
    let path = entry.map_err(|e| fs_error(dir, &e))?.path();
    entries.push(DirEntry {
      is_dir: path.is_dir(),
      path,
    });
  }
  Ok(entries)
}

impl Fs for LiveFs {
  fn read_text(&self, path: &Path) -> Effect<String, InfraError> {
    let path = path.to_path_buf();
    blocking(move || std::fs::read_to_string(&path).map_err(|e| fs_error(&path, &e)))
  }

  fn write_text(&self, path: &Path, text: String) -> Effect<(), InfraError> {
    let path = path.to_path_buf();
    blocking(move || {
      if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| fs_error(&path, &e))?;
      }
      std::fs::write(&path, text).map_err(|e| fs_error(&path, &e))
    })
  }

  fn kind(&self, path: &Path) -> Effect<Option<EntryKind>, InfraError> {
    let path = path.to_path_buf();
    blocking(move || {
      Ok(std::fs::metadata(&path).ok().map(|meta| {
        if meta.is_file() {
          EntryKind::File
        } else if meta.is_dir() {
          EntryKind::Dir
        } else {
          EntryKind::Other
        }
      }))
    })
  }

  fn read_dir(&self, path: &Path) -> Effect<Vec<DirEntry>, InfraError> {
    let path = path.to_path_buf();
    blocking(move || list_dir(&path))
  }
}

// ── store ───────────────────────────────────────────────────────────────────────────────────

/// The store database under `store_root`, opened on the first query and kept open after.
///
/// Opening lazily (rather than with `nixstore::layer`, which opens when the layer is built)
/// keeps today's contract: a suite without `pathInfo` claims never touches the database, and a
/// missing or unreadable database fails only the claims that query it (each gets the same
/// cached open error as its verdict) instead of aborting the whole run.
pub struct LazyPathInfoStore {
  inner: Arc<LazyStore>,
}

struct LazyStore {
  store_root: PathBuf,
  store: OnceLock<Result<Arc<SqlitePathInfoStore>, nixstore::InfraError>>,
}

impl LazyPathInfoStore {
  pub fn new(store_root: impl Into<PathBuf>) -> Self {
    Self {
      inner: Arc::new(LazyStore {
        store_root: store_root.into(),
        store: OnceLock::new(),
      }),
    }
  }
}

impl LazyStore {
  fn opened(&self) -> Result<Arc<SqlitePathInfoStore>, nixstore::InfraError> {
    self
      .store
      .get_or_init(|| SqlitePathInfoStore::open(&self.store_root).map(Arc::new))
      .clone()
  }
}

impl PathInfoStore for LazyPathInfoStore {
  fn query(&self, path: &str, opts: QueryOpts) -> Effect<PathInfo, nixstore::InfraError> {
    let (inner, path) = (Arc::clone(&self.inner), path.to_owned());
    suspend(move || match inner.opened() {
      Ok(store) => store.query(&path, opts),
      Err(e) => fail(e),
    })
  }
}

#[cfg(test)]
mod tests {
  use id_effect::{expect_exit_failure, expect_exit_success, run_test};

  use super::*;

  fn has_nix() -> bool {
    std::process::Command::new("nix")
      .arg("--version")
      .output()
      .is_ok_and(|o| o.status.success())
  }

  #[test]
  fn live_nix_eval_expr_file_and_source() {
    if !has_nix() {
      eprintln!("skipping live_nix_eval_expr_file_and_source: nix not on PATH");
      return;
    }
    assert_eq!(
      expect_exit_success(LiveNixEval.eval_expr("1 + 1").block_on()),
      serde_json::json!(2)
    );
    let thrown = expect_exit_failure(LiveNixEval.eval_expr("builtins.throw \"boom\"").block_on());
    assert!(matches!(thrown, AssayOutcome::EvalError { kind, .. } if kind == "throw"));
    assert_eq!(
      expect_exit_success(LiveNixEval.eval_source("{ a = 1; }").block_on()),
      serde_json::json!({"a": 1})
    );
    let missing = LiveNixEval.eval_file(Path::new("/nonexistent/assay-live.nix"));
    assert!(matches!(
      expect_exit_failure(missing.block_on()),
      AssayOutcome::EvalError { .. }
    ));
  }

  #[test]
  fn live_fs_reads_writes_and_lists() {
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    let root = std::env::temp_dir().join(format!("assay-live-fs-{}-{n}", std::process::id()));
    let file = root.join("a/b/f.json");
    expect_exit_success(run_test(LiveFs.write_text(&file, "{}\n".into())));
    assert_eq!(
      expect_exit_success(run_test(LiveFs.read_text(&file))),
      "{}\n"
    );
    assert_eq!(
      expect_exit_success(run_test(LiveFs.kind(&file))),
      Some(EntryKind::File)
    );
    assert_eq!(
      expect_exit_success(run_test(LiveFs.kind(&root))),
      Some(EntryKind::Dir)
    );
    assert_eq!(
      expect_exit_success(run_test(LiveFs.kind(&root.join("none")))),
      None
    );
    assert_eq!(
      expect_exit_success(run_test(LiveFs.read_dir(&root))),
      [DirEntry {
        path: root.join("a"),
        is_dir: true
      }]
    );
    let missing = root.join("missing");
    let err = expect_exit_failure(run_test(LiveFs.read_text(&missing)));
    assert!(
      matches!(&err, InfraError::Io(m) if m.starts_with(&format!("{}: ", missing.display()))),
      "{err:?}"
    );
    assert!(matches!(
      expect_exit_failure(run_test(LiveFs.read_dir(&missing))),
      InfraError::Io(_)
    ));
    let _ = std::fs::remove_dir_all(root);
  }

  #[test]
  fn lazy_store_fails_queries_without_a_database() {
    let store = LazyPathInfoStore::new("/nonexistent/assay-store");
    let opts = QueryOpts {
      include_referrers: false,
      include_closure_size: false,
    };
    let first = expect_exit_failure(run_test(store.query("/nix/store/x-a", opts)));
    assert!(
      matches!(first, nixstore::InfraError::DbMissing(_)),
      "{first:?}"
    );
    // The open error is cached: the second query reports the same error.
    assert_eq!(
      expect_exit_failure(run_test(store.query("/nix/store/x-a", opts))),
      first
    );
  }

  #[test]
  fn lazy_store_opens_a_real_database_once_and_answers_queries() {
    let root =
      std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../nixstore/fixtures/minimal");
    let store = LazyPathInfoStore::new(root);
    let opts = QueryOpts {
      include_referrers: true,
      include_closure_size: true,
    };
    let a = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-a";
    let info = expect_exit_success(run_test(store.query(a, opts)));
    assert_eq!((info.path.as_str(), info.nar_size), (a, 100));
    // The second query reuses the cached connection.
    let again = expect_exit_success(run_test(store.query(a, opts)));
    assert_eq!(again.nar_hash, info.nar_hash);
    assert!(matches!(
      expect_exit_failure(run_test(store.query("/nix/store/missing", opts))),
      nixstore::InfraError::UnknownPath(_)
    ));
  }
}
