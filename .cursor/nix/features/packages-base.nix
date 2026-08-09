{
  lib,
  config,
  pkgs,
  ...
}: let
  cfg = config.cursor.features.packages-base;
in {
  options.cursor.features.packages-base.enable = lib.mkEnableOption "base devenv packages (git, direnv, prek, jq, …)";

  config = lib.mkIf cfg.enable {
    packages = with pkgs; [
      cmake
      cachix
      perl
      direnv
      prek
      git
      commitizen
      gh
      zlib
      stdenv.cc.cc.lib
      jq
    ];
  };
}
