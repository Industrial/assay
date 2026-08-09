{
  lib,
  config,
  ...
}: let
  cfg = config.cursor.features.languages-python-uv;
in {
  options.cursor.features.languages-python-uv = {
    enable = lib.mkEnableOption "Python + uv sync via devenv languages.python";
    version = lib.mkOption {
      type = lib.types.str;
      default = "3.12";
      description = "Python version for devenv languages.python";
    };
    syncArguments = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [];
      description = "Extra args for `uv sync` (dependency groups, --no-install-project, …). Set in consumer root.";
    };
  };

  config = lib.mkIf cfg.enable {
    languages.python = {
      enable = true;
      version = cfg.version;
      uv = {
        enable = true;
        sync = {
          enable = true;
          arguments = cfg.syncArguments;
        };
      };
    };

    # uv sync installs entry points into `.devenv/state/venv/bin`.
    enterShell = lib.mkBefore ''
      export PATH="''${DEVENV_ROOT}/.devenv/state/venv/bin:''${PATH}"
    '';
  };
}
