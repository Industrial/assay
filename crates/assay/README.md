# Assay

Hybrid Nix unit testing: **claims authored as Nix values**, **outcomes produced by a Rust + `id_effect` runner** with isolated evaluation, normalization, and structured reporting.

## Why hybrid?

Pure Nix runners (`lib.runTests`, nix-unit, namaka, nixt) cannot reliably:

- catch per-case throws without polluting sibling cases,
- compare derivations safely (deep force → stack overflow),
- classify eval failures, enforce force-set claims, or run properties with shrink.

Assay keeps **authoring in Nix** and moves **evaluation, isolation, and diffing** to a capability-injected runner.

Public docs: [../../docs/authoring.md](../../docs/authoring.md).



Nix DSL reference: [../../nix/assay/README.md](../../nix/assay/README.md).

Competitive comparison (measured): [COMPARISON.md](COMPARISON.md).

## Architecture

```text
Nix suites (nix/assay)  →  assay run  →  Rust runner (id_effect)
                                    ↓
                          Pass | Fail | EvalError | SnapshotMismatch | …
```

| Layer | Location | Role |
|-------|----------|------|
| DSL | `nix/assay/default.nix` | `suite`, `eq`, `throws`, `subset`, … |
| Runner | `crates/assay/` | Discover suites, eval in isolation, normalize, report |
| Goldens | `testdata/goldens/` | Snapshot storage |
| Compat | `fixtures/compat/` | runTests / nix-unit shaped fixtures |

## id_effect runner

**Doctrine:** library code returns `Effect` (id_effect 0.13); capabilities are the only I/O; soft assertion results are `CaseVerdict` in `Exit::Success`; infra is `Fail(InfraError)`; panics are `Die`. The CLI is a thin `id_effect_cli::run_main` edge (via `nixstore::edge::run_main_coded_by`).

### Core signatures

```rust
use id_effect::Effect;

// Claim interpreter — reaches nix / snapshots / the store only through AssayEnv.
pub fn interpret_claim(claim: Claim) -> Effect<CaseVerdict, InfraError, AssayEnv>;

// Suite orchestrator — load a suite, run its cases, build the typed report.
pub fn run_suite(path: &Path, opts: &RunOptions) -> Effect<SuiteReport, InfraError, AssayEnv>;
```

`run_path` dispatches a file or a directory; `render_report` turns a `SuiteReport` into
human / JSON / TAP text at the edge. `normalize` / `diff` stay pure value morphisms.

Cases run with `for_each_par(Concurrency::Bounded(n))` (`n` = available parallelism, fallback 4);
`--case-timeout-ms` is `timeout_fail` from the moment the case starts; `--retry-flaky-eval` is a
bounded `Effect::retry` (1 ms spacing, 2 retries) for I/O failures only.

### Capabilities

Each capability is a trait whose methods return `Effect`, behind a service newtype in an
`*Env = caps!(…)` environment — never globals. Live adapters live in `src/lib/live.rs`.

| Capability | Service (`caps.rs`) | Role | Live adapter |
|------------|---------------------|------|--------------|
| Nix evaluator | `NixEvalService` (`NixEval`) | suite / compat load, batch and per-claim eval | `LiveNixEval` (`nix eval` via `tokio::process`, killed on interrupt — see [SPIKE.md](SPIKE.md)) |
| Snapshot store | `SnapshotService` | golden read/write under `testdata/goldens/` | `SnapshotStore` over `Fs` |
| Store path-info | `nixstore::PathInfoService` | `pathInfo` claims | `LazyPathInfoStore` (opens `db.sqlite` on the first query) |
| Filesystem | `FsService` (`Fs`) | suite reads, discovery, snapshots | `LiveFs` (`std::fs` on the executor's blocking pool) |

Per-claim environments: `EvalEnv`, `SnapshotEnv`, `PathInfoEnv`, `FsEnv`, `SuiteEnv`; a run
needs `AssayEnv`. Time comes from the id_effect executor, so timeout and retry tests run on
`TestExecutor` virtual time.

In-memory fakes (`MockNixEval`, `SyncEval`, `MemFs`, `FakeStore`) are compiled only under
`cfg(test)` or the `test-support` feature; tests use `run_test` / `assert_exit_*` and
`testing::property` for the law / claim properties.

## CLI

```bash
devenv shell -- assay run <path>          # file or directory of suites
devenv shell -- assay run nix/assay/tests
```

Options (see `assay run --help`):

- `--format human|json|tap` — report format (default: human)
- `--json` — alias for `--format json` (backward compatible)
- `--update-snapshots` — refresh golden files
- `--case-timeout-ms <MS>` — per-case timeout, measured from the case's start
- `--retry-flaky-eval` — retry I/O eval failures (2 retries)
- `--no-batch` — one nix process per claim

Exit codes: `0` all cases passed, `1` any non-pass or a suite-load error, `2` usage, `101`
defect, `130` Ctrl-C — see [docs/cli.md](../../docs/cli.md#exit-codes).

### Report formats

| Format | Use case |
|--------|----------|
| `human` | Local dev (PASS/FAIL lines + summary) |
| `json` | CI artifact / machine parsing |
| `tap` | Test Anything Protocol consumers |

The run builds a typed `SuiteReport`; the pure `render_report` formats it once every case has
finished (no live per-case streaming).

## Writing tests

```nix
let assay = import ../../../nix/assay/default.nix;
in
  assay.suite "example" {
    ok = assay.eq (builtins.add 2 2) 4;
    fails = assay.throws (_: builtins.throw "nope") "nope";
  }
```

See [nix/assay/README.md](../../../nix/assay/README.md) for all claim types and authoring rules.

## runTests / nix-unit compatibility

Drop-in attrset shape is accepted for migration:

```nix
{
  myTest = {
    expr = builtins.length [1 2 3];
    expected = 3;
  };
}
```

The runner maps `expr` / `expected` to internal `eq` claims. Fixtures live under `fixtures/compat/`.

## Development

```bash
devenv shell -- cargo test -p nix-assay
devenv shell -- cargo clippy -p nix-assay --all-targets -- -D warnings
```

## Status

The runner is on id_effect 0.13 (MSRV 1.94); see [CHANGELOG.md](../../CHANGELOG.md). The Nix DSL in `nix/assay/` is stable as **data**.

Evaluator backend: process-isolated `nix eval` is the v1 Live provider; the Nix C API remains **optional** (see [SPIKE.md](SPIKE.md)).

## Related tools in devenv

`nix-unit`, `namaka`, and `nixt` remain available in devenv for comparison and migration. Assay does not remove them in v0.
