//! In-memory fakes for the capabilities (tests; the `test-support` feature).
//!
//! - [`MemFs`]: a [`PathIo`] over an in-memory tree (cheap handle: clones share the tree).
//! - [`FakeHttp`] / [`FakeGit`]: scripted [`Reply`]s per URL (per URL + rev), counting calls.
//!   [`Reply::Hang`] sleeps on the executor clock (virtual on a `TestExecutor`) so attempt and
//!   overall timeouts can be tested.

// id_effect_lints: test fakes keep their scripted / in-memory state behind a `Mutex` (shared
// with the effects they hand out, not fiber coordination).
#![cfg_attr(dylint_lib = "id_effect_lints", allow(no_mutex_for_fiber_coordination))]

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use id_effect::{Effect, die, fail, sleep, succeed, suspend};
use nixstore::edge::from_result;

use crate::caps::{FsNode, Git, Http, PathIo};
use crate::error::InfraError;

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
  m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// One scripted answer of a fake.
#[derive(Debug, Clone)]
pub enum Reply<T> {
  /// Succeeds with this value.
  Ok(T),
  /// Fails with this error.
  Fail(InfraError),
  /// Never answers (sleeps a virtual day): only a timeout ends the attempt.
  Hang,
}

/// Scripts by key, shared between a fake and the effects it returns.
type Scripts<K, T> = Arc<Mutex<HashMap<K, VecDeque<Reply<T>>>>>;

/// The next reply of a script: replies are used in order and the last one repeats.
fn next_reply<T: Clone>(script: Option<&mut VecDeque<Reply<T>>>) -> Option<Reply<T>> {
  let script = script?;
  if script.len() > 1 {
    script.pop_front()
  } else {
    script.front().cloned()
  }
}

fn answer<T: Send + 'static>(reply: Reply<T>) -> Effect<T, InfraError> {
  match reply {
    Reply::Ok(v) => succeed(v),
    Reply::Fail(e) => fail(e),
    Reply::Hang => sleep(Duration::from_secs(24 * 3600))
      .flat_map(|()| die("fake: a hanging reply outlived every timeout")),
  }
}

// ── HTTP ────────────────────────────────────────────────────────────────────────────────────

/// Scripted HTTP: replies per URL; an unscripted URL answers 404.
#[derive(Default)]
pub struct FakeHttp {
  replies: Scripts<String, Vec<u8>>,
  calls: Arc<AtomicUsize>,
}

impl FakeHttp {
  /// Appends `reply` to `url`'s script.
  pub fn reply(&self, url: impl Into<String>, reply: Reply<Vec<u8>>) -> &Self {
    lock(&self.replies)
      .entry(url.into())
      .or_default()
      .push_back(reply);
    self
  }

  /// How many `get`s ran (attempts, not effects built).
  pub fn calls(&self) -> usize {
    self.calls.load(Ordering::SeqCst)
  }
}

impl Http for FakeHttp {
  fn get(&self, url: &str) -> Effect<Vec<u8>, InfraError> {
    let (replies, calls, url) = (
      Arc::clone(&self.replies),
      Arc::clone(&self.calls),
      url.to_owned(),
    );
    suspend(move || {
      calls.fetch_add(1, Ordering::SeqCst);
      let reply = next_reply(lock(&replies).get_mut(&url));
      answer(reply.unwrap_or(Reply::Fail(InfraError::HttpStatus { url, status: 404 })))
    })
  }
}

// ── git ─────────────────────────────────────────────────────────────────────────────────────

/// Scripted git: replies per `(url, rev)`; `Reply::Ok(tree)` writes `tree` (plus a `.git`
/// directory) into the shared [`MemFs`]. An unscripted repo fails with `GitCheckout`.
pub struct FakeGit {
  fs: MemFs,
  replies: Scripts<(String, String), FsNode>,
  calls: Arc<AtomicUsize>,
}

impl FakeGit {
  /// A fake that checks trees out into `fs`.
  pub fn new(fs: &MemFs) -> Self {
    Self {
      fs: fs.clone(),
      replies: Arc::default(),
      calls: Arc::default(),
    }
  }

  /// Appends `reply` to the script of `url` at `rev`.
  pub fn reply(
    &self,
    url: impl Into<String>,
    rev: impl Into<String>,
    reply: Reply<FsNode>,
  ) -> &Self {
    lock(&self.replies)
      .entry((url.into(), rev.into()))
      .or_default()
      .push_back(reply);
    self
  }

