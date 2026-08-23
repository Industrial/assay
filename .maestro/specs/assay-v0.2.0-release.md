# Spec: Assay v0.2.0 Release

Mission: release all work since v0.1.0 (benchmark CI job, simple_eval/eval.rs work,
common assert.nix helper library) as **v0.2.0** — GitHub release with binaries plus
first crates.io publication.

## Rationale

- Minor bump, not patch: new public surface ships (simple_eval module, assert.nix
  helper library, bench job). SemVer 0.x convention: minor = compatible additions.
- crates.io publish closes the gap between docs/install.md (which already promises
  `cargo install nix-assay`) and reality.

## Release blockers found in research

| # | Blocker | Evidence |
|---|---------|----------|
| B1 | `assert.nix` (root) untracked but imported by tracked `nix/assay/tests/dogfood.assay.nix` | fresh clone breaks |
| B2 | Stray divergent copy `nix/assert.nix` (stub `typ: list: list`) | confusing dead variant |
| B3 | CI runs smoke only, not dogfood | B1 class undetectable in CI |

## Tasks (execution order)

### t1. Consolidate assert.nix (fixes B1+B2)
- Keep root `assert.nix` (functional copy, the one dogfood imports).
- Delete stray `nix/assert.nix`.
- `git add assert.nix`; commit `feat(nix): add common assert.nix helper library`.
- **Accept:** `git ls-files | grep '^assert.nix$'` non-empty; `nix/assert.nix` gone;
  local dogfood suite passes.

### t2. Gate dogfood suite in CI (fixes B3)
- Extend `.github/workflows/ci.yml` smoke step to also run
  `nix/assay/tests/dogfood.assay.nix`.
- Commit `test(ci): run dogfood suite alongside smoke`.
- **Accept:** ci.yml contains dogfood invocation; local run of both suites green.

### t3. Version bump to 0.2.0
- Adopt workspace inheritance intended by root Cargo.toml comment: all 5 crate
  manifests use `version.workspace = true` (drop literal versions); keep metadata.
- Bump `workspace.package.version` → `0.2.0`; bump inter-crate dep reqs
  (nixq/nixdrv/nixstore/nixfetch/assay edges) → `version = "0.2.0"`.
- Sync `flake.nix` version string; refresh Cargo.lock.
- Commit `chore(release): bump workspace to 0.2.0`.
- **Accept:** `cargo check --workspace` green; `cargo metadata` shows 0.2.0 for all
  5 packages; flake evaluates with 0.2.0.

### t4. Docs truth-check
- Verify README/docs/install.md claims still hold post-change (smoke expected-pass
  count "12/12", install commands, no stale version strings).
- Fix anything drifted; single docs commit if needed.
- **Accept:** every command in docs/install.md either verified locally or removed.

### t5. Full local quality gate sweep
- `moon check`, `moon lint` (fmt+clippy -D warnings), `moon test` (nextest),
  `moon audit`, `moon assay` (smoke+dogfood), quick `cargo bench` sanity.
- **Accept:** all green on final tree. No new commits unless a gate forces a fix
  (then re-run affected gates).

### t6. Push, tag, GitHub release
- `git pull --rebase && git push origin main` (prek pre-push runs moon gates).
- `git tag v0.2.0 && git push origin v0.2.0` → release.yml builds
  linux-x86_64 + macOS arm64/x86_64 tarballs, creates GH release with generated notes.
- **Accept:** Actions release run succeeds; release v0.2.0 lists 3 `.tar.gz` assets;
  `main` up to date with origin.

### t7. Publish to crates.io (ordered)
- Dry-run each: `cargo publish --dry-run -p <crate>` in dep order
  `nixstore → nixq → nixdrv → nixfetch → nix-assay`.
- Real publish same order (requires user's `cargo login` / CARGO_REGISTRY_TOKEN).
- **Accept:** crates.io API reports `nix-assay` max_version 0.2.0 (and deps present);
  `cargo install nix-assay --locked` succeeds in a temp dir (optional, slow).

## Waves

```
t1 ─┬─> t2 ─> t3 ─> t4 ─> t5 ─> t6 ─> t7
    └(t1/t2 independent, but commit order enforced sequentially)
```

## Risks / open points

- crates.io publish needs user credentials; if unavailable, ship t1–t6 and defer t7.
- `id_effect` deps already come from crates.io (0.4.0), so publish chain is clean.
- Name collision: plain `assay` on crates.io is an unrelated crate — we publish
  `nix-assay` only (matches docs).
- Optional: initialize CHANGELOG.md (GH generated notes otherwise suffice).
