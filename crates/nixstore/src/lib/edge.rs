//! Small program-edge helpers shared by the workspace's bins (nixstore, nixq, nixdrv, ...).

use std::fmt;
use std::io::Write;
use std::process::ExitCode;
use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use id_effect::{Effect, HNil, fail, succeed, suspend};
use id_effect_cli::{RunMainConfig, run_main};

use crate::error::InfraError;

/// Lifts a finished `Result` into an effect that needs nothing (so `~from_result(..)` binds
/// inside any `effect!` without annotations).
pub fn from_result<A, E>(result: Result<A, E>) -> Effect<A, E> {
  match result {
    Ok(a) => succeed(a),
    Err(e) => fail(e),
  }
}

/// Writes `text` plus a newline to stdout; a write error fails with `InfraError::Io`.
pub fn print_line<E>(text: impl Into<String>) -> Effect<(), E>
where
  E: From<InfraError> + Send + 'static,
{
  let text = text.into();
  suspend(move || {
    let mut out = std::io::stdout().lock();
    match writeln!(out, "{text}") {
      Ok(()) => succeed(()),
      Err(e) => fail(E::from(InfraError::Io {
        path: "<stdout>".into(),
        message: e.to_string(),
      })),
    }
  })
}

/// `run_main` prints typed failures with `Debug`; this wrapper shows their `Display` instead.
pub struct CliError<E>(pub E);

impl<E: fmt::Display> fmt::Debug for CliError<E> {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    fmt::Display::fmt(&self.0, f)
  }
}

/// [`run_main`] for bins whose exit codes go beyond its 0/1 split.
///
/// | outcome | stderr | exit code |
/// |---|---|---|
/// | success with `code` | – | `code` (e.g. 1 for a false predicate) |
/// | typed failure `Fail(e)` | `error: {e:?}` | `fail_code` |
/// | defect | `error: defect: …` | `101` |
/// | interrupt (Ctrl-C, finalizers run) | `error: interrupted (…)` | `130` |
pub fn run_main_coded<E>(program: Effect<u8, E>, fail_code: u8) -> ExitCode
where
  E: fmt::Debug + Send + 'static,
{
  run_main_coded_by(program, move |_| fail_code)
}

/// [`run_main_coded`] whose typed-failure exit code depends on the error: `Fail(e)` exits
/// `fail_code(&e)` (e.g. 1 for a hash mismatch, 2 for everything else). Defect 101 and
/// interrupt 130 are unchanged.
pub fn run_main_coded_by<E>(
  program: Effect<u8, E>,
  fail_code: impl FnOnce(&E) -> u8 + Send + 'static,
) -> ExitCode
where
  E: fmt::Debug + Send + 'static,
{
  let code = Arc::new(AtomicU8::new(0));
  let (on_success, on_failure) = (Arc::clone(&code), Arc::clone(&code));
  let program = program
    .map(move |c| on_success.store(c, Ordering::SeqCst))
    .map_err(move |e| {
      on_failure.store(fail_code(&e), Ordering::SeqCst);
      e
    });
  let exit = run_main(program, HNil, RunMainConfig::minimal());
  // `run_main` maps every typed failure (and only those) to 1; `code` then holds its code.
  if exit == ExitCode::SUCCESS || exit == ExitCode::from(1) {
    ExitCode::from(code.load(Ordering::SeqCst))
  } else {
    exit
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use id_effect::{assert_exit_fail, die, expect_exit_success, run_test};

  #[test]
  fn from_result_lifts_ok_and_err() {
    assert_eq!(
      expect_exit_success(run_test(from_result::<_, ()>(Ok(3)))),
      3
    );
    assert_exit_fail(&run_test(from_result::<(), _>(Err("e"))), &"e");
  }

  #[test]
  fn cli_error_debug_is_display() {
    let e = CliError(InfraError::UnknownPath("/nix/store/x".into()));
    assert_eq!(format!("{e:?}"), e.0.to_string());
  }

  #[test]
  fn run_main_coded_maps_success_failure_and_defect() {
    assert_eq!(
      run_main_coded(succeed::<u8, String, HNil>(0), 2),
      ExitCode::SUCCESS
    );
    assert_eq!(
      run_main_coded(succeed::<u8, String, HNil>(1), 2),
      ExitCode::from(1)
    );
    assert_eq!(
      run_main_coded(fail::<u8, String, HNil>("x".into()), 2),
      ExitCode::from(2)
    );
    assert_eq!(
      run_main_coded(fail::<u8, String, HNil>("x".into()), 1),
      ExitCode::from(1)
    );
    assert_eq!(
      run_main_coded(die::<u8, String, HNil>("bug"), 2),
      ExitCode::from(101)
    );
  }

  #[test]
  fn run_main_coded_by_picks_the_code_from_the_error() {
    let code = |e: &String| if e == "mismatch" { 1 } else { 2 };
    assert_eq!(
      run_main_coded_by(fail::<u8, String, HNil>("mismatch".into()), code),
      ExitCode::from(1)
    );
    assert_eq!(
      run_main_coded_by(fail::<u8, String, HNil>("io".into()), code),
      ExitCode::from(2)
    );
    assert_eq!(
      run_main_coded_by(succeed::<u8, String, HNil>(0), code),
      ExitCode::SUCCESS
    );
    assert_eq!(
      run_main_coded_by(die::<u8, String, HNil>("bug"), code),
      ExitCode::from(101)
    );
  }
}
