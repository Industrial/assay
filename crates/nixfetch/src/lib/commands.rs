//! Effect programs for the nixfetch commands, and the typed outputs they render at the edge.
//!
//! Each command asks for exactly the capabilities it uses: `hash` / `verify` need
//! [`PathIoEnv`], `fetch-url` [`FetchUrlEnv`], `fetch-git` [`FetchGitEnv`], and `store-path`
//! nothing. Hashing, NAR serialization and store-path math are plain Rust.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use id_effect::{Effect, HNil, Never, effect, succeed, suspend};
use nixdrv::DEFAULT_STORE_DIR;
use nixstore::edge::from_result;
use serde::Serialize;

use crate::caps::{
  FetchGitEnv, FetchUrlEnv, Git, GitService, HttpService, PathIo, PathIoEnv, PathIoService,
};
use crate::error::InfraError;
use crate::hash_parse::{
  ExpectedHash, format_digest, format_hex, format_nix32, parse_expected_hash, verify_digest,
};
use crate::ingest::hash_flat_bytes;
use crate::nar::hash_nar;
use crate::resilience::{RetryPolicy, with_retry};
use crate::verify::make_fixed_output_path;

/// Name prefix of the staging directories `fetch-git` checks out into.
pub const STAGING_PREFIX: &str = ".nixfetch-git-";

/// Pretty-printed JSON of a command output (no trailing newline).
fn render_json<T: Serialize>(output: &T) -> Result<String, InfraError> {
  serde_json::to_string_pretty(output).map_err(|e| InfraError::Render(e.to_string()))
}

// Every report keeps its fields in key order: the printed objects keep declaration order and
// match the pre-0.13 (sorted-map) output byte for byte.

/// A sha256 digest in its three spellings (`hash` output).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DigestReport {
  /// Lowercase hex (64 chars).
  pub hex: String,
  /// Nix base32 (52 chars).
  pub nix32: String,
  /// SRI `sha256-<base64>`.
  pub sri: String,
}

impl DigestReport {
  /// All three spellings of `digest`.
  pub fn of(digest: &[u8; 32]) -> Self {
    Self {
      hex: format_hex(digest),
      nix32: format_nix32(digest),
      sri: format_digest(digest),
    }
  }

  /// Pretty-printed JSON (no trailing newline).
  pub fn render(&self) -> Result<String, InfraError> {
    render_json(self)
  }
}

/// `verify` output: the digest that matched.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct VerifyReport {
  /// Digest in lowercase hex.
  pub hex: String,
  /// Always `true` (a mismatch is an error, not a report).
  pub matched: bool,
  /// Digest in Nix base32.
  pub nix32: String,
  /// Digest in SRI form.
  pub sri: String,
}

impl VerifyReport {
  fn matched(digest: &[u8; 32]) -> Self {
    let d = DigestReport::of(digest);
    Self {
      hex: d.hex,
      matched: true,
      nix32: d.nix32,
      sri: d.sri,
    }
  }

  /// Pretty-printed JSON (no trailing newline).
  pub fn render(&self) -> Result<String, InfraError> {
    render_json(self)
  }
}

/// `fetch-url` output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FetchUrlReport {
  /// Flat digest of the download in lowercase hex.
  pub hex: String,
  /// Always `true` (a mismatch is an error, not a report).
  pub matched: bool,
  /// Flat digest in Nix base32.
  pub nix32: String,
  /// Where the verified download was written, if an output path was given.
  #[serde(skip_serializing_if = "Option::is_none")]
  pub out: Option<String>,
  /// Flat digest in SRI form.
  pub sri: String,
  /// Flat fixed-output store path, if a name was given.
  #[serde(rename = "storePath", skip_serializing_if = "Option::is_none")]
  pub store_path: Option<String>,
  /// The downloaded URL.
  pub url: String,
}

impl FetchUrlReport {
  /// Pretty-printed JSON (no trailing newline).
  pub fn render(&self) -> Result<String, InfraError> {
    render_json(self)
  }
}

/// `fetch-git` output; `out` is where the verified tree now is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FetchGitReport {
  /// NAR digest of the tree (without `.git`) in lowercase hex.
  pub hex: String,
  /// Always `true` (a mismatch is an error, not a report).
  pub matched: bool,
  /// NAR digest in Nix base32.
  pub nix32: String,
  /// Directory holding the verified tree.
  pub out: String,
  /// The checked-out revision.
  pub rev: String,
  /// NAR digest in SRI form.
  pub sri: String,
  /// Recursive fixed-output store path, if a name was given.
  #[serde(rename = "storePath", skip_serializing_if = "Option::is_none")]
  pub store_path: Option<String>,
  /// The repository URL.
  pub url: String,
}

