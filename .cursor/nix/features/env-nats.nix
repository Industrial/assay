{
  lib,
  config,
  pkgs,
  ...
}: let
  cfg = config.cursor.features.env-nats;
in {
  options.cursor.features.env-nats = {
    enable = lib.mkEnableOption "NATS CLI package (env vars left to dotenv / consumer root)";
  };

  config = lib.mkIf cfg.enable {
    # Do not set NATS_URL / NATS_PUBLISH_ENABLED here — they commonly come from
    # `.env` via devenv dotenv and conflict at mkDefault priority.
    packages = [pkgs.natscli];
  };
}