  /// How many checkouts ran.
  pub fn calls(&self) -> usize {
    self.calls.load(Ordering::SeqCst)
  }
}

impl Git for FakeGit {
  fn checkout(&self, url: &str, rev: &str, dest: &Path) -> Effect<(), InfraError> {
    let (fs, replies, calls) = (
      self.fs.clone(),
      Arc::clone(&self.replies),
      Arc::clone(&self.calls),
    );
    let (key, dest) = ((url.to_owned(), rev.to_owned()), dest.to_path_buf());
    suspend(move || {
      calls.fetch_add(1, Ordering::SeqCst);
      let reply = next_reply(lock(&replies).get_mut(&key)).unwrap_or_else(|| {
        Reply::Fail(InfraError::GitCheckout {
          rev: key.1.clone(),
          message: "fake: repository not scripted".into(),
        })
      });
      answer(reply).flat_map(move |tree| from_result(fs.checkout_into(&dest, &tree)))
    })
  }
}

// ── filesystem ──────────────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
enum Entry {
  File { executable: bool, contents: Vec<u8> },
  Dir,
  Symlink(String),
}

#[derive(Default)]
struct Tree {
  entries: BTreeMap<PathBuf, Entry>,
}

fn io_err(path: &Path, message: &str) -> InfraError {
  InfraError::Io {
    path: path.display().to_string(),
    message: format!("fake fs: {message}"),
  }
}

impl Tree {
  fn under<'a>(&'a self, root: &'a Path) -> impl Iterator<Item = &'a PathBuf> + 'a {
    self.entries.keys().filter(move |p| p.starts_with(root))
  }

  /// Creates every missing ancestor of `path` as a directory.
  fn make_parents(&mut self, path: &Path) -> Result<(), InfraError> {
    for dir in path.ancestors().skip(1) {
      if dir.as_os_str().is_empty() {
        continue;
      }
      match self.entries.get(dir) {
        None => {
          self.entries.insert(dir.to_path_buf(), Entry::Dir);
        }
        Some(Entry::Dir) => {}
        Some(_) => return Err(io_err(dir, "not a directory")),
      }
    }
    Ok(())
  }

  fn insert(&mut self, path: &Path, entry: Entry) -> Result<(), InfraError> {
    self.make_parents(path)?;
    match (self.entries.get(path), &entry) {
      (Some(Entry::Dir), Entry::Dir) => return Ok(()),
      (Some(Entry::Dir), _) => return Err(io_err(path, "is a directory")),
      (Some(_), Entry::Dir) => return Err(io_err(path, "exists and is not a directory")),
      _ => {}
    }
    self.entries.insert(path.to_path_buf(), entry);
    Ok(())
  }

  fn insert_node(&mut self, path: &Path, node: &FsNode) -> Result<(), InfraError> {
    match node {
      FsNode::Regular {
        executable,
        contents,
      } => self.insert(
        path,
        Entry::File {
          executable: *executable,
          contents: contents.clone(),
        },
      ),
      FsNode::Symlink { target } => self.insert(path, Entry::Symlink(target.clone())),
      FsNode::Directory(children) => {
        self.insert(path, Entry::Dir)?;
        children
          .iter()
          .try_for_each(|(name, child)| self.insert_node(&path.join(name), child))
      }
    }
  }

  fn node(&self, path: &Path) -> Result<FsNode, InfraError> {
    match self.entries.get(path) {
      None => Err(io_err(path, "not found")),
      Some(Entry::File {
        executable,
        contents,
      }) => Ok(FsNode::Regular {
        executable: *executable,
        contents: contents.clone(),
      }),
      Some(Entry::Symlink(target)) => Ok(FsNode::Symlink {
        target: target.clone(),
      }),
      Some(Entry::Dir) => {
        let children = self
          .entries
          .keys()
          .filter(|p| p.parent() == Some(path))
          .map(|p| {
            let name = p
              .file_name()
              .unwrap_or_default()
              .to_string_lossy()
              .into_owned();
            self.node(p).map(|n| (name, n))
          })
          .collect::<Result<_, _>>()?;
        Ok(FsNode::Directory(children))
      }
    }
  }

  fn remove_all(&mut self, path: &Path) {
    let doomed: Vec<PathBuf> = self.under(path).cloned().collect();
    for p in doomed {
      self.entries.remove(&p);
    }
  }

  fn rename(&mut self, from: &Path, to: &Path) -> Result<(), InfraError> {
    if !self.entries.contains_key(from) {
      return Err(io_err(from, "not found"));
    }
    if self.entries.contains_key(to) {
      return Err(io_err(to, "already exists"));
    }
    self.make_parents(to)?;
    let moved: Vec<PathBuf> = self.under(from).cloned().collect();
    for p in moved {
      if let (Some(entry), Ok(rest)) = (self.entries.remove(&p), p.strip_prefix(from)) {
        self.entries.insert(to.join(rest), entry);
      }
    }
    Ok(())
  }
}