impl FetchGitReport {
  /// Pretty-printed JSON (no trailing newline).
  pub fn render(&self) -> Result<String, InfraError> {
    render_json(self)
  }
}

/// `store-path` output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StorePathReport {
  /// The full digest the path was computed from.
  pub digest: DigestReport,
  /// Nix-base32 hash part of the store path.
  pub hash: String,
  /// Name part of the store path.
  pub name: String,
  /// Full store path (under the given or default store directory).
  pub path: String,
}

impl StorePathReport {
  /// Pretty-printed JSON (no trailing newline).
  pub fn render(&self) -> Result<String, InfraError> {
    render_json(self)
  }
}

/// The full store path of `name` for `digest` (`None` without a name).
fn named_store_path(
  name: Option<&str>,
  recursive: bool,
  digest: &[u8; 32],
) -> Result<Option<String>, InfraError> {
  name
    .map(|n| {
      make_fixed_output_path(n, recursive, digest, None).map(|sp| sp.full_path(DEFAULT_STORE_DIR))
    })
    .transpose()
}

/// The sha256 of `path`: of its bytes (flat) or of its NAR (`recursive`).
pub fn digest_path(
  io: Arc<dyn PathIo>,
  path: PathBuf,
  recursive: bool,
) -> Effect<[u8; 32], InfraError> {
  if recursive {
    io.read_tree(&path).map(|tree| hash_nar(&tree))
  } else {
    io.read_file(&path).map(|bytes| hash_flat_bytes(&bytes))
  }
}

/// `hash`: the digest of `path` (flat or recursive).
pub fn hash(path: PathBuf, recursive: bool) -> Effect<DigestReport, InfraError, PathIoEnv> {
  effect!({
    let io = ~PathIoService;
    let digest = ~digest_path(Arc::clone(&io.0), path, recursive);
    DigestReport::of(&digest)
  })
}

/// `verify`: `path` hashes to `expected` (else `HashMismatch`).
pub fn verify(
  path: PathBuf,
  expected: String,
  recursive: bool,
) -> Effect<VerifyReport, InfraError, PathIoEnv> {
  effect!({
    let want = parse_expected_hash(&expected)?;
    let io = ~PathIoService;
    let digest = ~digest_path(Arc::clone(&io.0), path, recursive);
    verify_digest(&want, &digest)?;
    VerifyReport::matched(&digest)
  })
}

/// `fetch-url`: downloads `url` (retried per `policy`), flat-verifies it against `expected`
/// and only then writes it to `out`.
pub fn fetch_url(
  url: String,
  expected: String,
  out: Option<PathBuf>,
  name: Option<String>,
  policy: RetryPolicy,
) -> Effect<FetchUrlReport, InfraError, FetchUrlEnv> {
  effect!({
    let want = parse_expected_hash(&expected)?;
    let http = Arc::clone(&(~HttpService).0);
    let io = Arc::clone(&(~PathIoService).0);
    let target = url.clone();
    let bytes = ~with_retry(format!("GET {url}"), policy, move || http.get(&target));
    let digest = hash_flat_bytes(&bytes);
    verify_digest(&want, &digest)?;
    if let Some(path) = &out {
      ~io.write_file(path, bytes);
    }
    let store_path = named_store_path(name.as_deref(), false, &digest)?;
    let d = DigestReport::of(&digest);
    FetchUrlReport {
      hex: d.hex,
      matched: true,
      nix32: d.nix32,
      out: out.map(|p| p.display().to_string()),
      sri: d.sri,
      store_path,
      url,
    }
  })
}

/// `fetch-git`: checks `url` at `rev` out (retried per `policy`), recursive-verifies the tree
/// (without `.git`) against `expected` and moves it to `dest` (default:
/// `$TMPDIR/nixfetch-git-<nix32>`). See [`checkout_verified`].
pub fn fetch_git(
  url: String,
  rev: String,
  expected: String,
  dest: Option<PathBuf>,
  name: Option<String>,
  policy: RetryPolicy,
) -> Effect<FetchGitReport, InfraError, FetchGitEnv> {
  effect!({
    let want = parse_expected_hash(&expected)?;
    let git = Arc::clone(&(~GitService).0);
    let io = Arc::clone(&(~PathIoService).0);
    let (out, digest) = ~checkout_verified(git, io, url.clone(), rev.clone(), want, dest, policy);
    let store_path = named_store_path(name.as_deref(), true, &digest)?;
    let d = DigestReport::of(&digest);
    FetchGitReport {
      hex: d.hex,
      matched: true,
      nix32: d.nix32,
      out: out.display().to_string(),
      rev,
      sri: d.sri,
      store_path,
      url,
    }
  })
}

