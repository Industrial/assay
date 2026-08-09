# Assay

**Nix unit testing** with a claim algebra authored in Nix and an isolated Rust runner
(`id_effect`). Catch throws per case, compare derivations safely, and report with
human / JSON / TAP formats.

[![crates.io](https://img.shields.io/crates/v/assay.svg)](https://crates.io/crates/assay)
[![docs.rs](https://docs.rs/assay/badge.svg)](https://docs.rs/assay)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Install

```bash
# Nix (no install)
nix run github:Industrial/assay -- run path/to/suite.assay.nix

# Cargo
cargo install assay --locked
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
| [`assay`](crates/assay) | Runner CLI |
| [`nixq`](crates/nixq) | JSON / attrpath query |
| [`nixdrv`](crates/nixdrv) | Derivation helpers |
| [`nixstore`](crates/nixstore) | Store path-info |
| [`nixfetch`](crates/nixfetch) | Fixed-output fetch / hash verify |

## Development

```bash
devenv shell -- cargo test --workspace
devenv shell -- moon run :format :check :test
devenv shell -- moon run :assay   # DSL smoke + dogfood
```

## License

MIT
