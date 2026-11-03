# Assay

**Nix unit testing** with a claim algebra authored in Nix and an isolated Rust runner
(`id_effect`). Catch throws per case, compare derivations safely, and report with
human / JSON / TAP formats.

[![crates.io](https://img.shields.io/crates/v/nix-assay.svg)](https://crates.io/crates/nix-assay)
[![docs.rs](https://docs.rs/nix-assay/badge.svg)](https://docs.rs/nix-assay)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Install

```bash
# Nix (no install)
nix run github:Industrial/assay -- run path/to/suite.assay.nix

# Cargo
cargo install nix-assay --locked   # installs the `assay` binary
```

More options: [docs/install.md](docs/install.md).

## 5-minute quickstart

```bash
git clone --recurse-submodules https://github.com/Industrial/assay.git
cd assay
nix run . -- run nix/assay/tests/smoke.assay.nix
```

Author a suite:

```nix
# my-suite.assay.nix
let
  assay = import ./nix/assay/default.nix;
in
  assay.suite "demo" {
    add = assay.eq (1 + 1) 2;
  }
```

```bash
assay run my-suite.assay.nix
```

## Documentation

| Doc | Topic |
|-----|--------|
| [docs/install.md](docs/install.md) | Install matrix |
| [docs/authoring.md](docs/authoring.md) | Writing suites |
| [docs/cli.md](docs/cli.md) | CLI reference |
| [docs/comparison.md](docs/comparison.md) | vs nix-unit / runTests / namaka |
| [nix/assay/README.md](nix/assay/README.md) | DSL deep dive |
| [examples/quickstart](examples/quickstart) | Consumer example |

## Workspace crates

| Crate | Role |
|-------|------|
| [`nix-assay`](crates/assay) (`assay` binary) | Runner CLI |
| [`nixq`](crates/nixq) | JSON / attrpath query |
| [`nixdrv`](crates/nixdrv) | Derivation helpers |
| [`nixstore`](crates/nixstore) | Store path-info |
| [`nixfetch`](crates/nixfetch) | Fixed-output fetch / hash verify |

## Development

```bash
devenv shell -- cargo test --workspace
devenv shell -- moon run :format :check :test
devenv shell -- moon run :assay   # DSL smoke + dogfood
devenv shell -- moon run :lint-effect   # id_effect_lints via Dylint (also pre-push + CI)
```

`:lint-effect` runs [`scripts/dylint.sh`](scripts/dylint.sh). Its first run installs the
lints' pinned nightly (rustup, under `.devenv/state/rustup`) and `cargo-dylint` /
`dylint-link` 6.2.0 (under `.devenv/state/dylint-tools`). Outside devenv it needs `rustup`;
install the tools yourself with
`cargo +nightly-2026-08-20 install --locked cargo-dylint@6.2.0 dylint-link@6.2.0`.
The workspace needs Rust 1.94+ (MSRV); see [CHANGELOG.md](CHANGELOG.md) for releases.

## License

MIT
