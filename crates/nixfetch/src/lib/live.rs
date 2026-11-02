//! Live adapters: async reqwest (rustls), the `git` binary through `tokio::process`, and
//! `std::fs`. This is the only module that touches the network, subprocesses or the filesystem.
//!
//! # Where the work runs
//!
//! `run_main` polls the root fiber on the calling (main) thread, outside the tokio runtime
//! context, while forked fibers (every `timeout` / `race` racer) run on tokio worker threads.
//! So the adapters never do their I/O inline in the fiber:
//!
//! - network and subprocess futures run as a task on the fiber's executor ([`on_executor`]):
//!   on the default runtime that is tokio, whose context reqwest and `tokio::process` need.
//!   Interrupting the effect (an attempt timeout, Ctrl-C) drops the future inside the task,
//!   which closes the connection or kills the `git` child (`kill_on_drop`);
//! - blocking `std::fs` calls run on the executor's blocking pool ([`blocking`]), so they never
//!   park an executor thread. On a `TestExecutor` they run as an ordinary task.

// id_effect_lints: this is the I/O boundary. Its async helpers run only inside `on_executor`
// tasks; `settle` / `on_executor` / `blocking` pass any `E` through unchanged (no `From` to
// apply); `TEMP_SEQ` is a temp-name counter, not a service.
#![cfg_attr(
  dylint_lib = "id_effect_lints",
  allow(
    no_async_fn_in_effect_crate,
    effect_error_bound_must_use_from,
    no_static_service_globals
  )
)]

use std::collections::BTreeMap;
use std::error::Error as _;
use std::ffi::OsStr;
use std::fs;
use std::future::{Future, poll_fn};
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::pin::{Pin, pin};
use std::process::Stdio;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::Poll;

use id_effect::{Ctx, Effect, HNil, die, fail, succeed};
use tokio::sync::oneshot;

use crate::caps::{FsNode, Git, Http, PathIo};
use crate::error::InfraError;

