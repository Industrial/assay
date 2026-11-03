//! nixfetch's capabilities: HTTP, git and filesystem traits, their newtypes, environment aliases
//! and the live layer.
//!
//! Every method returns an effect that needs nothing (`R = HNil`). The live adapters are in
//! [`live`](crate::live); the in-memory fakes (`MemFs`, `FakeHttp`, `FakeGit`) are behind
//! `cfg(test)` / the `test-support` feature.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use id_effect::{Effect, Layer, caps, hlist};

use crate::error::InfraError;
use crate::live::{FsPathIo, LiveGit, LiveHttp};

/// HTTP downloads.
pub trait Http: Send + Sync {
  /// `GET url`: the body of a 2xx answer.
  fn get(&self, url: &str) -> Effect<Vec<u8>, InfraError>;
}

/// The capability effects ask for (`~HttpService`).
#[derive(Clone)]
pub struct HttpService(pub Arc<dyn Http>);

/// Git checkouts.
pub trait Git: Send + Sync {
  /// Clones `url` into `dest` (which must not exist yet) and checks out `rev` there. The
  /// repository's `.git` directory is left in `dest`.
  fn checkout(&self, url: &str, rev: &str, dest: &Path) -> Effect<(), InfraError>;
}

/// The capability effects ask for (`~GitService`).
#[derive(Clone)]
pub struct GitService(pub Arc<dyn Git>);

/// A filesystem tree as the NAR serializer sees it (symlinks are not followed; directory
/// entries are ordered by name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FsNode {
  /// A regular file.
  Regular {
    /// Whether the file has the executable bit set.
    executable: bool,
    /// File contents.
    contents: Vec<u8>,
  },
  /// A directory: entry names mapped to their nodes.
  Directory(BTreeMap<String, FsNode>),
  /// A symbolic link.
  Symlink {
    /// Link target, as stored (not resolved).
    target: String,
  },
}

/// Filesystem access: hashing / NAR reads, download destinations and temp dirs.
pub trait PathIo: Send + Sync {
  /// The bytes of the file at `path`.
  fn read_file(&self, path: &Path) -> Effect<Vec<u8>, InfraError>;
  /// The whole tree at `path` (a file, a directory or a symlink).
  fn read_tree(&self, path: &Path) -> Effect<FsNode, InfraError>;
  /// Writes `bytes` to `path`, creating missing parent directories.
  fn write_file(&self, path: &Path, bytes: Vec<u8>) -> Effect<(), InfraError>;
  /// Creates `path` and any missing parent directories.
  fn create_dir_all(&self, path: &Path) -> Effect<(), InfraError>;
  /// Removes the file, symlink or directory tree at `path`; a missing `path` is not an error.
  fn remove_all(&self, path: &Path) -> Effect<(), InfraError>;
  /// Moves `from` to `to` (same filesystem); `to` must not exist.
  fn rename(&self, from: &Path, to: &Path) -> Effect<(), InfraError>;
  /// The directory for temporary files (`$TMPDIR` on the live filesystem).
  fn temp_root(&self) -> Effect<PathBuf, InfraError>;
  /// Creates a new, uniquely named, empty directory `parent/{prefix}…` and returns its path.
  fn make_temp_dir(&self, parent: &Path, prefix: &str) -> Effect<PathBuf, InfraError>;
}

/// The capability effects ask for (`~PathIoService`).
#[derive(Clone)]
pub struct PathIoService(pub Arc<dyn PathIo>);

/// What `hash` / `verify` need.
pub type PathIoEnv = caps!(PathIoService);
/// What `fetch-url` needs.
pub type FetchUrlEnv = caps!(HttpService, PathIoService);
/// What `fetch-git` needs.
pub type FetchGitEnv = caps!(GitService, PathIoService);
/// Every nixfetch capability (what [`layer`] provides; each command takes the part it needs).
pub type NixfetchEnv = caps!(HttpService, GitService, PathIoService);

/// The live layer: reqwest, the `git` binary and the local filesystem. `E` is the error of the
/// graph it joins; building it never fails.
pub fn layer<E>() -> Layer<NixfetchEnv, E>
where
  E: Send + 'static,
{
  layer_from(
    HttpService(Arc::new(LiveHttp)),
    GitService(Arc::new(LiveGit)),
    PathIoService(Arc::new(FsPathIo)),
  )
}

/// A layer over the given capability values (fakes in tests).
pub fn layer_from<E>(http: HttpService, git: GitService, io: PathIoService) -> Layer<NixfetchEnv, E>
where
  E: Send + 'static,
{
  Layer::succeed(hlist![http, git, io])
}
