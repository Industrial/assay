{
  lib,
  config,
  pkgs,
  ...
}: let
  cfg = config.cursor.features.packages-formatters;
in {
  options.cursor.features.packages-formatters.enable = lib.mkEnableOption "formatters and linters (treefmt, alejandra, biome, …)";

  config = lib.mkIf cfg.enable {
    packages = with pkgs; [
      actionlint
      alejandra
      beautysh
      biome
      deadnix
      rustfmt
      taplo
      treefmt
      vulnix
      yamlfmt
    ];
  };
}
