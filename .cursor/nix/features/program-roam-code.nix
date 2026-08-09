{
  lib,
  config,
  pkgs,
  ...
}: let
  cfg = config.cursor.features.program-roam-code;
  # https://github.com/Cranot/roam-code — pin to a tagged release with MCP support.
  roam-code-src = pkgs.fetchFromGitHub {
    owner = "Cranot";
    repo = "roam-code";
    rev = "9023ed76922d61ae4514d15e9d81b86ddfaf1569"; # v11.2.0
    hash = "sha256-hE1gihZlJUQ8e8dOOpsxQM3b2KgvPAsU4wsJclmkptc=";
  };
  roam-code = pkgs.python3Packages.buildPythonApplication rec {
    pname = "roam-code";
    version = "11.2.0";
    src = roam-code-src;
    format = "pyproject";
    nativeBuildInputs = with pkgs.python3Packages; [setuptools wheel];
    propagatedBuildInputs = with pkgs.python3Packages; [
      click
      tree-sitter
      tree-sitter-language-pack
      networkx
      fastmcp
    ];
    doCheck = false;
  };
in {
  options.cursor.features.program-roam-code.enable = lib.mkEnableOption "roam-code CLI + MCP";

  config = lib.mkIf cfg.enable {
    packages = [roam-code];
  };
}
