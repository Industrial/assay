//! Discover assay suite files under a directory tree (through the [`Fs`] capability).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use id_effect::{Concurrency, Effect, all, need, succeed};

use crate::caps::{FsEnv, FsService};
use crate::files::{EntryKind, Fs, reword_io};
use crate::verdict::InfraError;

/// A discovered suite file and its kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SuiteFile {
  /// Path of the suite file.
  pub path: PathBuf,
  /// How the file is loaded.
  pub kind: SuiteKind,
}

/// Kind of suite file discovered on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SuiteKind {
  /// Compat JSON suite (`suite.json`, or `.json` under a compat fixtures dir).
  CompatJson,
  /// Compat Nix suite (`.nix` under a compat fixtures dir).
  CompatNix,
  /// Native assay suite (`*.assay.nix` or `*.assay.json`).
  AssayNix,
}

/// [`discover_suites`] with the filesystem from [`FsEnv`].
pub fn discover(root: &Path) -> Effect<Vec<SuiteFile>, InfraError, FsEnv> {
  let root = root.to_path_buf();
  need::<FsService>().flat_map(move |fs: Arc<FsService>| discover_suites(&fs.0, &root).any_env())
}

/// The suite files at `root`: `root` itself when it is a suite file, else every suite file
/// under it (recursively, skipping caches / VCS / build outputs), sorted by path.
pub fn discover_suites(fs: &Arc<dyn Fs>, root: &Path) -> Effect<Vec<SuiteFile>, InfraError> {
  let (fs, root) = (Arc::clone(fs), root.to_path_buf());
  fs.kind(&root).flat_map(move |kind| {
    if kind == Some(EntryKind::File) {
      let suite = suite_kind(&root).map(|kind| SuiteFile { path: root, kind });
      return succeed(suite.into_iter().collect());
    }
    walk_dir(fs, root).map(|mut suites| {
      suites.sort_by(|a, b| a.path.cmp(&b.path));
      suites
    })
  })
}

/// Classify a single path as a suite file, if it matches a known pattern.
pub fn suite_kind(path: &Path) -> Option<SuiteKind> {
  let file_name = path.file_name().and_then(|n| n.to_str())?;

  if file_name == "suite.json" {
    return Some(SuiteKind::CompatJson);
  }
  if file_name.ends_with(".assay.nix") || file_name.ends_with(".assay.json") {
    return Some(SuiteKind::AssayNix);
  }
  if is_fixtures_compat_child(path) {
    if file_name.ends_with(".json") {
      return Some(SuiteKind::CompatJson);
    }
    if file_name.ends_with(".nix") {
      return Some(SuiteKind::CompatNix);
    }
  }
  None
}

/// The suite files under `dir` (unsorted).
fn walk_dir(fs: Arc<dyn Fs>, dir: PathBuf) -> Effect<Vec<SuiteFile>, InfraError> {
  fs.read_dir(&dir)
    .map_err(|e| reword_io(e, |m| InfraError::Io(format!("read dir {m}"))))
    .flat_map(move |entries| {
      let mut suites = Vec::new();
      let mut subdirs = Vec::new();
      for entry in entries {
        if entry.is_dir {
          let name = entry
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
          if !should_skip_dir(name) {
            subdirs.push(walk_dir(Arc::clone(&fs), entry.path));
          }
        } else if let Some(kind) = suite_kind(&entry.path) {
          suites.push(SuiteFile {
            path: entry.path,
            kind,
          });
        }
      }
      all(subdirs, Concurrency::Sequential).map(move |nested| {
        suites.extend(nested.into_iter().flatten());
        suites
      })
    })
}

/// Directories excluded from whole-repo discovery (caches, VCS, build outputs).
fn should_skip_dir(name: &str) -> bool {
  matches!(
    name,
    ".git"
      | ".devenv"
      | ".direnv"
      | ".moon"
      | ".cache"
      | ".hermes"
      | "node_modules"
      | "target"
      | "result"
      | "__pycache__"
      | "vendor"
  ) || name.starts_with("target-")
    || name.starts_with("result-")
}

fn is_fixtures_compat_child(path: &Path) -> bool {
  let compat = path.parent();
  let fixtures = compat.and_then(|p| p.parent());
  compat.is_some_and(|p| p.file_name().is_some_and(|n| n == "compat"))
    && fixtures.is_some_and(|p| p.file_name().is_some_and(|n| n == "fixtures"))
}

#[cfg(test)]
mod tests {
  use id_effect::{Layer, expect_exit_failure, expect_exit_success, hlist, run_test};

