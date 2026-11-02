# nixstore

Read-only Nix store **path-info** over `db.sqlite` (tier 3 of the Nix→Rust ladder).

**Doctrine:** query `ValidPaths` / `Refs` via the `PathInfoStore` capability (`PathInfoService`, `NixstoreEnv = caps!(PathInfoService)`, `layer(store_root)` opens the connection once) + `effect!`; never shell out to `nix`; never write the DB. `path_info` returns a typed `PathInfoReport` rendered at the edge; CLI = `id_effect_cli::run_main`.

## Library

- `open_db` / `SqlitePathInfoStore` — SQLite URI `mode=ro`
- `PathInfo` — v1-shaped JSON fields (`narHash` SRI, full paths, `references`)
- `PathInfoStore` trait (methods return `Effect`) + `MockPathInfoStore` (only under `cfg(test)` / the `test-support` feature)
- `edge` — the exit wrappers (`run_main_coded`, `run_main_coded_by`) shared by every bin in the workspace
- Closure size via BFS over references

Assay (`pathInfo` claim) and nixq (`path-info` subcommand) depend on this crate.

**Dependency rule:** no `nixq` / `nixdrv` deps (avoids workspace cycle: `nixdrv` → `nixq` → `nixstore`).

## CLI

```bash
nixstore path-info [--json] [-s|--size] [-S|--closure-size] [-h|--human-readable] \
  [--sigs] [--referrers] [--store DIR] PATH...
```

Default DB: `{store}/var/nix/db/db.sqlite` with `store` defaulting to `/nix`.

Exit: `0` ok, `1` typed failure (unknown path, missing DB), `2` usage error, `101` defect, `130` Ctrl-C.

## Packaging (v1)

Devenv + moon only (`cargo test -p nixstore`). No NixOS module yet.

## Non-goals

- GC delete / roots planner
- Daemon / remote store protocol
- NAR walk / hash from filesystem
- `--all` full-store dump
- JSON format v2/v3
