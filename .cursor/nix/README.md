# `.cursor/nix` — shared devenv feature modules

Composable [devenv](https://devenv.sh) modules for Industrial repos.

## Layout

```text
.cursor/nix/
  default.nix                 # barrel: imports every feature module
  devenv.yaml                 # optional shared input defaults
  features/
    program-*.nix             # pinned CLI derivations
    *.nix                     # packages, env, languages, hooks, enterShell
```

## Enable API

Each feature defaults to **off**. In the **consumer repo** root `devenv.nix`:

```nix
{ pkgs, lib, ... }: {
  imports = [ ./.cursor/nix ];

  cursor.features.program-moon.enable = true;
  cursor.features.program-lean-ctx.enable = true;
  cursor.features.program-roam-code.enable = true;
  cursor.features.dotenv.enable = true;
  cursor.features.packages-base.enable = true;
  cursor.features.packages-rust-dev.enable = true;
  cursor.features.packages-formatters.enable = true;
  cursor.features.env-python-ld.enable = true;
  cursor.features.languages-javascript.enable = true;
  cursor.features.languages-rust.enable = true;
  cursor.features.languages-python-uv.enable = true;
  cursor.features.git-hooks-moon.enable = true;
  cursor.features.git-hooks-prek.enable = true;
  # optional:
  # cursor.features.env-bindgen.enable = true;
  # cursor.features.env-nats.enable = true;

  # Project-only overrides (mkForce / mkAfter) live here — not in this submodule.
}
```

## Naming

Identifiers and paths in this tree stay **project-agnostic**. Consumer-specific
wiring belongs in the importing repository's root `devenv.nix`.