/// In-memory [`PathIo`]. Clones share one tree; `temp_root` is `/tmp`.
#[derive(Clone, Default)]
pub struct MemFs {
  tree: Arc<Mutex<Tree>>,
  next_temp: Arc<AtomicU64>,
}

impl MemFs {
  /// A fresh, empty filesystem.
  pub fn new() -> Self {
    Self::default()
  }

  /// Puts `node` at `path` (creating parents).
  pub fn put(&self, path: impl AsRef<Path>, node: &FsNode) -> Result<(), InfraError> {
    lock(&self.tree).insert_node(path.as_ref(), node)
  }

  /// Puts a non-executable file with `contents` at `path`.
  pub fn put_file(&self, path: impl AsRef<Path>, contents: &[u8]) -> Result<(), InfraError> {
    self.put(
      path,
      &FsNode::Regular {
        executable: false,
        contents: contents.to_vec(),
      },
    )
  }

  /// The tree at `path`, if anything is there.
  pub fn get(&self, path: impl AsRef<Path>) -> Option<FsNode> {
    lock(&self.tree).node(path.as_ref()).ok()
  }

  /// `true` when an entry exists at exactly `path`.
  pub fn exists(&self, path: impl AsRef<Path>) -> bool {
    lock(&self.tree).entries.contains_key(path.as_ref())
  }

  /// Every path in the filesystem, sorted.
  pub fn paths(&self) -> Vec<PathBuf> {
    lock(&self.tree).entries.keys().cloned().collect()
  }

  /// What [`FakeGit`] does on success: `tree` plus a `.git` directory at `dest`.
  fn checkout_into(&self, dest: &Path, tree: &FsNode) -> Result<(), InfraError> {
    let mut t = lock(&self.tree);
    if t.entries.contains_key(dest) {
      return Err(io_err(dest, "checkout destination already exists"));
    }
    t.insert_node(dest, tree)?;
    t.insert(
      &dest.join(".git/HEAD"),
      Entry::File {
        executable: false,
        contents: b"ref: refs/heads/main\n".to_vec(),
      },
    )
  }

  /// `op` on the locked tree, run when the effect runs.
  fn with<A: Send + 'static>(
    &self,
    op: impl FnOnce(&mut Tree) -> Result<A, InfraError> + Send + 'static,
  ) -> Effect<A, InfraError> {
    let tree = Arc::clone(&self.tree);
    suspend(move || from_result(op(&mut lock(&tree))))
  }
}

impl PathIo for MemFs {
  fn read_file(&self, path: &Path) -> Effect<Vec<u8>, InfraError> {
    let path = path.to_path_buf();
    self.with(move |t| match t.entries.get(&path) {
      Some(Entry::File { contents, .. }) => Ok(contents.clone()),
      Some(_) => Err(io_err(&path, "not a regular file")),
      None => Err(io_err(&path, "not found")),
    })
  }

  fn read_tree(&self, path: &Path) -> Effect<FsNode, InfraError> {
    let path = path.to_path_buf();
    self.with(move |t| t.node(&path))
  }

  fn write_file(&self, path: &Path, bytes: Vec<u8>) -> Effect<(), InfraError> {
    let path = path.to_path_buf();
    self.with(move |t| {
      t.insert(
        &path,
        Entry::File {
          executable: false,
          contents: bytes,
        },
      )
    })
  }

  fn create_dir_all(&self, path: &Path) -> Effect<(), InfraError> {
    let path = path.to_path_buf();
    self.with(move |t| t.insert(&path, Entry::Dir))
  }

