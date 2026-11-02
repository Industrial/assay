//! Command programs over the in-memory fakes: no network, no git, no filesystem.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use id_effect::{
  Effect, Executor, Exit, Layer, TestExecutor, assert_exit_fail, expect_exit_failure,
  expect_exit_success, run_test, run_test_on,
};

use crate::caps::{GitService, HttpService, NixfetchEnv, PathIoService, layer_from};
use crate::commands::{
  STAGING_PREFIX, fetch_git, fetch_url, hash, store_path, store_path_report, verify,
};
use crate::error::InfraError;
use crate::fake::{FakeGit, FakeHttp, MemFs, Reply};
use crate::hash_parse::{format_digest, format_nix32};
use crate::ingest::hash_flat_bytes;
use crate::nar::hash_nar;
use crate::nar::tests::{file, fixture_tree};
use crate::resilience::RetryPolicy;

const URL: &str = "https://example.com/blob";
const REPO: &str = "https://git.example/repo.git";
const REV: &str = "deadbeef";
const ZERO_SRI: &str = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

/// Exact timings: 30 s per attempt, 2 retries from 100 ms, no jitter.
const POLICY: RetryPolicy = RetryPolicy {
  attempt_timeout: Duration::from_secs(30),
  retries: 2,
  base_delay: Duration::from_millis(100),
  overall_timeout: Duration::from_secs(600),
  jitter: false,
};

struct World {
  fs: MemFs,
  http: Arc<FakeHttp>,
  git: Arc<FakeGit>,
}

impl World {
  fn new() -> Self {
    let fs = MemFs::new();
    let git = Arc::new(FakeGit::new(&fs));
    Self {
      fs,
      http: Arc::new(FakeHttp::default()),
      git,
    }
  }

  fn layer(&self) -> Layer<NixfetchEnv, InfraError> {
    layer_from(
      HttpService(self.http.clone()),
      GitService(self.git.clone()),
      PathIoService(Arc::new(self.fs.clone())),
    )
  }

  /// Staging dirs still in the filesystem.
  fn staging_dirs(&self) -> Vec<PathBuf> {
    self
      .fs
      .paths()
      .into_iter()
      .filter(|p| {
        p.file_name()
          .is_some_and(|n| n.to_string_lossy().starts_with(STAGING_PREFIX))
      })
      .collect()
  }
}

/// The exit and the virtual time it took.
fn timed<A: Send + 'static>(eff: Effect<A, InfraError>) -> (Exit<A, InfraError>, Duration) {
  let exec = TestExecutor::new();
  let start = exec.now();
  let exit = run_test_on(&exec, eff);
  (exit, exec.now() - start)
}

fn tree_sri() -> String {
  format_digest(&hash_nar(&fixture_tree()))
}

// ── hash / verify ───────────────────────────────────────────────────────────────────────────

#[test]
fn hash_flat_and_recursive_read_through_path_io() {
  let w = World::new();
  w.fs.put_file("/in/hello", b"hello").expect("put");
  w.fs.put("/in/tree", &fixture_tree()).expect("put");
  let flat = expect_exit_success(run_test(
    hash("/in/hello".into(), false).provide_layer(w.layer()),
  ));
  assert_eq!(
    flat.hex,
    "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
  );
  let rec = expect_exit_success(run_test(
    hash("/in/tree".into(), true).provide_layer(w.layer()),
  ));
  assert_eq!(
    rec.nix32,
    "0d0228wgvwp1443zlgfwd88kpzprzbsg5ifr6ikicg76ymwcy7lx"
  );
  assert_eq!(
    rec.render().expect("render"),
    include_str!("../../tests/golden/hash_recursive.stdout").trim_end()
  );
}

#[test]
fn hash_of_a_missing_path_is_an_io_failure() {
  let w = World::new();
  let err = expect_exit_failure(run_test(
    hash("/no/such".into(), false).provide_layer(w.layer()),
  ));
  assert!(matches!(err, InfraError::Io { .. }), "{err:?}");
  assert_eq!(err.exit_code(), 2);
}

#[test]
fn verify_matches_mismatches_and_rejects_bad_hashes() {
  let w = World::new();
  w.fs.put_file("/in/hello", b"hello").expect("put");
  let good = format_digest(&hash_flat_bytes(b"hello"));
  let ok = expect_exit_success(run_test(
    verify("/in/hello".into(), good, false).provide_layer(w.layer()),
  ));
  assert!(ok.matched);
  assert_eq!(
    ok.render().expect("render"),
    format!(
      "{{\n  \"hex\": \"{}\",\n  \"matched\": true,\n  \"nix32\": \"{}\",\n  \"sri\": \"{}\"\n}}",
      ok.hex, ok.nix32, ok.sri
    )
  );

  let bad = expect_exit_failure(run_test(
    verify("/in/hello".into(), ZERO_SRI.into(), false).provide_layer(w.layer()),
  ));
  assert!(matches!(bad, InfraError::HashMismatch { .. }));
  assert_eq!(bad.exit_code(), 1);

  let garbage = expect_exit_failure(run_test(
    verify("/in/hello".into(), "garbage".into(), false).provide_layer(w.layer()),
  ));
  assert!(matches!(garbage, InfraError::Parse(_)));
  assert_eq!(garbage.exit_code(), 2);

  w.fs.put("/in/tree", &fixture_tree()).expect("put");
  let rec_nix32 = format_nix32(&hash_nar(&fixture_tree()));
  assert!(
    expect_exit_success(run_test(
      verify("/in/tree".into(), rec_nix32, true).provide_layer(w.layer())
    ))
    .matched
  );
}

