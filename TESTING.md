# Testing

Assay's tests come in layers, from fast in-process checks to the real binaries running real
`nix`. Pick the lowest layer that can show the behaviour; add a higher one only when the
behaviour lives there (an exit code, a signal, a Nix evaluation).

```bash
moon run :test     # nextest (profile default) + doctests — layers 1–5
moon run :assay    # layer 6: the Nix suites
moon run :coverage # llvm-cov over nextest, HTML report
```

nextest profiles live in `.config/nextest.toml`: `default` (local), `ci` (fewer retries, fail
fast), `coverage`. Run one with `cargo nextest run --profile ci`. Doctests are not run by
nextest; `:test` runs them separately with `cargo test --doc`.

Tests that need `nix` (or `kill`) on `PATH` print `skipping …` and pass when it is missing, so
run them inside `devenv shell` to actually exercise them.

## 1. Unit tests (fakes, `run_test`)

`#[cfg(test)] mod tests` next to the code, plus cross-module files such as
`crates/*/src/lib/coverage_tests.rs`. Effects run with `id_effect::run_test` and are checked
with `expect_exit_success` / `expect_exit_failure` / `assert_exit_fail`.

Capabilities are swapped for in-memory fakes (`MockNixEval`, `MockJsonSource`, `FakeStore`, …)
provided through a layer, so domain logic is tested without touching the filesystem or
spawning `nix`. Fakes are compiled under `#[cfg(any(test, feature = "test-support"))]`; a
crate testing against a sibling's fakes enables that sibling's `test-support` feature in
`[dev-dependencies]`. The live adapters (`live.rs`) get their own small tests against temp
dirs and local processes.

**Add one** for any new logic, error path or capability method.

## 2. Virtual time (`TestExecutor`)

Timeouts, retries and backoff are tested on `id_effect::TestExecutor`, whose clock only moves
when the program sleeps — no real waiting, deterministic results. See `timed` in
`crates/nixfetch/src/lib/coverage_tests.rs` (fetch timeouts and retries) and the case-timeout
tests in `crates/assay/src/lib/run.rs`.

**Add one** whenever behaviour depends on time. Never `std::thread::sleep` in domain code or its
tests (the `id_effect_lints` Dylint lints flag it).

## 3. Property tests (proptest)

`proptest!` blocks using generators from `id_effect::testing::property` (`arb_unknown`,
`from_schema`, `prop_check`): the claim laws (`crates/assay/src/lib/laws.rs`), claim codec
round trips (`claims.rs`) and the shrinker (`prop.rs`). Enabled by the `proptest` feature of
`id_effect` in `nix-assay`'s `[dev-dependencies]`.

**Add one** for algebraic laws, encode/decode round trips and parsers.

## 4. Binary contract (subprocess goldens)

`crates/<crate>/tests/bin_contract.rs` runs the real binary and pins its exit code and stdout.
Each case is one line, using the shared harness in the unpublished `bin-contract-support` crate
(`crates/bin-contract-support`):

```rust
bin_case!(get_missing, ["get", "a.zz"], stdin: Some(r#"{"a":{"b":1}}"#), exit: 1, golden: true);
```

With `golden: true`, stdout must equal `tests/golden/<case>.stdout` byte for byte (the
workspace path is normalized to `<WORKSPACE>`). `assay`'s file also has `nix_case!`, which
skips when `nix` is missing; output whose text comes from Nix itself pins only the exit code
(`golden: false`).

These files are the exit-code contract documented in [docs/cli.md](docs/cli.md). **Add a case**
for every new subcommand, flag or exit path.

### Re-blessing goldens

Never edit or reformat golden files by hand — regenerate them and review the diff:

```bash
BLESS=1 cargo test -p <crate> --test bin_contract   # tests/golden/*.stdout
ID_EFFECT_BLESS=1 cargo test -p nix-assay           # testdata/exits/* (id_effect assert_golden)
```

A changed golden is a user-visible change: mention it in the commit (`feat!:` / `fix:`) and
update `docs/cli.md` if an exit code moved.

## 5. Interrupt (SIGINT)

`crates/assay/tests/interrupt.rs` starts `assay run` on a slow suite, sends SIGINT, and checks
the process exits `130`, the `nix` child is gone and the `assay-batch-*` temp dir was removed.
It polls a real process with bounded waits, which is why it is the one place that sleeps.
Needs `nix` and `kill`.

**Add to it** when you add a resource that must be released on Ctrl-C (a child process, a temp
dir).

## 6. Nix suites (Assay testing itself)

`nix/assay/tests/smoke.assay.nix` exercises the DSL's claim kinds; `dogfood.assay.nix` tests
`nix/assert.nix` with Assay. `moon run :assay` builds the binary and runs both; CI runs the
smoke suite as well. Run one directly:

```bash
cargo run -p nix-assay -- run nix/assay/tests/smoke.assay.nix
```

**Add a case** when you add or change a DSL claim in `nix/assay/default.nix`. Suite verdicts
are also pinned as goldens (`run_smoke_*`, `run_dogfood_json`) in `nix-assay`'s bin contract,
so re-bless those too.

## Benchmarks

`nix-assay` depends on `criterion` for benchmarks; `moon run :bench` runs them when a
`benches/` directory exists.
