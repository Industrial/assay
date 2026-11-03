# Project Conventions

Repo-level conventions for agents working in this codebase. The harness pointer surface
lives in `.maestro/AGENTS.md`; this file holds code conventions, build commands, and
feature boundaries. Human contributors: see [CONTRIBUTING.md](CONTRIBUTING.md) and
[TESTING.md](TESTING.md).

## Build / test / verify

Run inside `devenv shell` (or prefix each with `devenv shell --`). The toolchain is pinned by
`rust-toolchain.toml`; MSRV is 1.94.

```bash
moon run :build         # cargo build
moon run :check         # cargo check --workspace --all-features
moon run :test          # cargo nextest run (profile default) + cargo test --doc
moon run :lint          # clippy, all targets and features, -D warnings
moon run :lint-effect   # id_effect Dylint lints (scripts/dylint.sh); any warning fails
moon run :format        # treefmt (writes); :ci-format checks without writing
moon run :check-docs    # rustdoc with -D warnings
moon run :deny          # cargo deny (licenses, bans, sources, advisories)
moon run :audit         # cargo audit
moon run :assay         # Nix suites: nix/assay/tests/{smoke,dogfood}.assay.nix
```

Before claiming a task done: `:format :check :lint :test`, plus `:lint-effect` and `:assay`
when Rust or Nix DSL code changed. The pre-push hook mirrors CI.

## Layout

- `crates/<name>/` — one crate per directory: `assay` (package `nix-assay`, bin `assay`),
  `nixq`, `nixdrv`, `nixstore`, `nixfetch`.
  - `src/lib/mod.rs` — the library (`[lib] path`); `src/main.rs` — the bin (CLI edge only).
  - `src/lib/caps.rs` — capabilities; `src/lib/live.rs` — live adapters; `src/lib/fake.rs`
    or a `fake` module in `caps.rs` — in-memory fakes.
  - `tests/bin_contract.rs` + `tests/golden/*.stdout` — subprocess exit-code/stdout goldens.
  - `fixtures/`, `testdata/` — inputs and in-crate goldens.
- `crates/bin-contract-support/` — the shared `bin_case!` harness (`publish = false`, a
  path-only dev-dependency of each crate's `tests/bin_contract.rs`).
- `nix/assay/` — the Nix DSL (`default.nix`) and its suites (`tests/smoke.assay.nix`,
  `tests/dogfood.assay.nix`); `nix/assert.nix` is the assertion library dogfood exercises.
- `examples/quickstart/` — consumer example. `docs/` — user docs (`cli.md` holds the
  exit-code tables). `scripts/` — `dylint.sh` and MCP wrappers. The `program-assay.nix` module
  Nix consumers use lives upstream in `Industrial/cursor-setup`.
- `.maestro/` — harness state (read `.maestro/AGENTS.md` first).

## Conventions

- **Capabilities.** I/O goes through a trait whose methods return `Effect<A, E>`, a newtype
  service (`JsonSourceService(Arc<dyn JsonSource>)`), an env alias (`caps!(…)`) and a
  `layer()` that builds the live env. Reference: `crates/nixq/src/lib/caps.rs`.
- **Live I/O only in `live.rs`** (or the crate's named adapter module, e.g. `nixstore`'s
  `db.rs`). Domain code never touches the filesystem, processes or the clock directly.
- **Fakes** live behind `#[cfg(any(test, feature = "test-support"))]`; dependents enable the
  `test-support` feature in `[dev-dependencies]`.
- **Exit codes are a contract.** `0`/`1`/`2` per bin, `101` defect, `130` Ctrl-C. Pinned by
  `crates/*/tests/bin_contract.rs` and documented in `docs/cli.md`; change both together.
- **Golden files are byte-exact.** Never reformat, re-indent or strip whitespace in
  `tests/golden/` or `testdata/`; regenerate them (`BLESS=1`, `ID_EFFECT_BLESS=1`; see
  TESTING.md) and review the diff.
- **Lints.** `[workspace.lints]` in the root `Cargo.toml`; every crate has
  `[lints] workspace = true`. Silence an `id_effect_lints` false positive with a narrow
  `#[cfg_attr(dylint_lib = "id_effect_lints", allow(…))]` and a comment saying why.
- **Metadata and versions** inherit from `[workspace.package]`; all five crates share one
  version.
- **Conventional Commits** (`feat:`, `fix:`, `docs:`, `refactor:`, `test:`, `ci:`, `chore:`;
  `!` for breaking). User-visible changes also get an entry under `## [Unreleased]` in
  `CHANGELOG.md`; versions change only in a release PR ([docs/RELEASING.md](docs/RELEASING.md)).
- Match existing code style; use established libraries before adding new ones.
- Surgical edits only — touch what the task requires.

## See also

- `.maestro/MAESTRO.md` — read order, lane policy, daily commands
- `.maestro/docs/HARNESS.md` — product-delta vs harness-delta model
- `.maestro/docs/FEATURE_INTAKE.md` — work-type classification decision tree
- `.maestro/docs/VALIDATION_LADDER.md` — 7-step verification protocol

<!-- maestro-setup:start -->
## Maestro

This project is wired into the Maestro harness. State and config live
under `.maestro/`. Run `./init.sh` to bring a fresh checkout up; run
`maestro doctor` and `maestro status` to see what Maestro knows.

Preserve content outside this managed block; the block is rewritten by
`maestro setup` and the `maestro-setup` skill, but everything else in
this file is yours.
<!-- maestro-setup:end -->
