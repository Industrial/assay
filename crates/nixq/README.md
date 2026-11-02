# nixq

JSON/attrpath query library and CLI on [id_effect](https://github.com/).

**Doctrine:** pure value algebra (`get`, `hasAttrs`, `subset`, `normalize`, `diff`, `force-path`); I/O via the `JsonSource` capability (`JsonSourceService`, `layer()`) + `effect!`; commands return typed outputs rendered at the edge; CLI runs through `id_effect_cli::run_main` (via `nixstore::edge::run_main_coded`). `path-info` composes `nixstore` Effects.

## CLI

```bash
nixq get a.b[0] -f value.json
echo '{"a":1}' | nixq has-attrs a
nixq subset -f actual.json --expected expect.json
nixq force-path a.b -f value.json
nixq normalize -f value.json
nixq diff -f left.json --right right.json
```

Exit: `0` = true / ok, `1` = false / documents differ / I/O or parse failure, `2` = usage error or any `path-info` failure, `101` = defect, `130` = Ctrl-C.

## Library

Assay depends on this crate for value-mode helpers (`value_has_attrs`, `value_contains_subset`, `normalize_value`, `structural_diff`).

## Packaging (v1)

Devenv + moon only (`devenv shell -- nixq`, `moon run :nixq-test` / `:nixq-coverage`). No NixOS `features/cli/nixq` module yet.