  fn remove_all(&self, path: &Path) -> Effect<(), InfraError> {
    let path = path.to_path_buf();
    self.with(move |t| {
      t.remove_all(&path);
      Ok(())
    })
  }

  fn rename(&self, from: &Path, to: &Path) -> Effect<(), InfraError> {
    let (from, to) = (from.to_path_buf(), to.to_path_buf());
    self.with(move |t| t.rename(&from, &to))
  }

  fn temp_root(&self) -> Effect<PathBuf, InfraError> {
    succeed(PathBuf::from("/tmp"))
  }

  fn make_temp_dir(&self, parent: &Path, prefix: &str) -> Effect<PathBuf, InfraError> {
    let next = Arc::clone(&self.next_temp);
    let dir = parent.to_path_buf();
    let prefix = prefix.to_owned();
    self.with(move |t| {
      if !matches!(t.entries.get(&dir), Some(Entry::Dir)) {
        return Err(io_err(&dir, "parent directory not found"));
      }
      let path = dir.join(format!("{prefix}{}", next.fetch_add(1, Ordering::SeqCst)));
      t.insert(&path, Entry::Dir)?;
      Ok(path)
    })
  }
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;

  use id_effect::{expect_exit_failure, expect_exit_success, run_test};

  use super::*;

  fn file(contents: &[u8]) -> FsNode {
    FsNode::Regular {
      executable: false,
      contents: contents.to_vec(),
    }
  }

  #[test]
  fn mem_fs_tree_ops() {
    let fs = MemFs::new();
    fs.put_file("/w/a/x", b"x").expect("put");
    expect_exit_success(run_test(fs.write_file(Path::new("/w/b/y"), b"y".to_vec())));
    assert_eq!(
      expect_exit_success(run_test(fs.read_file(Path::new("/w/b/y")))),
      b"y"
    );
    let tree = expect_exit_success(run_test(fs.read_tree(Path::new("/w"))));
    let FsNode::Directory(top) = &tree else {
      panic!("{tree:?}")
    };
    assert_eq!(top.keys().collect::<Vec<_>>(), ["a", "b"]);

    let t = expect_exit_success(run_test(fs.make_temp_dir(Path::new("/w"), ".s-")));
    expect_exit_success(run_test(fs.rename(Path::new("/w/a"), &t.join("a"))));
    assert_eq!(fs.get(t.join("a/x")), Some(file(b"x")));
    assert!(!fs.exists("/w/a"));
    expect_exit_success(run_test(fs.remove_all(&t)));
    assert!(!fs.exists(&t) && !fs.exists(t.join("a/x")));

    assert!(matches!(
      expect_exit_failure(run_test(fs.read_file(Path::new("/w")))),
      InfraError::Io { .. }
    ));
    assert!(matches!(
      expect_exit_failure(run_test(fs.make_temp_dir(Path::new("/nope"), "p"))),
      InfraError::Io { .. }
    ));
  }

  #[test]
  fn scripts_replay_in_order_and_repeat_the_last() {
    let http = FakeHttp::default();
    http
      .reply(
        "u",
        Reply::Fail(InfraError::HttpStatus {
          url: "u".into(),
          status: 503,
        }),
      )
      .reply("u", Reply::Ok(b"body".to_vec()));
    assert!(expect_exit_failure(run_test(http.get("u"))).is_transient());
    assert_eq!(expect_exit_success(run_test(http.get("u"))), b"body");
    assert_eq!(expect_exit_success(run_test(http.get("u"))), b"body");
    assert_eq!(
      expect_exit_failure(run_test(http.get("other"))),
      InfraError::HttpStatus {
        url: "other".into(),
        status: 404
      }
    );
    assert_eq!(http.calls(), 4);
  }

  #[test]
  fn fake_git_writes_tree_and_dot_git() {
    let fs = MemFs::new();
    let git = Arc::new(FakeGit::new(&fs));
    git.reply(
      "r",
      "v1",
      Reply::Ok(FsNode::Directory([("f".into(), file(b"1"))].into())),
    );
    expect_exit_success(run_test(git.checkout("r", "v1", Path::new("/co"))));
    assert_eq!(fs.get("/co/f"), Some(file(b"1")));
    assert!(fs.exists("/co/.git/HEAD"));
    assert!(matches!(
      expect_exit_failure(run_test(git.checkout("r", "v2", Path::new("/co2")))),
      InfraError::GitCheckout { .. }
    ));
  }
}