// ── fetch-url ───────────────────────────────────────────────────────────────────────────────

#[test]
fn fetch_url_verifies_then_writes_out() {
  let w = World::new();
  w.http.reply(URL, Reply::Ok(b"body".to_vec()));
  let sri = format_digest(&hash_flat_bytes(b"body"));
  let report = expect_exit_success(run_test(
    fetch_url(
      URL.into(),
      sri.clone(),
      Some("/out/blob".into()),
      Some("pkg".into()),
      POLICY,
    )
    .provide_layer(w.layer()),
  ));
  assert_eq!(report.sri, sri);
  assert_eq!(report.out.as_deref(), Some("/out/blob"));
  assert!(
    report
      .store_path
      .as_deref()
      .is_some_and(|p| p.ends_with("-pkg"))
  );
  assert_eq!(w.fs.get("/out/blob"), Some(file(b"body")));
  let text = report.render().expect("render");
  let keys: Vec<&str> = text
    .lines()
    .filter_map(|l| l.trim().strip_prefix('"')?.split('"').next())
    .collect();
  assert_eq!(
    keys,
    ["hex", "matched", "nix32", "out", "sri", "storePath", "url"]
  );
}

#[test]
fn fetch_url_mismatch_is_not_retried_and_writes_nothing() {
  let w = World::new();
  w.http.reply(URL, Reply::Ok(b"body".to_vec()));
  let (exit, took) = timed(
    fetch_url(
      URL.into(),
      ZERO_SRI.into(),
      Some("/out/blob".into()),
      None,
      POLICY,
    )
    .provide_layer(w.layer()),
  );
  assert!(matches!(
    expect_exit_failure(exit),
    InfraError::HashMismatch { .. }
  ));
  assert_eq!(w.http.calls(), 1);
  assert_eq!(took, Duration::ZERO);
  assert!(!w.fs.exists("/out/blob"));
}

#[test]
fn fetch_url_retries_transient_answers_with_backoff() {
  let w = World::new();
  let unavailable = InfraError::HttpStatus {
    url: URL.into(),
    status: 503,
  };
  w.http
    .reply(URL, Reply::Fail(unavailable.clone()))
    .reply(URL, Reply::Fail(unavailable))
    .reply(URL, Reply::Ok(b"body".to_vec()));
  let sri = format_digest(&hash_flat_bytes(b"body"));
  let (exit, took) = timed(fetch_url(URL.into(), sri, None, None, POLICY).provide_layer(w.layer()));
  expect_exit_success(exit);
  assert_eq!(w.http.calls(), 3);
  assert_eq!(took, Duration::from_millis(100 + 200));
}

#[test]
fn fetch_url_does_not_retry_a_404() {
  let w = World::new();
  let (exit, took) =
    timed(fetch_url(URL.into(), ZERO_SRI.into(), None, None, POLICY).provide_layer(w.layer()));
  assert_exit_fail(
    &exit,
    &InfraError::HttpStatus {
      url: URL.into(),
      status: 404,
    },
  );
  assert_eq!(w.http.calls(), 1);
  assert_eq!(took, Duration::ZERO);
}

#[test]
fn fetch_url_hanging_server_times_out_per_attempt() {
  let w = World::new();
  w.http.reply(URL, Reply::Hang);
  let (exit, took) =
    timed(fetch_url(URL.into(), ZERO_SRI.into(), None, None, POLICY).provide_layer(w.layer()));
  assert_exit_fail(
    &exit,
    &InfraError::Timeout {
      what: format!("GET {URL}"),
      after: Duration::from_secs(30),
    },
  );
  assert_eq!(w.http.calls(), 3);
  assert_eq!(took, Duration::from_secs(90) + Duration::from_millis(300));
}

// ── fetch-git ───────────────────────────────────────────────────────────────────────────────

#[test]
fn fetch_git_moves_the_verified_tree_and_drops_staging() {
  let w = World::new();
  w.git.reply(REPO, REV, Reply::Ok(fixture_tree()));
  let report = expect_exit_success(run_test(
    fetch_git(
      REPO.into(),
      REV.into(),
      tree_sri(),
      None,
      Some("src".into()),
      POLICY,
    )
    .provide_layer(w.layer()),
  ));
  let out = format!("/tmp/nixfetch-git-{}", report.nix32);
  assert_eq!(report.out, out);
  assert_eq!(
    w.fs.get(&out),
    Some(fixture_tree()),
    "verified tree, no .git"
  );
  assert!(w.staging_dirs().is_empty(), "{:?}", w.staging_dirs());
  assert!(
    report
      .store_path
      .as_deref()
      .is_some_and(|p| p.ends_with("-src"))
  );
  assert_eq!((report.url.as_str(), report.rev.as_str()), (REPO, REV));
}

