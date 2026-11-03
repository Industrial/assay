{
  inputs,
  config,
  lib,
  pkgs,
  ...
}: {
  imports = [./.cursor/nix];

  name = "assay";

  # Shared modules from `.cursor/nix` — defaults are off; enable what this repo needs.
  cursor.features.program-moon.enable = true;
  cursor.features.program-lean-ctx.enable = true;
  cursor.features.program-roam-code.enable = true;
  cursor.features.dotenv.enable = true;
  cursor.features.packages-base.enable = true;
  cursor.features.packages-rust-dev.enable = true;
  cursor.features.packages-formatters.enable = true;
  cursor.features.env-python-ld.enable = true;
  cursor.features.languages-javascript.enable = true;

  # The toolchain comes from rust-toolchain.toml (same file rustup reads in CI).
  # `toolchainFile` requires the channel/version left at "nixpkgs"/"latest".
  cursor.features.languages-rust = {
    enable = true;
    channel = "nixpkgs";
  };
  languages.rust.toolchainFile = ./rust-toolchain.toml;

  # uv venv at `.devenv/state/venv` — Serena MCP (`scripts/serena-mcp-wrapper.sh`);
  # see pyproject.toml [dependency-groups].
  cursor.features.languages-python-uv = {
    enable = true;
    syncArguments = [
      "--no-install-project"
      "--group"
      "serena"
    ];
  };

  cursor.features.git-hooks-moon = {
    enable = true;
    # Preserve this repo's Moon gate composition (not the shared ci-* defaults).
    preCommitTargets = ":format :check :lint :test";
    # :lint-effect (Dylint) runs on push and in CI only: its first run installs a nightly.
    prePushTargets = ":format :check :lint :lint-effect :build :test :audit :deny :check-docs";
  };
  cursor.features.git-hooks-prek.enable = true;

  # Project-only env (shared soft defaults cover CARGO_TERM_COLOR / Moon / nextest).
  env = {
    RUST_BACKTRACE = "1";
    RUSTC_WRAPPER = "sccache";
    # rustup for `scripts/dylint.sh` (Dylint builds id_effect_lints on its pinned nightly).
    # Not in `packages`: its cargo/rustc proxies would collide with the toolchain on PATH.
    ASSAY_RUSTUP_BIN = "${pkgs.rustup}/bin";
  };

  # packages-rust-dev also ships nixpkgs rustc/clippy/rustfmt, which shadow the
  # rust-overlay toolchain on PATH. id_effect 0.13 needs rustc >= 1.94, so put the
  # languages.rust toolchain first.
  enterShell = lib.mkAfter ''
    export PATH="${config.languages.rust.toolchainPackage}/bin:$PATH"
  '';

  # Project-only packages (beads removed upstream in favor of Maestro).
  packages = [
    inputs.definitively.packages.${pkgs.stdenv.hostPlatform.system}.definitively
    # `moon run :deny` (advisories, licenses, bans, sources — deny.toml).
    pkgs.cargo-deny
  ];
}
