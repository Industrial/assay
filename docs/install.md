# Install

Assay needs **Nix on `PATH`** at runtime (`assay run` shells out to `nix eval`).

## One-liners

### Nix flake (recommended)

```bash
nix run github:Industrial/assay -- --help
nix run github:Industrial/assay -- run path/to/suite.assay.nix
```

From a checkout:

```bash
nix run . -- run nix/assay/tests/smoke.assay.nix
```

### Cargo (crates.io)

```bash
cargo install nix-assay --locked
assay run path/to/suite.assay.nix
```

Building from source needs Rust **1.94** or newer (the workspace `rust-version`; id_effect 0.13's
MSRV).

### GitHub release binary

Download the asset for your platform from
[Releases](https://github.com/Industrial/assay/releases), put it on `PATH`, then:

```bash
assay --help
```

### From source (devenv)

```bash
git clone --recurse-submodules https://github.com/Industrial/assay.git
cd assay
devenv shell -- cargo install --path crates/assay --locked
```

## Verify

```bash
assay run nix/assay/tests/smoke.assay.nix   # from a checkout
# expect: 12/12 passed
```
