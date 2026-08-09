# cursor-setup integration

Copy `program-assay.nix` into `Industrial/cursor-setup` at `nix/features/program-assay.nix`
and add to the barrel import in `nix/devenv.nix`:

```nix
./features/program-assay.nix
```

Enable in a consumer `devenv.nix`:

```nix
cursor.features.program-assay.enable = true;
```

The `.cursor` submodule in this repo may be updated once that lands upstream.
