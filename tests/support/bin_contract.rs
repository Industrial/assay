//! Shared harness for each crate's `tests/bin_contract.rs`.
//!
//! A case runs the real binary from the crate's manifest dir, asserts its exit
//! code and, when `golden` is set, compares stdout with
//! `tests/golden/<case>.stdout`. The workspace path is normalized to
//! `<WORKSPACE>`. Re-bless with `BLESS=1 cargo test -p <crate> --test bin_contract`.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

pub struct Case {
  pub name: &'static str,
  pub args: &'static [&'static str],
  pub stdin: Option<&'static str>,
  pub exit: i32,
  pub golden: bool,
}

/// One `#[test]` per case: `bin_case!(name, ["arg", ..], stdin: None, exit: 0, golden: true);`
macro_rules! bin_case {
  ($name:ident, [$($arg:expr),* $(,)?], stdin: $stdin:expr, exit: $exit:expr, golden: $golden:expr) => {
    #[test]
    fn $name() {
      support::check(
        BIN,
        env!("CARGO_MANIFEST_DIR"),
        &support::Case {
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
#[allow(dead_code)]
pub fn has_program(program: &str) -> bool {
  Command::new(program)
    .arg("--version")
    .stdout(Stdio::null())
    .stderr(Stdio::null())
    .status()
    .is_ok_and(|s| s.success())
}
