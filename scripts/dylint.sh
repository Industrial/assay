#!/usr/bin/env bash
# id_effect_lints (Dylint) over the workspace; any warning fails. `moon run :lint-effect`, CI.
#
# The library is pinned by [workspace.metadata.dylint] in Cargo.toml. It is checked out here
# without submodules (`cargo dylint --all` would let cargo fetch the git dep, and cargo fails on
# a nested submodule of id_effect's `.cursor`), then run with `--path`.
#
# Needs rustup: Dylint builds the library with the nightly pinned in its rust-toolchain.toml.
# In devenv, rustup comes from $ASSAY_RUSTUP_BIN with its own RUSTUP_HOME under .devenv/state;
# cargo-dylint / dylint-link are installed on first use (`cargo install`, see README.md "Development").
set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

# Keep in step with the tag in Cargo.toml and lints/id_effect_lints/rust-toolchain.toml there.
ID_EFFECT_TAG="v0.13.0"
LINTS_TOOLCHAIN="nightly-2026-08-20"
DYLINT_VERSION="6.2.0"

if ! command -v rustup >/dev/null; then
    if [[ -z "${ASSAY_RUSTUP_BIN:-}" ]]; then
        echo "dylint.sh: rustup not found (run inside 'devenv shell', or install rustup)" >&2
        exit 1
    fi
    export RUSTUP_HOME="${RUSTUP_HOME_DYLINT:-${DEVENV_STATE:-$REPO_ROOT/.devenv/state}/rustup}"
    PATH="$ASSAY_RUSTUP_BIN:$PATH"
fi
# rustup's cargo proxy must come first: dylint-link reads RUSTUP_TOOLCHAIN, which the proxy sets.
PATH="$(dirname "$(command -v rustup)"):$PATH"
export PATH

rustup toolchain install "$LINTS_TOOLCHAIN" --profile minimal \
    --component rustc-dev,rust-src,llvm-tools-preview >/dev/null

tools_root=""
if [[ -n "${DEVENV_STATE:-}" ]]; then
    tools_root="$DEVENV_STATE/dylint-tools"
    PATH="$tools_root/bin:$PATH"
fi
if ! command -v cargo-dylint >/dev/null || ! command -v dylint-link >/dev/null; then
    # cargo-dylint's lockfile needs a newer rustc than the workspace MSRV: build it on the nightly.
    cargo "+$LINTS_TOOLCHAIN" install --locked ${tools_root:+--root "$tools_root"} \
        "cargo-dylint@$DYLINT_VERSION" "dylint-link@$DYLINT_VERSION"
fi

lints_src="target/dylint/id_effect-$ID_EFFECT_TAG"
if [[ ! -d "$lints_src/lints/id_effect_lints" ]]; then
    rm -rf "$lints_src"
    git clone --quiet --depth 1 --branch "$ID_EFFECT_TAG" -c advice.detachedHead=false \
        https://github.com/Industrial/id_effect "$lints_src"
fi

# Keep sccache out of the lint run (Dylint drives rustc through RUSTC_WORKSPACE_WRAPPER).
unset RUSTC_WRAPPER
DYLINT_RUSTFLAGS="-D warnings" cargo "+$LINTS_TOOLCHAIN" dylint \
    --path "$lints_src/lints/id_effect_lints" -- --workspace --all-targets --all-features
