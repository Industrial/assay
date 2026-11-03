# CLI reference

```text
assay <COMMAND>

Commands:
  run       Run suites under PATH
  discover  List discoverable suites
  laws      Emit/check law catalogue helpers
```

## `assay run`

```text
assay run [OPTIONS] <PATH>

Options:
      --json                  JSON report
      --format <FORMAT>       human | json | tap
      --update-snapshots      Refresh goldens
      --case-timeout-ms <MS>  Per-case timeout
      --retry-flaky-eval      Retry flaky eval failures
      --no-batch              One nix process per claim
```

`PATH` may be a single `*.assay.nix` file or a directory (recursive discovery).

`--case-timeout-ms` is measured from the moment a case starts running (not from when it was
queued) and bounds its retries too. `--retry-flaky-eval` retries a case up to twice (1 ms
apart) and only when evaluation failed with an I/O error; a failing claim is never retried.
A malformed suite is reported with every decode error and its path, e.g.
`cases.badProp.seed: expected i64`.

## `assay discover`

```text
assay discover <PATH>
```

Prints suite paths and kinds (e.g. `AssayNix`).

## Exit codes

Every bin runs through `id_effect_cli::run_main`. Codes `0`, `1` and `2` are each bin's own
contract, pinned by subprocess tests on the real binaries (`crates/*/tests/bin_contract.rs`).
Two codes are shared by all of them:

- `101` — a defect: a panic or other bug in the program, never an expected failure.
- `130` — Ctrl-C (SIGINT): the running program is interrupted, its finalizers run (temp
  dirs removed, child processes such as `nix` and `git` killed), then the process exits.

### `assay`

| Code | Meaning |
|------|---------|
| `0` | `run`: every case passed. `discover`, `laws`, `--version`: succeeded |
| `1` | `run`: any case did not pass (fail, eval error, snapshot mismatch, timeout, …); a suite could not be loaded (nix error, malformed suite, missing path); a bad `--format`; a `discover` / `laws` failure |
| `2` | usage error (unknown subcommand or flag) |
| `101` | defect, in the runner or in a case (a case defect wins over a failing case) |
| `130` | Ctrl-C |

### `nixq`

| Code | Meaning |
|------|---------|
| `0` | ok / predicate true / documents equal |
| `1` | predicate false (`get` path missing, `has-attrs` false), documents differ, or a typed failure of a JSON command (I/O, bad JSON, attrpath) |
| `2` | usage error, or any `path-info` failure (e.g. unknown store path) |
| `101` | defect |
| `130` | Ctrl-C |

### `nixdrv`

| Code | Meaning |
|------|---------|
| `0` | ok |
| `1` | typed failure (missing file, unparsable derivation, invalid store path or argument) |
| `2` | usage error |
| `101` | defect |
| `130` | Ctrl-C |

### `nixstore`

| Code | Meaning |
|------|---------|
| `0` | ok |
| `1` | typed failure (unknown store path, missing or unreadable `db.sqlite`) |
| `2` | usage error |
| `101` | defect |
| `130` | Ctrl-C |

### `nixfetch`

| Code | Meaning |
|------|---------|
| `0` | ok (hash computed, hash matches, fetch verified) |
| `1` | hash mismatch, and only that |
| `2` | any other failure (I/O, bad hash string, HTTP, git, timeout) or a usage error |
| `101` | defect |
| `130` | Ctrl-C (the `fetch-git` staging dir is removed) |

`fetch-url` / `fetch-git` write `--out` / the destination only after the hash verifies, so a
non-zero exit never leaves a partial or unverified result behind.
