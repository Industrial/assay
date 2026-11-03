# Releasing

Releases are automated: [release-plz](https://release-plz.dev) prepares and publishes the
crates, [cargo-dist](https://opensource.axo.dev/cargo-dist/) builds the binaries. All five crates
(`nix-assay`, `nixq`, `nixdrv`, `nixstore`, `nixfetch`) share one version and one
`CHANGELOG.md` ([Keep a Changelog](https://keepachangelog.com/en/1.1.0/)).

## The normal flow

1. **Merge to `main`** using [Conventional Commits](https://www.conventionalcommits.org/).
   release-plz reads them to pick the bump (`fix:` → patch, `feat:` → minor; while on `0.x`,
   a breaking change `!` → minor) and to write the changelog entry.
2. **Review the release PR.** On every push to `main`, release-plz opens or updates a PR that
   bumps `[workspace.package] version` (and the sibling `version = "…"` requirements) and adds
   the `CHANGELOG.md` section. Edit the changelog wording in that PR if needed; check that
   `cargo semver-checks` in CI agrees with the bump.
3. **Merge the release PR.** release-plz publishes the crates to crates.io in dependency order
   and pushes the tag `v<version>` with a GitHub release.
4. **The tag triggers cargo-dist**, which builds the release binaries for:
   - `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`
   - `x86_64-apple-darwin`, `aarch64-apple-darwin`

   and uploads to the GitHub release: archives with `.sha256` checksums, a shell installer,
   and the legacy `assay-<target>.tar.gz` assets (a directory `assay-<target>/` holding the
   `assay` binary) that Nix consumers fetch.
5. **Bump Nix consumers** (see below).

## Required secrets

| Secret | Used by | Why |
|--------|---------|-----|
| `CARGO_REGISTRY_TOKEN` | release-plz | crates.io token with publish rights on all five crates |
| `RELEASE_PLZ_TOKEN` | release-plz | a PAT or GitHub App token with `contents: write` and `pull-requests: write`. Tags pushed with the default `GITHUB_TOKEN` do not trigger other workflows, so without it the cargo-dist release never runs |

## Verifying a release

```bash
v=0.3.0   # the new version

# crates.io: every crate at the new version
for c in nix-assay nixq nixdrv nixstore nixfetch; do cargo search "$c" --limit 1; done
cargo install nix-assay --locked --version "$v" && assay --version

# GitHub release assets and checksums
gh release view "v$v" --repo Industrial/assay
gh release download "v$v" --repo Industrial/assay --pattern '*.sha256' --pattern 'assay-x86_64-unknown-linux-gnu.tar.gz*'
sha256sum -c assay-x86_64-unknown-linux-gnu.tar.gz.sha256

# Installer and flake
curl --proto '=https' --tlsv1.2 -LsSf "https://github.com/Industrial/assay/releases/download/v$v/nix-assay-installer.sh" | sh
nix run "github:Industrial/assay/v$v" -- --version
```

docs.rs builds each crate on publish; check `https://docs.rs/nix-assay/<version>`.

## Nix consumers

The `program-assay.nix` module in
[`Industrial/cursor-setup`](https://github.com/Industrial/cursor-setup) pins a version and the
hash of the legacy `assay-x86_64-unknown-linux-gnu.tar.gz` asset (`releaseHash`); with
`releaseHash = null` it falls back to `nix run github:Industrial/assay/v<version>`. After a
release, bump it:

```bash
nix store prefetch-file --json \
  "https://github.com/Industrial/assay/releases/download/v$v/assay-x86_64-unknown-linux-gnu.tar.gz" \
  | jq -r .hash
```

and set `version` and `releaseHash` in cursor-setup. Also update the `version` in this repo's
`flake.nix` if release-plz does not manage it.

## Manual or emergency release

Use this when automation is broken or a fix must ship without waiting for the release PR.

1. On an up-to-date `main`, bump `[workspace.package] version` and the sibling dependency
   versions in `crates/*/Cargo.toml`, add the `CHANGELOG.md` section, and commit
   `chore(release): <version>` through a PR (CI must be green).
2. Publish in dependency order (`nixstore` → `nixq` → `nixdrv` → `nixfetch` → `nix-assay`),
   after `cargo login` with a crates.io token. `release-plz release` run locally does the
   same (it reads the token from the `CARGO_REGISTRY_TOKEN` environment variable).

   ```bash
   cargo publish -p nixstore   # repeat per crate, in the order above
   ```

3. Tag and push; the tag runs cargo-dist as usual:

   ```bash
   git tag -a "v$v" -m "v$v" && git push origin "v$v"
   ```

4. If the dist workflow failed for a tag, fix it and re-run it from the Actions tab (or delete
   the draft release and re-push the tag). Never re-publish a crate version: crates.io versions
   are immutable — yank (`cargo yank --version <v> <crate>`) and release a new patch instead.
