{
  lib,
  config,
  ...
}: let
  cfg = config.cursor.features.languages-javascript;
in {
  options.cursor.features.languages-javascript.enable = lib.mkEnableOption "JavaScript/TypeScript + Bun via devenv languages";

  config = lib.mkIf cfg.enable {
    languages = {
      javascript = {
        enable = true;
        bun.enable = true;
      };
      typescript.enable = true;
    };
  };
}
