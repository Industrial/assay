{
  lib,
  config,
  pkgs,
  ...
}: let
  cfg = config.cursor.features.program-moon;
  # Moon from GitHub releases (x86_64-linux). See https://moonrepo.dev/docs/install
  moon = pkgs.stdenv.mkDerivation {
    pname = "moon-cli";
    version = "2.4.6";
    src = pkgs.fetchurl {
      url = "https://github.com/moonrepo/moon/releases/download/v2.4.6/moon_cli-x86_64-unknown-linux-gnu.tar.xz";
      sha256 = "15qxylhy5s9p1dm1w87a0sh8cx98gqx4phcnlf2r8gnrvlc4nshh";
    };
    nativeBuildInputs = [pkgs.autoPatchelfHook];
    buildInputs = [pkgs.stdenv.cc.cc.lib];
    installPhase = ''
      runHook preInstall
      mkdir -p $out/bin
      install -m755 moon $out/bin/moon
      runHook postInstall
    '';
    meta = {
      description = "Moon CLI (moonrepo)";
      homepage = "https://moonrepo.dev";
      license = pkgs.lib.licenses.mit;
      platforms = pkgs.lib.platforms.linux;
    };
  };
in {
  options.cursor.features.program-moon.enable = lib.mkEnableOption "Moon task runner CLI (pinned release)";

  config = lib.mkIf cfg.enable {
    packages = [moon];

    scripts.moon-sync.exec = ''
      moon sync
    '';

    enterShell = lib.mkBefore ''
      moon-sync
    '';
  };
}