/// The outcome a task hands back: `None` when it ended without one (it panicked).
fn settle<A, E>(out: Option<Result<A, E>>) -> Effect<A, E>
where
  A: Send + 'static,
  E: Send + 'static,
{
  match out {
    Some(Ok(a)) => succeed(a),
    Some(Err(e)) => fail(e),
    None => die("nixfetch: an I/O task ended without a result"),
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

// ── HTTP ────────────────────────────────────────────────────────────────────────────────────

/// HTTP over an async reqwest client (rustls, HTTP/1.1).
#[derive(Debug, Default, Clone, Copy)]
pub struct LiveHttp;

impl Http for LiveHttp {
  fn get(&self, url: &str) -> Effect<Vec<u8>, InfraError> {
    let url = url.to_owned();
    on_executor(move || http_get(url))
  }
}

async fn http_get(url: String) -> Result<Vec<u8>, InfraError> {
  let client = reqwest::Client::builder()
    .build()
    .map_err(|e| request_error(&url, &e))?;
  let response = client
    .get(&url)
    .send()
    .await
    .map_err(|e| request_error(&url, &e))?;
  let status = response.status();
  if !status.is_success() {
    return Err(InfraError::HttpStatus {
      url,
      status: status.as_u16(),
    });
  }
  match response.bytes().await {
    Ok(body) => Ok(body.to_vec()),
    Err(e) => Err(request_error(&url, &e)),
  }
}

/// A reqwest error with its source chain (e.g. "... : connection refused"); building the
/// request (an invalid URL) is permanent, everything else transient.
fn request_error(url: &str, e: &reqwest::Error) -> InfraError {
  let mut message = e.to_string();
  let mut source = e.source();
  while let Some(s) = source {
    message.push_str(": ");
    message.push_str(&s.to_string());
    source = s.source();
  }
  if e.is_builder() {
    InfraError::HttpRequest {
      url: url.into(),
      message,
    }
  } else {
    InfraError::Http {
      url: url.into(),
      message,
    }
  }
}

// ── git ─────────────────────────────────────────────────────────────────────────────────────

/// The `git` binary on `PATH`, run through `tokio::process` (killed when the attempt is
/// dropped). Its output is captured; stderr goes into the error on failure.
#[derive(Debug, Default, Clone, Copy)]
pub struct LiveGit;

impl Git for LiveGit {
  fn checkout(&self, url: &str, rev: &str, dest: &Path) -> Effect<(), InfraError> {
    let (url, rev, dest) = (url.to_owned(), rev.to_owned(), dest.to_path_buf());
    on_executor(move || git_checkout(url, rev, dest))
  }
}

async fn git_checkout(url: String, rev: String, dest: PathBuf) -> Result<(), InfraError> {
  let dest = dest.as_os_str();
  let shallow = [
    OsStr::new("clone"),
    "--depth".as_ref(),
    "1".as_ref(),
    url.as_ref(),
    dest,
  ];
  if git(&shallow).await?.is_err() {
    // Retry without `--depth` (git removes the directory a failed clone created).
    let full = [OsStr::new("clone"), url.as_ref(), dest];
    git(&full).await?.map_err(|message| InfraError::GitClone {
      url: url.clone(),
      message,
    })?;
  }
  let checkout = [OsStr::new("-C"), dest, "checkout".as_ref(), rev.as_ref()];
  git(&checkout)
    .await?
    .map_err(|message| InfraError::GitCheckout { rev, message })
}

/// Runs `git args`: `Err` when git cannot be started, `Ok(Err(stderr))` when it fails.
async fn git(args: &[&OsStr]) -> Result<Result<(), String>, InfraError> {
  let out = tokio::process::Command::new("git")
    .args(args)
    .stdin(Stdio::null())
    .kill_on_drop(true)
    .output()
    .await
    .map_err(|e| InfraError::GitSpawn(e.to_string()))?;
  if out.status.success() {
    return Ok(Ok(()));
  }
  let stderr = String::from_utf8_lossy(&out.stderr).trim().to_owned();
  Ok(Err(if stderr.is_empty() {
    format!("git exited with {}", out.status)
  } else {
    stderr
  }))
}

// ── filesystem ──────────────────────────────────────────────────────────────────────────────

/// The local filesystem (`std::fs` on the executor's blocking pool).
#[derive(Debug, Default, Clone, Copy)]
pub struct FsPathIo;

fn io_err(path: &Path, e: &io::Error) -> InfraError {
  InfraError::Io {
    path: path.display().to_string(),
    message: e.to_string(),
  }
}

fn read_node(path: &Path) -> Result<FsNode, InfraError> {
  let meta = fs::symlink_metadata(path).map_err(|e| io_err(path, &e))?;
  let ft = meta.file_type();
  if ft.is_symlink() {
    let target = fs::read_link(path).map_err(|e| io_err(path, &e))?;
    return Ok(FsNode::Symlink {
      target: target.to_string_lossy().into_owned(),
    });
  }
  if ft.is_dir() {
    let mut entries = BTreeMap::new();
    for entry in fs::read_dir(path).map_err(|e| io_err(path, &e))? {
      let entry = entry.map_err(|e| io_err(path, &e))?;
      let name = entry.file_name().to_string_lossy().into_owned();
      entries.insert(name, read_node(&entry.path())?);
    }
    return Ok(FsNode::Directory(entries));
  }
  // Regular files (and fifos / sockets / devices, read as regular files).
  Ok(FsNode::Regular {
    executable: meta.permissions().mode() & 0o111 != 0,
    contents: fs::read(path).map_err(|e| io_err(path, &e))?,
  })
}

fn remove_all(path: &Path) -> Result<(), InfraError> {
  let removed = match fs::symlink_metadata(path) {
    Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(()),
    Err(e) => Err(e),
    Ok(meta) if meta.is_dir() => fs::remove_dir_all(path),
    Ok(_) => fs::remove_file(path),
  };
  removed.map_err(|e| io_err(path, &e))
}

/// Uniquifies temp dir names within this process (no clock involved).
static TEMP_SEQ: AtomicU64 = AtomicU64::new(0);

fn make_temp_dir(parent: &Path, prefix: &str) -> Result<PathBuf, InfraError> {
  let pid = std::process::id();
  loop {
    let n = TEMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let dir = parent.join(format!("{prefix}{pid}-{n}"));
    match fs::create_dir(&dir) {
      Ok(()) => return Ok(dir),
      // Left over by an earlier process with the same pid: try the next name.
      Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
      Err(e) => return Err(io_err(&dir, &e)),
    }
  }
}

impl PathIo for FsPathIo {
  fn read_file(&self, path: &Path) -> Effect<Vec<u8>, InfraError> {
    let path = path.to_path_buf();
    blocking(move || fs::read(&path).map_err(|e| io_err(&path, &e)))
  }

  fn read_tree(&self, path: &Path) -> Effect<FsNode, InfraError> {
    let path = path.to_path_buf();
    blocking(move || read_node(&path))
  }

  fn write_file(&self, path: &Path, bytes: Vec<u8>) -> Effect<(), InfraError> {
    let path = path.to_path_buf();
    blocking(move || {
      if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| io_err(parent, &e))?;
      }
      fs::write(&path, bytes).map_err(|e| io_err(&path, &e))
    })
  }

  fn create_dir_all(&self, path: &Path) -> Effect<(), InfraError> {
    let path = path.to_path_buf();
    blocking(move || fs::create_dir_all(&path).map_err(|e| io_err(&path, &e)))
  }

  fn remove_all(&self, path: &Path) -> Effect<(), InfraError> {
    let path = path.to_path_buf();
    blocking(move || remove_all(&path))
  }

  fn rename(&self, from: &Path, to: &Path) -> Effect<(), InfraError> {
    let (from, to) = (from.to_path_buf(), to.to_path_buf());
    blocking(move || fs::rename(&from, &to).map_err(|e| io_err(&to, &e)))
  }

  fn temp_root(&self) -> Effect<PathBuf, InfraError> {
    blocking(|| Ok(std::env::temp_dir()))
  }

  fn make_temp_dir(&self, parent: &Path, prefix: &str) -> Effect<PathBuf, InfraError> {
    let (parent, prefix) = (parent.to_path_buf(), prefix.to_owned());
    blocking(move || make_temp_dir(&parent, &prefix))
  }
}

