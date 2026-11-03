---
slug: assay-id-effect-0-13
title: "Re-platform assay workspace on id_effect 0.13.0 (idiomatic, not compile-fix)"
acceptance_criteria:
  - 'Every id_effect* dependency in all 5 crates resolves to 0.13.0 from crates.io; Cargo.lock holds exactly one version of each id_effect* crate; no path/git id_effect deps.'
  - 'Zero uses of 0.4-only APIs: Env, Needs, Cap, CapList, Effect::new(|r|), run_blocking, run_fork, run_with, ThreadSleepRuntime, provide!, providers!, mock_capability!, join_binds, retry_with_clock.'
  - 'Every capability is a trait whose methods return Effect<A, E> (R = HNil) behind a newtype, with an *Env alias and a value-taking layer(); live impls in src, fakes behind cfg(test) or a test-support feature (no mocks in release builds).'
  - 'All nix / git / sqlite / fs / http I/O goes through a capability: no Command::new, std::fs, SystemTime::now or thread-local test hooks outside the live adapter files.'
  - 'assay run core uses for_each_par with Concurrency::Bounded(n), timeout_fail and a bounded Effect::retry(Schedule); no hand-rolled fibers, poll loops, Condvar semaphore, thread::sleep or nested run_blocking.'
  - 'Each bin runs through id_effect_cli::run_main (or a documented wrapper that keeps its contract): Ctrl-C interrupts, runs finalizers, exits 130; a defect exits 101.'
  - 'The exit-code contract of each bin is pinned by subprocess tests on the real binaries (CARGO_BIN_EXE_*), written BEFORE the port and green AFTER it.'
  - 'Commands return typed outputs (structs) rendered at the edge; no serde_json::Value command results and no "__text__" sentinel.'
  - 'Suite/claim decoding uses #[derive(Schema)] + decode_unknown_all -> ParseErrors (every error reported with its path); hand-written Schema::make codecs removed.'
  - 'One error enum per crate boundary with From widening; no anyhow in lib code; no unwrap/expect/println/unreachable! on effect paths.'
  - 'Tests use run_test + assert_exit_* / assert_golden_exit; timeout/retry tests use TestExecutor virtual time with exact-duration assertions; no wall-clock sleeps in tests; laws/claims property tests use testing::property (prop_check, from_schema).'
  - 'id_effect_lints wired via Dylint (moon :lint-effect on pre-push + CI job lint-effect) with zero warnings. Amended in REVIEW: kept out of :lint / pre-commit because Dylint builds on a nightly toolchain.'
  - 'moon :check :lint :test :assay green; nix/assay smoke + dogfood suites produce verdicts identical to the W0 baseline.'
  - 'docs/cli.md documents the exit-code table per bin; workspace version bumped (D5); CHANGELOG entry.'
non_goals:
  - id_effect_platform adoption (CommandSpec cannot capture stdout or set env; the crate pulls reqwest+TLS unconditionally).
  - id_effect_process / process-pool, id_effect_config, id_effect_ai, axum/rpc/sql batteries.
  - Changing claim algebra or DSL semantics.
  - crates.io publication (tracked by assay-public-release leaf-crates-io).
risk_class: high
mode: heavy
work_type: initiative
dependencies: []
---

# Re-platform assay on id_effect 0.13.0

## Context

The workspace is on id_effect 0.4.0 (crates.io). Going 0.4 -> 0.13 rewrites the
runtime, DI and macro surface (0.6 + 0.7 guides). The later deltas are small: 0.8
changes run_main config and adds Ctrl-C handling; 0.9–0.13 are mostly web/sql/mesh.
The guides chain: `id_effect/docs/MIGRATION-0.{6,7,8,...,13}.md` (read at tag v0.13.0).
MSRV is 1.94; local rustc is 1.93.

Crate dependency chain (forces wave order): nixstore <- nixq <- nixdrv <- nixfetch;
assay <- {nixq, nixdrv, nixstore}.

Supersedes Maestro task `leaf-deps-id-effect` (tsk-mslqjpv5-q4t3ly, mission
pln-mslqjm68-f5rkwl): W1 meets its AC (no absolute id_effect paths).

## Decisions (locked 2026-11-02, user approved all recommendations)

