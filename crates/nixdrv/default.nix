# Convenience wrapper: cargo-built `nixdrv` from the workspace root.
{
  lib,
  pkgs,
  cargo ? pkgs.cargo,
  rustc ? pkgs.rustc,
  ...
}:
pkgs.writeShellApplication {
  name = "nixdrv";
  runtimeInputs = [cargo rustc pkgs.gcc pkgs.pkg-config pkgs.nix pkgs.git];
  text = ''
    set -euo pipefail
    root="''${DEVENV_ROOT:-}"
    if [[ -z "$root" ]]; then
      root="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
    fi
    manifest="$root/Cargo.toml"
    bin="$root/target/debug/nixdrv"
    if [[ ! -f "$manifest" ]]; then
      echo "nixdrv: cannot find workspace Cargo.toml at $manifest" >&2
      exit 127
    fi
    if [[ ! -x "$bin" ]] || [[ -n "$(find "$root/crates" -name '*.rs' -newer "$bin" 2>/dev/null | head -n1)" ]]; then
      cargo build -p nixdrv --manifest-path "$manifest"
    fi
    exec "$bin" "$@"
  '';
}
