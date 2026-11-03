# Contributing

Thanks for helping with Assay. This guide covers the dev environment, the gates a change must
pass, and the conventions the release tooling depends on. Testing details live in
[TESTING.md](TESTING.md), releases in [docs/RELEASING.md](docs/RELEASING.md), and security
reports go through [SECURITY.md](SECURITY.md) — not public issues.

## Setup

You need [Nix](https://nixos.org/download) (flakes enabled) and [devenv](https://devenv.sh).

```bash
git clone --recurse-submodules https://github.com/Industrial/assay.git
cd assay
git submodule update --init   # if cloned without --recurse-submodules
devenv shell                  # toolchain from rust-toolchain.toml, moon, treefmt, nextest, …
```

The devenv shell imports shared Nix modules from
[`Industrial/cursor-setup`](https://github.com/Industrial/cursor-setup) as a pinned devenv
input. The only submodule is `.claude-setup`
([`Industrial/claude-setup`](https://github.com/Industrial/claude-setup); `.claude` symlinks
into it), which holds agent configuration and is not needed to build. The Rust toolchain is
pinned by `rust-toolchain.toml`; the MSRV is **1.94** (`rust-version` in `Cargo.toml`,
required by id_effect 0.13).

Entering `devenv shell` installs the git hooks. The hooks are shared by every worktree of a
clone and point at the config of the checkout that installed them, so enter devenv in the
checkout (or worktree) you commit from.

`./init.sh` only checks the [Maestro](AGENTS.md#maestro) agent harness (`maestro doctor`,
`maestro status`; `maestro` comes from the dev shell); you do not need it to build or test.

## Daily loop

Inside `devenv shell` (or prefix with `devenv shell --`):

```bash
moon run :check :test        # compile + nextest + doctests
moon run :assay              # Nix DSL smoke + dogfood suites (needs nix)
cargo nextest run -p nixq    # one crate
```

| Task | What it runs |
|------|--------------|
| `:build` | `cargo build` |
| `:check` | `cargo check --workspace --all-features` |
| `:test` | `cargo nextest run` (profile `default`) and `cargo test --doc` |
| `:lint` | clippy on all targets and features, `-D warnings` |
| `:lint-effect` | id_effect Dylint lints via [`scripts/dylint.sh`](scripts/dylint.sh); any warning fails |
| `:format` / `:ci-format` | treefmt (Rust, Nix, shell, YAML, TOML, …); the CI variant fails on change |
| `:check-docs` | rustdoc with `-D warnings` |
| `:deny` | `cargo deny` (licenses, bans, sources, advisories) |
| `:audit` | `cargo audit` |
| `:coverage` | `cargo llvm-cov nextest` (HTML report) |
| `:assay` | builds `assay`, runs `nix/assay/tests/{smoke,dogfood}.assay.nix` |

nextest profiles (`default`, `ci`, `coverage`) are in `.config/nextest.toml`.

## Before you push

The git hooks run the gates for you: pre-commit runs a fast subset, pre-push mirrors CI. To run
the full set by hand:

```bash
moon run :ci-format :check :lint :lint-effect :test :check-docs :deny :audit :assay
```

The first `:lint-effect` run installs the lints' pinned nightly and `cargo-dylint` /
`dylint-link` (versions in `scripts/dylint.sh`) under `.devenv/state/`.

CI additionally checks the MSRV, every feature combination (`cargo hack --feature-powerset`),
semver compatibility (`cargo semver-checks`), and runs tests on Linux and macOS. The `ci-pass`
job aggregates them and is the required check on `main`.

## Commits and pull requests

- Use [Conventional Commits](https://www.conventionalcommits.org/): `feat:`, `fix:`, `docs:`,
  `refactor:`, `perf:`, `test:`, `ci:`, `chore:`; a scope is welcome (`fix(nixfetch): …`);
  mark breaking changes with `!` or a `BREAKING CHANGE:` footer.
- Add an entry under `## [Unreleased]` in `CHANGELOG.md` for every user-visible change
  (Added / Changed / Fixed / Removed / Security), written for a user. Crate versions change only
  in a release PR ([docs/RELEASING.md](docs/RELEASING.md)).
- Keep PRs focused. Fill in the PR template; update `docs/` and rustdoc when behaviour changes.
- A change to a bin's exit codes or stdout is a contract change: update its goldens and
  `docs/cli.md` in the same PR, and say so in the description.

## Code conventions

[AGENTS.md](AGENTS.md) is the short version. In brief: I/O goes through id_effect
capabilities (a trait returning `Effect`, a newtype service, an env alias and a `layer()`);
live adapters live in `live.rs`; in-memory fakes sit behind
`#[cfg(any(test, feature = "test-support"))]`. `unsafe` is denied workspace-wide.

## Adding a crate

1. Create `crates/<name>/` with `src/lib/mod.rs` (and `src/main.rs` for a bin), and add it to
   `members` in the root `Cargo.toml`.
2. Inherit shared metadata and lints:

   ```toml
   [package]
   name = "<name>"
   version.workspace = true
   edition.workspace = true
   rust-version.workspace = true
   license.workspace = true
   authors.workspace = true
   repository.workspace = true
   homepage.workspace = true
   description = "…"

   [lints]
   workspace = true

   [lib]
   path = "src/lib/mod.rs"
   ```

3. Depend on sibling crates with both `path` and `version` (needed to publish), and keep the
   id_effect version in step with the other crates.
4. Add a `tests/bin_contract.rs` for any bin (see [TESTING.md](TESTING.md)), a `README.md`,
   a row in the root README's crate table, and a flake package in `flake.nix` if it ships a
   binary.

All crates share one version and are released together, so a new crate starts at the current
workspace version.

## Updating goldens

Golden files are byte-exact; never edit or reformat them by hand. Regenerate, then review the
diff like code:

```bash
BLESS=1 cargo test -p nixq --test bin_contract      # crates/*/tests/golden/*.stdout
ID_EFFECT_BLESS=1 cargo test -p nix-assay           # crates/assay/testdata/exits/*
```

`assay` bin-contract cases that evaluate Nix are skipped when `nix` is not on `PATH`; bless
those from inside `devenv shell`.

## License

By contributing you agree that your contributions are licensed under the [MIT License](LICENSE).