| ID | Decision | Proposal | Delta if wrong |
|----|----------|----------|----------------|
| D1 | Exit-code contract | Keep each bin's current contract (characterized in W0) and add 101 (defect) / 130 (interrupt). Use run_main as-is where its 0/1/101/130 map fits; elsewhere a thin wrapper that keeps run_main's Ctrl-C + mapping and adds the bin's codes (assay worst-case fold, nixq predicate 0/1, nixfetch HashMismatch) | Raw run_main changes exit codes that consumers' CI can see |
| D2 | Subprocess/fs ports | Our own capability traits (NixEval, Git, PathInfoStore, Fs) returning Effect; live impls use tokio::process / std inside from_async / sync | platform needs an upstream CommandSpec stdout patch + reqwest in every bin |
| D3 | Shared edge/fs code | No new crate; put shared helpers (exit wrapper, Fs source) in the lowest crate that needs them (nixstore) and re-export | New crate = an extra crates.io publish and version churn |
| D4 | Optics | Drop id_effect_optics (only used in tests / off the production path) | Keep the feature and port the Traversal |
| D5 | Versioning | Workspace 0.3.0 (Ctrl-C/130, MSRV, typed output) | 0.2.2 if outputs stay byte-identical |
| D6 | Intermediate state | Feature branch `feat/id-effect-0.13`; per-crate gates `cargo test -p <crate>` until W3; workspace gate required from W3 | One big-bang commit |

## Waves

### W0 Safety net (still on 0.4, no behavior change)
- w0-toolchain: devenv Rust >= 1.94; `cargo check --workspace` green.
- w0-golden-bins: subprocess tests per bin (CARGO_BIN_EXE_*) sweeping exit codes + golden stdout for representative inputs; capture baseline verdicts for the nix/assay smoke + dogfood suites.

### W1 Leaf crates (bottom-up, serial: nixstore -> nixq -> nixdrv)
Per crate:
- Bump deps to 0.13.0; drop unused direct id_effect_macro/_proc_macro deps and schema-serde where unused; move rstest to dev-deps.
- Capability = trait returning Effect + newtype + Env alias + layer().
- Each command is one `effect!` fn with an exact R; typed outputs.
- One error enum + From impls; run_main edge (D1).
- Fakes behind cfg(test); tests on run_test / assert_exit_*.

Crate-specific:
- nixstore: open the sqlite connection once via Layer::acquire_release (today it opens per query); typed PathInfo; StorePath/Hash brands via Schema refine.
- nixq: JsonSource on Schema/Unknown; predicate exit 0/1 via the D1 wrapper; drop optics (D4).
- nixdrv: DrvSource fs capability; ATerm parsing stays pure plain Rust.

### W2 nixfetch (runs in parallel with W3a)
- HttpFetch / GitFetch / PathIo capabilities; hashing + NAR go through PathIo.
- Temp dirs via Effect::scoped + finalizer.
- Per attempt timeout_fail + Effect::retry(exponential.intersect(recurs(n)).jittered()) + an overall timeout.
- Remove the unused ClockKey; HashMismatch exit code per D1.

### W3 assay (serial sub-leaves)
- w3a caps:
  - Split the god env into per-claim-kind R.
  - All nix eval (batch, suite load, compat) goes through the NixEval capability; remove the BATCH_NIX_FILE_EVAL thread-local.
  - PathInfoStore comes from the nixstore layer; no deep provision or env mutation.
  - Drop the EvalBackend alias and ClockKey (time belongs to the executor).
- w3b run core: for_each_par(cases, Concurrency::Bounded(jobs)); per-case timeout_fail; retry via Schedule. Delete the pool.rs Condvar semaphore, the run_fork/poll/sleep code and the timeout.rs loop; one Cause -> CaseVerdict fold.
- w3c schema: #[derive(Schema)] for Claim/Suite; decode_unknown_all -> ParseErrors reported in suite-load errors.
- w3d report: typed SuiteReport rendered at the edge (text/json). Use Stream only where output is genuinely incremental (map_effect_par keeps order). No println in effects; remove anyhow from lib.
- w3e tests: TestExecutor virtual-time tests for timeout/retry (exact durations); prop_check + from_schema for laws; assert_golden_exit for report snapshots; delete wall-clock sleeps.

### W4 Tooling + release
- Dylint id_effect_lints (tag v0.13.0) in moon :lint + CI with zero warnings; `id-effect doctor` in CI (one id_effect version).
- docs/cli.md exit table, fix the docs/comparison.md path-dep note, CHANGELOG, version per D5; supersede tsk-mslqjpv5-q4t3ly.

## Verification

Per leaf: `devenv shell -- cargo test -p <crate>` + clippy -D warnings + that bin's W0 golden-bin tests.
Wave exit: `moon :check :lint :test :assay`; smoke/dogfood verdicts diff-equal to the W0 baseline.

## Risks

- assay run-core semantics (timeouts/parallelism): mitigated by the W0 bin tests and virtual-time tests.
- Types move in lock-step across crates: when a crate's public capability API is used by assay, assay's matching patch lands in the same leaf.
- Tokio is now mandatory in core: blocking subprocess calls must not starve the executor (from_async + tokio::process).
