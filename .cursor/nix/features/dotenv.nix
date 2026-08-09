{
  lib,
  config,
  ...
}: let
  cfg = config.cursor.features.dotenv;
in {
  options.cursor.features.dotenv.enable = lib.mkEnableOption "devenv dotenv loading";

  config = lib.mkIf cfg.enable {
    dotenv.enable = true;
  };
}
