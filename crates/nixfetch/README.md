# nixfetch

Fixed-output `fetchurl` / `fetchgit` download and flat/NAR hash verification on id_effect.

**Doctrine:** Pure hash/NAR/verify algebra; all I/O via id_effect 0.13 capabilities (`Http`, `Git`, `PathIo`; live adapters in `src/lib/live.rs`, fakes behind `test-support`); `fetch-url` / `fetch-git` retry transient failures (3 retries, 100 ms exponential backoff, jittered) with a 30 s per-attempt and 120 s overall timeout; `fetch-git` checks out into a staging dir that is removed however the run ends. CLI = `id_effect_cli::run_main` (via `nixstore::edge::run_main_coded_by`).

## CLI

```bash
nixfetch hash --flat path/to/file
nixfetch hash --recursive path/to/dir
nixfetch verify --expected sha256-… --flat path/to/file
nixfetch fetch-url https://example.com/tarball.tar.gz --hash sha256-…
nixfetch fetch-git https://example.com/repo.git --rev abc123 --hash 0…
nixfetch store-path --name pkg --hash sha256-… [--recursive]
```

Exit: `0` match/success, `1` hash mismatch (only), `2` any other failure (io, parse, http, git, timeout) or usage error, `101` defect, `130` Ctrl-C.

## Library

- Flat SHA256 and recursive NAR SHA256
- Parse expected digests: SRI, hex, Nix base32
- `fixed_output_path` via path-dep `nixdrv`

## Packaging (v1)

Devenv + moon only (`devenv shell -- cargo test -p nixfetch`, `nixfetch-coverage`). No NixOS module yet.

## Non-goals

- Store realise / register / GC
- Auto-unpack of tarballs
- Full Nix evaluator / daemon
- Assay `Claim::Fetch` (deferred)
