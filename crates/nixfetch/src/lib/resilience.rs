//! Bounded retries and timeouts for the network fetches (`fetch-url`, `fetch-git`).
//!
//! [`with_retry`] runs one attempt at a time:
//!
//! 1. each attempt is bounded by `attempt_timeout` (`timeout_fail` -> a transient
//!    [`InfraError::Timeout`]; the timed-out attempt is interrupted, which drops its connection or
//!    kills its `git`);
//! 2. a [transient](InfraError::is_transient) failure is retried after `base_delay * 2^k`
//!    (`k = 0, 1, ..`, jittered by `[0.8, 1.2]`), at most `retries` times; a permanent failure
//!    (4xx, parse errors, a hash mismatch, ...) fails at once;
//! 3. all attempts together are bounded by `overall_timeout` ([`InfraError::Deadline`], never
//!    retried).
//!
//! Delays and deadlines are read from the fiber's executor clock: real time under `run_main`,
//! virtual time on a `TestExecutor`. Nothing here reads the wall clock.

use std::time::Duration;

use id_effect::{Effect, Schedule};

use crate::error::InfraError;

/// Default per-attempt timeout.
pub const ATTEMPT_TIMEOUT: Duration = Duration::from_secs(30);
/// Default number of retries after the first attempt (so 4 attempts at most).
pub const RETRIES: u64 = 3;
/// Default delay before the first retry (doubled for each further one: 100, 200, 400 ms).
pub const BASE_DELAY: Duration = Duration::from_millis(100);
/// Default bound on all attempts together.
pub const OVERALL_TIMEOUT: Duration = Duration::from_secs(120);

/// Retry and timeout settings of one fetch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
  pub attempt_timeout: Duration,
  pub retries: u64,
  pub base_delay: Duration,
  pub overall_timeout: Duration,
  /// Scale every delay by a random factor in `[0.8, 1.2]` (off for exact-timing tests).
  pub jitter: bool,
}

impl RetryPolicy {
  /// The defaults: 30 s per attempt, 3 retries from 100 ms, 120 s overall, jittered.
  pub const DEFAULT: Self = Self {
    attempt_timeout: ATTEMPT_TIMEOUT,
    retries: RETRIES,
    base_delay: BASE_DELAY,
    overall_timeout: OVERALL_TIMEOUT,
    jitter: true,
  };

  /// `exponential(base_delay) ∩ recurs(retries)`, jittered, continuing only on transient
  /// errors.
  pub fn schedule(&self) -> Schedule<(Duration, u64), InfraError> {
    let backoff = Schedule::exponential(self.base_delay).intersect(Schedule::recurs(self.retries));
    let backoff = if self.jitter {
      backoff.jittered()
    } else {
      backoff
    };
    backoff.while_input(InfraError::is_transient)
  }
}

impl Default for RetryPolicy {
  fn default() -> Self {
    Self::DEFAULT
  }
}

/// Runs `attempt()` under `policy` (see the [module docs](self)); `what` names the operation in
/// timeout errors (e.g. `GET https://…`). `attempt` builds a fresh effect per attempt.
pub fn with_retry<A>(
  what: impl Into<String>,
  policy: RetryPolicy,
  attempt: impl Fn() -> Effect<A, InfraError> + Send + Sync + 'static,
) -> Effect<A, InfraError>
where
  A: Send + 'static,
{
  let what = what.into();
  let per_attempt = what.clone();
  let RetryPolicy {
    attempt_timeout,
    overall_timeout,
    ..
  } = policy;
  Effect::retry(
    move || {
      let what = per_attempt.clone();
      attempt().timeout_fail(attempt_timeout, move || InfraError::Timeout {
        what,
        after: attempt_timeout,
      })
    },
    policy.schedule(),
  )
  .timeout_fail(overall_timeout, move || InfraError::Deadline {
    what,
    after: overall_timeout,
  })
}

#[cfg(test)]
mod tests {
  use std::sync::Arc;
  use std::sync::atomic::{AtomicU32, Ordering};

  use id_effect::{
    Executor, Exit, TestExecutor, assert_exit_fail, expect_exit_success, fail, run_test_on, sleep,
    succeed, suspend,
  };

  use super::*;

  const MS: Duration = Duration::from_millis(1);

  fn policy(retries: u64) -> RetryPolicy {
    RetryPolicy {
      attempt_timeout: Duration::from_secs(30),
      retries,
      base_delay: 100 * MS,
      overall_timeout: Duration::from_secs(600),
      jitter: false,
    }
  }

  fn status(status: u16) -> InfraError {
    InfraError::HttpStatus {
      url: "u".into(),
      status,
    }
  }

