# Assay

**Nix unit testing** with a claim algebra authored in Nix and an isolated Rust runner
(`id_effect`). Catch throws per case, compare derivations safely, and report with
human / JSON / TAP formats.

[![CI](https://github.com/Industrial/assay/actions/workflows/ci.yml/badge.svg?branch=main)](https://github.com/Industrial/assay/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/nix-assay.svg)](https://crates.io/crates/nix-assay)
[![docs.rs](https://docs.rs/nix-assay/badge.svg)](https://docs.rs/nix-assay)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)

## Install

```bash
# Nix (no install)
nix run github:Industrial/assay -- run path/to/suite.assay.nix

# Prebuilt binary: x86_64 / aarch64, unknown-linux-gnu / apple-darwin
t=x86_64-unknown-linux-gnu
gh release download --repo Industrial/assay --pattern "assay-$t.tar.gz*"
shasum -a 256 -c "assay-$t.tar.gz.sha256" && tar -xzf "assay-$t.tar.gz"
install -m 0755 "assay-$t/assay" ~/.local/bin/assay

# Cargo
cargo install nix-assay --locked   # installs the `assay` binary
```

`assay run` needs `nix` on `PATH`.

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
devenv shell -- moon run :format :check :test
devenv shell -- moon run :assay   # DSL smoke + dogfood
devenv shell -- moon run :lint-effect   # id_effect_lints via Dylint (also pre-push + CI)
```

`:lint-effect` runs [`scripts/dylint.sh`](scripts/dylint.sh). Its first run installs the
lints' pinned nightly (rustup, under `.devenv/state/rustup`) and `cargo-dylint` /
`dylint-link` (under `.devenv/state/dylint-tools`); the script pins both versions. Outside
devenv it needs `rustup` and installs the tools into cargo's default bin dir. The workspace needs Rust 1.94+ (MSRV; the toolchain is pinned in
`rust-toolchain.toml`); see [CHANGELOG.md](CHANGELOG.md) for releases.

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for setup, the gates to run and commit conventions,
[TESTING.md](TESTING.md) for the test layers, and [SECURITY.md](SECURITY.md) to report a
vulnerability.

## License

MIT
