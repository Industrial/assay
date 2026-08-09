{
  lib,
  config,
  pkgs,
  ...
}: let
  cfg = config.cursor.features.env-python-ld;
  soft = lib.mkOverride 1500;
in {
  options.cursor.features.env-python-ld.enable = lib.mkEnableOption "LD_LIBRARY_PATH for uv/numpy wheels on NixOS";

  config = lib.mkIf cfg.enable {
    env.LD_LIBRARY_PATH = soft "${pkgs.zlib}/lib:${pkgs.stdenv.cc.cc.lib}/lib";
  };
}
