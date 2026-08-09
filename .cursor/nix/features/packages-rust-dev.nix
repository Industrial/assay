{
  lib,
  config,
  pkgs,
  ...
}: let
  cfg = config.cursor.features.packages-rust-dev;
in {
  options.cursor.features.packages-rust-dev.enable = lib.mkEnableOption "Rust toolchain helpers (cargo-*, sccache, mold, lldb)";

  config = lib.mkIf cfg.enable {
    packages = with pkgs; [
      clippy
      rust-analyzer
      rustc
      lldb
      cargo-watch
      cargo-audit
      cargo-llvm-cov
      cargo-nextest
      sccache
      mold
      llvmPackages.libclang.lib
    ];

    enterShell = lib.mkBefore ''
      mkdir -p "$HOME/.cache/sccache"
      chmod 755 "$HOME/.cache/sccache" 2>/dev/null || true
    '';
  };
}