#[cfg(test)]
mod tests {
  //! The live adapters against the real filesystem, a local HTTP server and a local git repo.
  //! Network / git adapters run on the default runtime (`block_on`): their futures need tokio.

  use std::io::{Read, Write};
  use std::net::TcpListener;
  use std::process::Command;
  use std::thread;

  use id_effect::{assert_exit_fail, expect_exit_failure, expect_exit_success, run_test};

  use super::*;

  fn scratch(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("nixfetch-live-{tag}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("scratch dir");
    dir
  }

  /// Serves one canned response per connection, `responses.len()` connections, then stops.
  // A blocking test HTTP server on its own thread, outside any effect.
  #[cfg_attr(dylint_lib = "id_effect_lints", allow(no_thread_spawn))]
  fn serve(responses: Vec<Vec<u8>>) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind");
    let addr = listener.local_addr().expect("addr");
    thread::spawn(move || {
      for response in responses {
        let Ok((mut stream, _)) = listener.accept() else {
          return;
        };
        let mut buf = [0u8; 1024];
        let _ = stream.read(&mut buf);
        let _ = stream.write_all(&response);
      }
    });
    format!("http://{addr}/blob")
  }

  fn response(status: &str, body: &[u8]) -> Vec<u8> {
    let mut r = format!(
      "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
      body.len()
    )
    .into_bytes();
    r.extend_from_slice(body);
    r
  }

  #[test]
  fn live_http_gets_body_and_classifies_failures() {
    let url = serve(vec![
      response("200 OK", b"hello-live"),
      response("404 Not Found", b"nope"),
      response("503 Service Unavailable", b"busy"),
    ]);
    assert_eq!(
      expect_exit_success(LiveHttp.get(&url).block_on()),
      b"hello-live"
    );
    let not_found = expect_exit_failure(LiveHttp.get(&url).block_on());
    assert!(!not_found.is_transient(), "{not_found:?}");
    assert_eq!(
      not_found,
      InfraError::HttpStatus {
        url: url.clone(),
        status: 404
      }
    );
    assert!(expect_exit_failure(LiveHttp.get(&url).block_on()).is_transient());

    let refused = expect_exit_failure(LiveHttp.get("http://127.0.0.1:1/closed").block_on());
    assert!(
      matches!(refused, InfraError::Http { .. }) && refused.is_transient(),
      "{refused:?}"
    );
    let invalid = expect_exit_failure(LiveHttp.get("not a url").block_on());
    assert!(
      matches!(invalid, InfraError::HttpRequest { .. }),
      "{invalid:?}"
    );
  }

