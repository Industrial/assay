# Changelog

All notable changes to this workspace (`nix-assay`, `nixq`, `nixdrv`, `nixstore`, `nixfetch`)
are documented here. The format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and the workspace uses [Semantic Versioning](https://semver.org/) with one shared version.

## [0.3.0] - Unreleased

Re-platform of every crate on [id_effect](https://github.com/Industrial/id_effect) 0.13.0 from
crates.io (was 0.4). Capabilities are traits returning `Effect`, commands return typed outputs
rendered at the edge, and every bin runs through `id_effect_cli::run_main`. The `0`/`1`/`2` exit
contract of each bin and its stdout are unchanged (pinned by `crates/*/tests/bin_contract.rs`);
smoke and dogfood suite verdicts are identical to 0.2.1.

### Added

- Exit code `130` on Ctrl-C in every bin: the program is interrupted and its finalizers run
  (`nix` / `git` children killed, `nixfetch` staging dirs removed).
- Exit code `101` for a defect (panic) in every bin; in `assay`, a case that dies also yields
  `101`.
- `assay`: a malformed suite reports every decode error with its path (e.g.
  `cases.badProp.seed: expected i64`) instead of the first one.
- `nixfetch fetch-url` / `fetch-git`: per-attempt 30 s timeout, up to 3 retries of transient
  failures (100 ms exponential backoff, jittered) and a 120 s overall timeout; hash mismatches,
  HTTP 4xx and parse errors are never retried.
- `nixstore`: `edge` module with the exit wrappers (`run_main_coded`, `run_main_coded_by`)
  shared by every bin.
- `test-support` cargo feature on `nix-assay`, `nixq`, `nixstore` and `nixfetch` exposing their
  in-memory fakes.
- Workspace tooling: `id_effect_lints` via Dylint (`moon run :lint-effect`, CI) and
  `id-effect doctor` in CI (one id_effect version in the build).
- Exit-code table per bin in [docs/cli.md](docs/cli.md#exit-codes).

### Changed

- MSRV is Rust 1.94 (`rust-version` in every crate).
- `assay run --case-timeout-ms`: the clock starts when the case starts running, not when it
  was queued, and the timeout also bounds the retries.
- `assay run --retry-flaky-eval`: retries (2 retries, 1 ms apart) only cases whose evaluation
  failed with an I/O error.
- `nixfetch fetch-url` / `fetch-git` write `--out` / the destination only after the hash
  verifies; a failed verify leaves nothing behind.
- `nixfetch fetch-git`: the default destination is content-addressed,
  `$TMPDIR/nixfetch-git-<nix32>`.
- stderr messages are rendered from each error's `Display` and now include the underlying
  cause; scripts should match exit codes, not stderr text.
- `assay`: `null` is accepted for `referrers`, `closureSize` and `closure_size` in `pathInfo`
  claims.
- `nixstore`: the SQLite connection is opened once per run (`layer(store_root)`) instead of per
  query; `assay` opens it only on the first `pathInfo` claim.

### Removed

- `nix-assay`: the `proptest` cargo feature (property tests are dev-only now) and the `optics`
  feature (`id_effect_optics` dependency dropped).
- `nixq`: the `optics` feature and `src/lib/optics.rs`.
- `anyhow` from library code.
- The `cli-exit` feature is kept but is now a no-op (`id_effect_cli` is always linked).

### BREAKING CHANGES (library API)

- `nixstore`: `PathInfoStoreKey`, `providers_for_store`, `live_providers`, `cmd_path_info` and
  `SqlitePathInfoStore::{new, from_default, store_root}` removed; use `PathInfoService`,
  `NixstoreEnv`, `layer(store_root)` and the typed `path_info()` / `PathInfoReport`.
- `nixq`: `JsonSourceKey`, `live_providers`, `mock_providers`, `cmd_*`,
  `object_keys_traversal` and `value_has_attrs_via_traversal` removed; use `JsonSourceService`,
  `NixqEnv` and the typed command outputs (`JsonReport`, `DiffReport`, `PredicateResult`).
- `nixdrv`: `DrvSourceKey`, `MockDrvSource`, `live_providers`, `cmd_*`, `derivation_to_json`
  and `InfraError::{Io, Json(String)}` removed; `DrvSource` / `DrvSourceService` /
  `FsDrvSource` re-export nixq's `JsonSource` types.
- `nixfetch`: `cmd_*`, `live_providers`, `mock_*`, `ClockKey`, `StdClock`,
  `hash_path_recursive`, `hash_flat_path`, `digest_report` and `verify_or_err` removed; I/O goes
  through the `Http`, `Git` and `PathIo` capabilities.
- `nix-assay`: claim interpretation and runs are `Effect`s over per-claim environments
  (`EvalEnv`, `SnapshotEnv`, `PathInfoEnv`, `FsEnv`, `SuiteEnv`, `AssayEnv`); `ClockKey`,
  `StdClock`, `NixWorkerPoolKey`, the `EvalBackend` alias, the `BATCH_NIX_FILE_EVAL` /
  `FORCE_BATCH_JSON_EVAL` hooks and the `pool` / `timeout` modules removed; suite decoding uses
  `#[derive(Schema)]` wire types; fakes are behind `cfg(test)` / `test-support`.
  `EvalResult::into_effect` is now `EvalResult::lift` and `verdict::outcome_to_effect` is
  `verdict::outcome_verdict` (id_effect_lints: no `_effect` suffix on effect builders).
- All crates: the id_effect 0.4 DI surface (`provide!`, `mock_capability!`, `Needs`, `run_with`,
  …) is gone from public signatures; dependents must move to id_effect 0.13.

### Known gaps

- `nixfetch`: `git-remote-http` helpers can outlive an attempt timeout when only the
  `nixfetch` process is signalled.
- `nixfetch fetch-git`: a shallow clone that succeeds cannot check out a non-HEAD `--rev`.
- `nixfetch`: fifos are read as regular files.
- `assay` (pre-existing, pinned as-is): `throws` does not enforce the expected message
  (`assay.throws "builtins.throw \"a\"" "b"` passes), and a parse error counts as a throw
  (`assay.throws "this is not nix" null` passes).
- Exit codes differ across bins for similar failures (`nixq path-info` unknown path is `2`,
  `nixstore` is `1`; `nixfetch` I/O / parse failures are `2`, other bins `1`); kept for
  compatibility.

## [0.2.1] - 2026-08-23

- `assay --version`; inter-crate dependency requirements and template pins aligned.

[0.3.0]: https://github.com/Industrial/assay/compare/v0.2.1...HEAD
[0.2.1]: https://github.com/Industrial/assay/releases/tag/v0.2.1
