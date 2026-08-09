{
  lib,
  config,
  ...
}: let
  cfg = config.cursor.features.languages-rust;
in {
  options.cursor.features.languages-rust = {
    enable = lib.mkEnableOption "Rust via devenv languages.rust (rust-overlay)";
    channel = lib.mkOption {
      type = lib.types.str;
      default = "nightly";
      description = "rust-overlay channel: stable | nightly";
    };
    version = lib.mkOption {
      type = lib.types.str;
      default = "latest";
      description = "Toolchain version pin (e.g. latest, 1.xx.0)";
    };
  };

  config = lib.mkIf cfg.enable {
    languages.rust = {
      enable = true;
      channel = cfg.channel;
      version = cfg.version;
      components = [
        "cargo"
        "clippy"
        "rust-analyzer"
        "rustc"
        "rustfmt"
        "llvm-tools"
      ];
      targets = [];
    };
  };
}