  /// Fails with `errors[n]` on attempt `n`, then succeeds with the attempt count.
  fn scripted(
    calls: &Arc<AtomicU32>,
    errors: Vec<InfraError>,
  ) -> impl Fn() -> Effect<u32, InfraError> + Send + Sync + 'static {
    let calls = Arc::clone(calls);
    move || {
      let (calls, errors) = (Arc::clone(&calls), errors.clone());
      suspend(move || {
        let n = calls.fetch_add(1, Ordering::SeqCst);
        match errors.get(n as usize) {
          Some(e) => fail(e.clone()),
          None => succeed(n + 1),
        }
      })
    }
  }

  /// The exit and the virtual time it took.
  fn timed<A: Send + 'static>(eff: Effect<A, InfraError>) -> (Exit<A, InfraError>, Duration) {
    let exec = TestExecutor::new();
    let start = exec.now();
    let exit = run_test_on(&exec, eff);
    (exit, exec.now() - start)
  }

  #[test]
  fn transient_failures_back_off_exponentially_then_succeed() {
    let calls = Arc::new(AtomicU32::new(0));
    let attempt = scripted(&calls, vec![status(503), status(502)]);
    let (exit, took) = timed(with_retry("op", policy(3), attempt));
    assert_eq!(expect_exit_success(exit), 3);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(took, 100 * MS + 200 * MS);
  }

  #[test]
  fn retries_are_bounded_by_recurs() {
    // exponential(100 ms) ∩ recurs(2): 3 attempts, 100 + 200 ms of backoff, then the last error.
    let calls = Arc::new(AtomicU32::new(0));
    let attempt = scripted(&calls, vec![status(503); 10]);
    let (exit, took) = timed(with_retry("op", policy(2), attempt));
    assert_exit_fail(&exit, &status(503));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(took, 300 * MS);
  }

  #[test]
  fn permanent_failures_are_not_retried() {
    for permanent in [
      InfraError::HashMismatch {
        expected: "a".into(),
        actual: "b".into(),
      },
      status(404),
      InfraError::Parse("p".into()),
    ] {
      let calls = Arc::new(AtomicU32::new(0));
      let attempt = scripted(&calls, vec![permanent.clone(); 10]);
      let (exit, took) = timed(with_retry("op", policy(3), attempt));
      assert_exit_fail(&exit, &permanent);
      assert_eq!(calls.load(Ordering::SeqCst), 1, "{permanent:?}");
      assert_eq!(took, Duration::ZERO);
    }
  }

  #[test]
  fn a_hanging_attempt_times_out_and_is_retried() {
    // Attempts 1 and 2 hang (30 s timeout each), attempt 3 answers.
    let calls = Arc::new(AtomicU32::new(0));
    let seen = Arc::clone(&calls);
    let attempt = move || {
      let seen = Arc::clone(&seen);
      suspend(move || {
        if seen.fetch_add(1, Ordering::SeqCst) < 2 {
          sleep(Duration::from_secs(3600)).map(|()| 0)
        } else {
          succeed(7)
        }
      })
    };
    let (exit, took) = timed(with_retry("op", policy(3), attempt));
    assert_eq!(expect_exit_success(exit), 7);
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert_eq!(took, Duration::from_secs(60) + 300 * MS);
  }

  #[test]
  fn every_attempt_timing_out_fails_with_timeout() {
    let attempt = || sleep::<InfraError, _>(Duration::from_secs(3600)).map(|()| 0u8);
    let (exit, took) = timed(with_retry("GET u", policy(2), attempt));
    assert_exit_fail(
      &exit,
      &InfraError::Timeout {
        what: "GET u".into(),
        after: Duration::from_secs(30),
      },
    );
    assert_eq!(took, Duration::from_secs(90) + 300 * MS);
  }

  #[test]
  fn the_overall_timeout_cuts_the_retries_short() {
    let attempt = || sleep::<InfraError, _>(Duration::from_secs(3600)).map(|()| 0u8);
    let p = RetryPolicy {
      overall_timeout: Duration::from_secs(45),
      ..policy(3)
    };
    let (exit, took) = timed(with_retry("GET u", p, attempt));
    assert_exit_fail(
      &exit,
      &InfraError::Deadline {
        what: "GET u".into(),
        after: Duration::from_secs(45),
      },
    );
    assert_eq!(took, Duration::from_secs(45));
  }

  #[test]
  fn jitter_stays_within_bounds() {
    let calls = Arc::new(AtomicU32::new(0));
    let attempt = scripted(&calls, vec![status(503); 10]);
    let p = RetryPolicy {
      jitter: true,
      ..policy(2)
    };
    let (exit, took) = timed(with_retry("op", p, attempt));
    assert_exit_fail(&exit, &status(503));
    assert_eq!(calls.load(Ordering::SeqCst), 3);
    assert!(
      (240 * MS..=360 * MS).contains(&took),
      "300 ms of backoff jittered by [0.8, 1.2]: {took:?}"
    );
  }

  #[test]
  fn defaults() {
    assert_eq!(RetryPolicy::default(), RetryPolicy::DEFAULT);
    let RetryPolicy {
      attempt_timeout,
      retries,
      base_delay,
      overall_timeout,
      jitter,
    } = RetryPolicy::default();
    assert_eq!(
      (
        attempt_timeout,
        retries,
        base_delay,
        overall_timeout,
        jitter
      ),
      (
        Duration::from_secs(30),
        3,
        100 * MS,
        Duration::from_secs(120),
        true
      )
    );
  }
}