#[test]
fn fetch_git_replaces_an_existing_dest() {
  let w = World::new();
  w.fs.put_file("/work/dest/stale", b"old").expect("put");
  w.git.reply(REPO, REV, Reply::Ok(fixture_tree()));
  let report = expect_exit_success(run_test(
    fetch_git(
      REPO.into(),
      REV.into(),
      tree_sri(),
      Some("/work/dest".into()),
      None,
      POLICY,
    )
    .provide_layer(w.layer()),
  ));
  assert_eq!(report.out, "/work/dest");
  assert_eq!(w.fs.get("/work/dest"), Some(fixture_tree()));
  assert!(w.staging_dirs().is_empty());
}

#[test]
fn fetch_git_mismatch_removes_staging_and_keeps_dest() {
  let w = World::new();
  w.fs.put_file("/work/dest/kept", b"old").expect("put");
  w.git.reply(REPO, REV, Reply::Ok(fixture_tree()));
  let err = expect_exit_failure(run_test(
    fetch_git(
      REPO.into(),
      REV.into(),
      ZERO_SRI.into(),
      Some("/work/dest".into()),
      None,
      POLICY,
    )
    .provide_layer(w.layer()),
  ));
  assert!(matches!(err, InfraError::HashMismatch { .. }));
  assert_eq!(w.git.calls(), 1, "a mismatch is not retried");
  assert!(w.staging_dirs().is_empty(), "{:?}", w.staging_dirs());
  assert_eq!(w.fs.get("/work/dest/kept"), Some(file(b"old")));
}

#[test]
fn fetch_git_retries_a_failed_clone() {
  let w = World::new();
  w.git
    .reply(
      REPO,
      REV,
      Reply::Fail(InfraError::GitClone {
        url: REPO.into(),
        message: "connection reset".into(),
      }),
    )
    .reply(REPO, REV, Reply::Ok(fixture_tree()));
  let (exit, took) = timed(
    fetch_git(REPO.into(), REV.into(), tree_sri(), None, None, POLICY).provide_layer(w.layer()),
  );
  expect_exit_success(exit);
  assert_eq!(w.git.calls(), 2);
  assert_eq!(took, Duration::from_millis(100));
  assert!(w.staging_dirs().is_empty());
}

#[test]
fn fetch_git_timeout_removes_staging() {
  let w = World::new();
  w.git.reply(REPO, REV, Reply::Hang);
  let policy = RetryPolicy {
    overall_timeout: Duration::from_secs(45),
    ..POLICY
  };
  let (exit, took) = timed(
    fetch_git(REPO.into(), REV.into(), tree_sri(), None, None, policy).provide_layer(w.layer()),
  );
  assert!(matches!(
    expect_exit_failure(exit),
    InfraError::Deadline { .. }
  ));
  assert_eq!(took, Duration::from_secs(45));
  assert_eq!(w.git.calls(), 2);
  assert!(w.staging_dirs().is_empty(), "{:?}", w.staging_dirs());
}

#[test]
fn fetch_git_interrupt_removes_staging() {
  let w = World::new();
  w.git.reply(REPO, REV, Reply::Hang);
  let exec = TestExecutor::new();
  let program =
    fetch_git(REPO.into(), REV.into(), tree_sri(), None, None, POLICY).provide_layer(w.layer());
  let handle = program.fork_root(Arc::new(exec.clone()));
  exec.run_until_stalled();
  assert_eq!(w.staging_dirs().len(), 1, "checking out into staging");
  handle.interrupt_now();
  exec.run_until_stalled();
  assert!(handle.is_done());
  assert!(w.staging_dirs().is_empty(), "{:?}", w.staging_dirs());
}

// ── store-path ──────────────────────────────────────────────────────────────────────────────

#[test]
fn store_path_needs_a_full_digest() {
  let report = expect_exit_success(run_test(store_path(
    "hello".into(),
    ZERO_SRI.into(),
    false,
    None,
  )));
  assert_eq!(
    report.render().expect("render"),
    include_str!("../../tests/golden/store_path.stdout").trim_end()
  );
  let compressed = nixdrv::nix_base32_encode(&nixdrv::compress_hash(&hash_flat_bytes(b"x")));
  assert!(matches!(
    store_path_report("n", &compressed, false, None),
    Err(InfraError::Parse(_))
  ));
  let custom = store_path_report("n", ZERO_SRI, true, Some("/tmp/store")).expect("report");
  assert!(custom.path.starts_with("/tmp/store/"));
  assert!(Path::new(&custom.path).ends_with(format!("{}-n", custom.hash)));
}