  use super::*;
  use crate::fake::MemFs;

  fn discover_in(files: &[&str], root: &str) -> Vec<SuiteFile> {
    let fs: Arc<dyn Fs> = Arc::new(MemFs::with_files(files.iter().map(|f| (*f, "#"))));
    expect_exit_success(run_test(discover_suites(&fs, Path::new(root))))
  }

  fn paths(found: &[SuiteFile]) -> Vec<String> {
    found.iter().map(|s| s.path.display().to_string()).collect()
  }

  #[test]
  fn suite_kind_recognizes_patterns() {
    let compat_json = Path::new("fixtures/compat/pass.json");
    let suite_json = Path::new("nested/suite.json");
    let assay = Path::new("lib/foo.assay.nix");
    let compat_nix = Path::new("fixtures/compat/pass.nix");

    assert_eq!(suite_kind(compat_json), Some(SuiteKind::CompatJson));
    assert_eq!(suite_kind(suite_json), Some(SuiteKind::CompatJson));
    assert_eq!(suite_kind(assay), Some(SuiteKind::AssayNix));
    assert_eq!(suite_kind(compat_nix), Some(SuiteKind::CompatNix));
    assert_eq!(
      suite_kind(Path::new("x.assay.json")),
      Some(SuiteKind::AssayNix)
    );
    assert_eq!(suite_kind(Path::new("fixtures/compat/foo.txt")), None);
    assert_eq!(suite_kind(Path::new("other/foo.json")), None);
  }

  #[test]
  fn should_skip_dir_ignores_caches_and_vcs() {
    assert!(should_skip_dir(".git"));
    assert!(should_skip_dir("node_modules"));
    assert!(should_skip_dir("target-ci-stable"));
    assert!(should_skip_dir("result-2"));
    assert!(!should_skip_dir("common"));
    assert!(!should_skip_dir("features"));
  }

  #[test]
  fn discover_skips_nested_git_and_node_modules() {
    let found = discover_in(
      &[
        "/r/keep/demo.assay.nix",
        "/r/.git/hidden.assay.nix",
        "/r/node_modules/pkg/hidden.assay.nix",
        "/r/target/hidden.assay.nix",
      ],
      "/r",
    );
    assert_eq!(paths(&found), ["/r/keep/demo.assay.nix"]);
  }

  #[test]
  fn discover_suites_finds_nested_files_sorted() {
    let found = discover_in(
      &[
        "/r/fixtures/compat/two.nix",
        "/r/fixtures/compat/one.json",
        "/r/deep/suite.json",
        "/r/cases/demo.assay.nix",
        "/r/junk/readme.txt",
      ],
      "/r",
    );
    assert_eq!(
      found,
      [
        SuiteFile {
          path: "/r/cases/demo.assay.nix".into(),
          kind: SuiteKind::AssayNix
        },
        SuiteFile {
          path: "/r/deep/suite.json".into(),
          kind: SuiteKind::CompatJson
        },
        SuiteFile {
          path: "/r/fixtures/compat/one.json".into(),
          kind: SuiteKind::CompatJson
        },
        SuiteFile {
          path: "/r/fixtures/compat/two.nix".into(),
          kind: SuiteKind::CompatNix
        },
      ]
    );
  }

  #[test]
  fn discover_single_files() {
    assert!(discover_in(&["/d/notsuite.txt"], "/d/notsuite.txt").is_empty());
    let json = discover_in(&["/d/suite.json"], "/d/suite.json");
    assert_eq!(json.len(), 1);
    assert_eq!(json[0].kind, SuiteKind::CompatJson);
    let assay = discover_in(&["/d/x.assay.nix"], "/d/x.assay.nix");
    assert_eq!(assay[0].kind, SuiteKind::AssayNix);
  }

  #[test]
  fn discover_missing_root_is_a_read_dir_error() {
    let fs: Arc<dyn Fs> = Arc::new(MemFs::default());
    let err = expect_exit_failure(run_test(discover_suites(&fs, Path::new("/nope"))));
    assert_eq!(
      err,
      InfraError::Io("read dir /nope: not a directory".into())
    );
  }

  #[test]
  fn discover_reads_the_fs_from_its_env() {
    let fs = MemFs::with_files([("/r/a.assay.nix", "#")]);
    let layer = Layer::<FsEnv, InfraError>::succeed(hlist![FsService(Arc::new(fs))]);
    let found = expect_exit_success(run_test(discover(Path::new("/r")).provide_layer(layer)));
    assert_eq!(paths(&found), ["/r/a.assay.nix"]);
  }
}
