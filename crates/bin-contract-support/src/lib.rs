//! Shared harness for each crate's `tests/bin_contract.rs`.
//!
//! A case runs the real binary from the crate's manifest dir, asserts its exit
//! code and, when `golden` is set, compares stdout with
//! `tests/golden/<case>.stdout`. The workspace path is normalized to
//! `<WORKSPACE>`. Re-bless with `BLESS=1 cargo test -p <crate> --test bin_contract`.
//!
//! The suites read workspace files (sibling fixtures, `nix/assay/tests`), so they are
//! excluded from the packaged crates; this crate is never published.

// A test harness: a failed `expect` is the test failure, with the message as its report.
#![allow(clippy::expect_used)]

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

/// One pinned invocation of a binary.
#[derive(Debug, Clone, Copy)]
pub struct Case {
  /// Test name; also the golden file stem (`tests/golden/<name>.stdout`).
  pub name: &'static str,
  /// Command-line arguments.
  pub args: &'static [&'static str],
  /// Bytes written to stdin, or `None` for a null stdin.
  pub stdin: Option<&'static str>,
  /// Expected exit code.
  pub exit: i32,
  /// Whether stdout is compared with the golden file.
  pub golden: bool,
}

/// One `#[test]` per case: `bin_case!(name, ["arg", ..], stdin: None, exit: 0, golden: true);`
///
/// Expects a `const BIN: &str` (the binary path, `env!("CARGO_BIN_EXE_<name>")`) in scope.
#[macro_export]
macro_rules! bin_case {
  ($name:ident, [$($arg:expr),* $(,)?], stdin: $stdin:expr, exit: $exit:expr, golden: $golden:expr) => {
    #[test]
    fn $name() {
      $crate::check(
        BIN,
        env!("CARGO_MANIFEST_DIR"),
        &$crate::Case {
          name: stringify!($name),
          args: &[$($arg),*],
          stdin: $stdin,
          exit: $exit,
          golden: $golden,
        },
      );
    }
  };
}

/// Runs `bin` for `case` from `manifest_dir` and asserts its exit code and (golden) stdout.
///
/// # Panics
///
/// When the binary cannot be run, the exit code differs, or stdout differs from the golden
/// file (or the golden file is missing). With `BLESS` set, writes the golden file instead.
pub fn check(bin: &str, manifest_dir: &str, case: &Case) {
  let root = Path::new(manifest_dir);
  let mut child = Command::new(bin)
    .args(case.args)
    .current_dir(root)
    .stdin(if case.stdin.is_some() {
      Stdio::piped()
    } else {
      Stdio::null()
    })
    .stdout(Stdio::piped())
    .stderr(Stdio::piped())
    .spawn()
    .unwrap_or_else(|e| panic!("{}: spawn {bin}: {e}", case.name));
  if let Some(input) = case.stdin {
    child
      .stdin
      .take()
      .expect("piped stdin")
      .write_all(input.as_bytes())
      .expect("write stdin");
  }
  let out = child.wait_with_output().expect("wait for binary");
  let stdout = String::from_utf8_lossy(&out.stdout);
  assert_eq!(
    out.status.code(),
    Some(case.exit),
    "{}: exit code\n--- stdout\n{stdout}\n--- stderr\n{}",
    case.name,
    String::from_utf8_lossy(&out.stderr)
  );
  if !case.golden {
    return;
  }
  let workspace = root
    .join("../..")
    .canonicalize()
    .expect("canonical workspace root");
  let actual = stdout.replace(&*workspace.to_string_lossy(), "<WORKSPACE>");
  let golden = root
    .join("tests/golden")
    .join(format!("{}.stdout", case.name));
  if std::env::var_os("BLESS").is_some() {
    std::fs::create_dir_all(golden.parent().expect("golden dir")).expect("create golden dir");
    std::fs::write(&golden, &actual).expect("write golden");
    return;
  }
  let expected = std::fs::read_to_string(&golden).unwrap_or_else(|e| {
    panic!(
      "{}: read {} ({e}); bless with BLESS=1",
      case.name,
      golden.display()
    )
  });
  assert_eq!(
    actual,
    expected,
    "{}: stdout differs from {}",
    case.name,
    golden.display()
  );
}

/// True when `program` runs (e.g. `nix` for suites that evaluate Nix).
#[must_use]
pub fn has_program(program: &str) -> bool {
  Command::new(program)
    .arg("--version")
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .is_ok_and(|s| s.success())
}