/// The directory a staging dir for `dest` goes in (same filesystem, so the final move is a
/// rename).
fn parent_of(dest: &Path) -> PathBuf {
  match dest.parent() {
    Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
    _ => PathBuf::from("."),
  }
}

/// Removes `dir`, ignoring failures (a finalizer cannot fail).
fn remove_quietly(io: Arc<dyn PathIo>, dir: PathBuf) -> Effect<(), Never> {
  io.remove_all(&dir).catch(|_| succeed(()))
}

/// Checks `url@rev` out into a fresh staging directory next to the destination, strips
/// `.git`, NAR-hashes the tree and verifies it against `want`; only a verified tree is moved to
/// `dest` (replacing what was there). The staging directory is a scope finalizer: it is removed
/// however this ends (success, failure, defect or interrupt), so a mismatching or half-cloned
/// tree never survives.
pub fn checkout_verified(
  git: Arc<dyn Git>,
  io: Arc<dyn PathIo>,
  url: String,
  rev: String,
  want: ExpectedHash,
  dest: Option<PathBuf>,
  policy: RetryPolicy,
) -> Effect<(PathBuf, [u8; 32]), InfraError> {
  Effect::scoped(move |scope| -> Effect<(PathBuf, [u8; 32]), InfraError> {
    effect!({
      let parent = match &dest {
        Some(d) => parent_of(d),
        None => ~io.temp_root(),
      };
      ~io.create_dir_all(&parent);
      let staging = ~io.make_temp_dir(&parent, STAGING_PREFIX);
      ~scope.add_finalizer::<InfraError, HNil>(remove_quietly(Arc::clone(&io), staging.clone()));
      let tree = staging.join("tree");
      let attempt = {
        let (git, io, tree, url, rev) = (git, Arc::clone(&io), tree.clone(), url.clone(), rev.clone());
        move || {
          let (git, tree, url, rev) = (Arc::clone(&git), tree.clone(), url.clone(), rev.clone());
          // A failed attempt may leave a partial checkout behind.
          io.remove_all(&tree)
            .flat_map(move |()| git.checkout(&url, &rev, &tree))
        }
      };
      ~with_retry(format!("git checkout {url} {rev}"), policy, attempt);
      ~io.remove_all(&tree.join(".git"));
      let node = ~io.read_tree(&tree);
      let digest = hash_nar(&node);
      verify_digest(&want, &digest)?;
      let target = match dest {
        Some(d) => d,
        None => parent.join(format!("nixfetch-git-{}", format_nix32(&digest))),
      };
      ~io.remove_all(&target);
      ~io.rename(&tree, &target);
      (target, digest)
    })
  })
}

/// The `store-path` output for `name` and a full digest `expected`.
pub fn store_path_report(
  name: &str,
  expected: &str,
  recursive: bool,
  store_dir: Option<&str>,
) -> Result<StorePathReport, InfraError> {
  let digest = match parse_expected_hash(expected)? {
    ExpectedHash::Digest(d) => d,
    ExpectedHash::Compressed(_) => {
      return Err(InfraError::Parse(
        "store-path requires full digest (SRI or hex), not nix32 alone".into(),
      ));
    }
  };
  let sp = make_fixed_output_path(name, recursive, &digest, store_dir)?;
  let dir = store_dir.unwrap_or(DEFAULT_STORE_DIR);
  Ok(StorePathReport {
    digest: DigestReport::of(&digest),
    path: sp.full_path(dir),
    hash: sp.hash,
    name: sp.name,
  })
}

/// `store-path`: needs nothing.
pub fn store_path(
  name: String,
  expected: String,
  recursive: bool,
  store_dir: Option<String>,
) -> Effect<StorePathReport, InfraError> {
  suspend(move || {
    from_result(store_path_report(
      &name,
      &expected,
      recursive,
      store_dir.as_deref(),
    ))
  })
}
