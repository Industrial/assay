//! The filesystem capability: suite files, the discover walk and snapshot goldens.
//!
//! Live adapter: [`LiveFs`](crate::live::LiveFs) (`std::fs` on the executor's blocking pool);
//! in-memory fake: `MemFs` in `fake` (`cfg(test)` / `test-support`).

use std::path::{Path, PathBuf};

use id_effect::Effect;

use crate::verdict::InfraError;

/// What is at a path (symlinks followed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
  /// A regular file.
  File,
  /// A directory.
  Dir,
  /// Anything else (e.g. a socket or device).
  Other,
}

/// One entry of a directory listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirEntry {
  /// Full path of the entry.
  pub path: PathBuf,
  /// Whether the entry is (or links to) a directory.
  pub is_dir: bool,
}

/// Filesystem access. Failures are [`InfraError::Io`] carrying `"{path}: {reason}"`; callers
/// add their context with [`reword_io`].
pub trait Fs: Send + Sync {
  /// The UTF-8 text of the file at `path`.
  fn read_text(&self, path: &Path) -> Effect<String, InfraError>;
  /// Writes `text` to `path`, creating missing parent directories.
  fn write_text(&self, path: &Path, text: String) -> Effect<(), InfraError>;
  /// What is at `path`; `None` when nothing is (or it cannot be inspected).
  fn kind(&self, path: &Path) -> Effect<Option<EntryKind>, InfraError>;
  /// The entries of the directory `path`, in no particular order.
  fn read_dir(&self, path: &Path) -> Effect<Vec<DirEntry>, InfraError>;
}

/// Rewords an [`InfraError::Io`] message with `f` (other errors pass through).
pub fn reword_io(e: InfraError, f: impl FnOnce(String) -> InfraError) -> InfraError {
  match e {
    InfraError::Io(message) => f(message),
    other => other,
  }
}
