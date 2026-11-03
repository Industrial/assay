//! Ctrl-C on the real binary: exit 130, the nix child dies, and the batch temp dir is removed.
//!
//! This is the subprocess tier, so it waits on a real process: it polls (bounded) for the
//! batch directory to appear before signalling, and for the process to exit afterwards.

use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const BIN: &str = env!("CARGO_BIN_EXE_assay");

fn has_program(program: &str) -> bool {
  Command::new(program)
    .arg("--version")
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .is_ok_and(|s| s.success())
}

fn batch_dirs(tmp: &Path) -> Vec<String> {
  std::fs::read_dir(tmp)
    .map(|entries| {
      entries
        .filter_map(Result::ok)
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with("assay-batch-"))
        .collect()
    })
    .unwrap_or_default()
}

/// Waits on a real OS process, which no virtual clock can drive.
#[cfg_attr(dylint_lib = "id_effect_lints", allow(no_thread_sleep_in_domain))]
fn pause() {
  std::thread::sleep(Duration::from_millis(20));
}

#[test]
fn sigint_exits_130_and_removes_the_batch_temp_dir() {
  if !has_program("nix") || !has_program("kill") {
    eprintln!("skipping sigint test: nix or kill not on PATH");
    return;
  }
  let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
  let tmp = std::env::temp_dir().join(format!("assay-sigint-{}", std::process::id()));
  let _ = std::fs::remove_dir_all(&tmp);
  std::fs::create_dir_all(&tmp).expect("scratch TMPDIR");

  let mut child = Command::new(BIN)
    .args(["run", "tests/fixtures/slow.assay.json"])
    .current_dir(manifest)
    .env("TMPDIR", &tmp)
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .spawn()
    .expect("spawn assay");

  let deadline = Instant::now() + Duration::from_secs(30);
  while batch_dirs(&tmp).is_empty() {
    assert!(Instant::now() < deadline, "batch temp dir never appeared");
    assert!(
      child.try_wait().expect("poll").is_none(),
      "assay exited before the signal"
    );
    pause();
  }

  let killed = Command::new("kill")
    .args(["-INT", &child.id().to_string()])
    .status()
    .expect("run kill");
  assert!(killed.success(), "kill -INT failed");

  let deadline = Instant::now() + Duration::from_secs(30);
  let status = loop {
    if let Some(status) = child.try_wait().expect("poll") {
      break status;
    }
    if Instant::now() >= deadline {
      let _ = child.kill();
      panic!("assay did not exit after SIGINT");
    }
    pause();
  };

  assert_eq!(status.code(), Some(130), "Ctrl-C exit code");
  assert_eq!(
    batch_dirs(&tmp),
    Vec::<String>::new(),
    "batch temp dir left behind"
  );
  let _ = std::fs::remove_dir_all(&tmp);
}