  #[test]
  fn live_git_checks_out_a_local_repo() {
    let root = scratch("git");
    let repo = root.join("repo");
    fs::create_dir_all(&repo).expect("repo dir");
    let run = |args: &[&str]| {
      let out = Command::new("git")
        .args(args)
        .current_dir(&repo)
        .output()
        .expect("run git");
      assert!(out.status.success(), "git {args:?}: {out:?}");
      String::from_utf8_lossy(&out.stdout).trim().to_owned()
    };
    run(&["init", "-q"]);
    run(&["config", "user.email", "live@example.com"]);
    run(&["config", "user.name", "live"]);
    fs::write(repo.join("tracked.txt"), b"git-live").expect("write");
    run(&["add", "tracked.txt"]);
    run(&["commit", "-q", "-m", "c"]);
    let rev = run(&["rev-parse", "HEAD"]);
    let url = format!("file://{}", repo.display());

    let dest = root.join("out");
    expect_exit_success(LiveGit.checkout(&url, &rev, &dest).block_on());
    assert_eq!(
      fs::read(dest.join("tracked.txt")).expect("read"),
      b"git-live"
    );
    assert!(dest.join(".git").is_dir(), "checkout leaves .git");

    let bad_rev = LiveGit.checkout(&url, &"0".repeat(40), &root.join("bad-rev"));
    assert!(matches!(
      expect_exit_failure(bad_rev.block_on()),
      InfraError::GitCheckout { .. }
    ));
    let no_repo = LiveGit.checkout("file:///nonexistent/nixfetch", "HEAD", &root.join("x"));
    assert!(matches!(
      expect_exit_failure(no_repo.block_on()),
      InfraError::GitClone { .. }
    ));
    let _ = fs::remove_dir_all(&root);
  }

  #[test]
  fn fs_path_io_roundtrip_tree_and_temp_dirs() {
    let root = scratch("fs");
    let io = FsPathIo;
    let file = root.join("a/b/f");
    expect_exit_success(run_test(io.write_file(&file, b"hi".to_vec())));
    assert_eq!(expect_exit_success(run_test(io.read_file(&file))), b"hi");
    std::os::unix::fs::symlink("b/f", root.join("a/link")).expect("symlink");
    let tool = root.join("a/tool");
    fs::write(&tool, b"#!").expect("write");
    fs::set_permissions(&tool, fs::Permissions::from_mode(0o755)).expect("chmod");

    let tree = expect_exit_success(run_test(io.read_tree(&root.join("a"))));
    let FsNode::Directory(entries) = tree else {
      panic!("not a directory: {tree:?}")
    };
    assert_eq!(
      entries.keys().map(String::as_str).collect::<Vec<_>>(),
      ["b", "link", "tool"]
    );
    assert_eq!(
      entries["link"],
      FsNode::Symlink {
        target: "b/f".into()
      }
    );
    assert!(matches!(
      entries["tool"],
      FsNode::Regular {
        executable: true,
        ..
      }
    ));

    let t1 = expect_exit_success(run_test(io.make_temp_dir(&root, ".stage-")));
    let t2 = expect_exit_success(run_test(io.make_temp_dir(&root, ".stage-")));
    assert_ne!(t1, t2);
    assert!(t1.is_dir() && t2.is_dir());
    expect_exit_success(run_test(io.rename(&t1, &root.join("moved"))));
    assert!(!t1.exists() && root.join("moved").is_dir());

    expect_exit_success(run_test(io.remove_all(&root.join("a"))));
    assert!(!root.join("a").exists());
    expect_exit_success(run_test(io.remove_all(&root.join("a"))));

    let missing = root.join("missing");
    assert_exit_fail(
      &run_test(io.read_file(&missing)),
      &InfraError::Io {
        path: missing.display().to_string(),
        message: "No such file or directory (os error 2)".into(),
      },
    );
    assert!(matches!(
      expect_exit_failure(run_test(io.read_tree(&missing))),
      InfraError::Io { .. }
    ));
    let _ = fs::remove_dir_all(&root);
  }
}
