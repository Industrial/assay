{
  lib,
  config,
  pkgs,
  ...
}: let
  cfg = config.cursor.features.program-lean-ctx;
  # https://github.com/yvgude/lean-ctx — pin a release binary (avoids nixpkgs rustc
  # building rten-gemm, which fails on rustc 1.97+ AVX512 VNNI intrinsic mismatch).
  # Named lean-ctx-pkg so `with pkgs` does not pick nixpkgs' `lean-ctx`.
  lean-ctx-pkg = pkgs.stdenv.mkDerivation {
    pname = "lean-ctx";
    version = "3.9.18";
    src = pkgs.fetchurl {
      url = "https://github.com/yvgude/lean-ctx/releases/download/v3.9.18/lean-ctx-x86_64-unknown-linux-gnu.tar.gz";
      hash = "sha256-jjZ2sqM5TjN4Faj+Uqo9VtR/GY/60mbHg+uHDnqeZng=";
    };
    nativeBuildInputs = [pkgs.autoPatchelfHook];
    buildInputs = [pkgs.stdenv.cc.cc.lib];
    sourceRoot = ".";
    installPhase = ''
      runHook preInstall
      mkdir -p $out/bin
      install -m755 lean-ctx $out/bin/lean-ctx
      runHook postInstall
    '';
    meta = {
      description = "lean-ctx MCP + CLI (pinned GitHub release)";
      homepage = "https://github.com/yvgude/lean-ctx";
      license = pkgs.lib.licenses.mit;
      platforms = pkgs.lib.platforms.linux;
    };
  };
in {
  options.cursor.features.program-lean-ctx.enable = lib.mkEnableOption "lean-ctx MCP + CLI";

  config = lib.mkIf cfg.enable {
    packages = [lean-ctx-pkg];
    env.LEAN_CTX_COMPRESS = lib.mkDefault "1";
  };
}
