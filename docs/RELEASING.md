# Releasing

A release is a `v<version>` tag on `main`. Pushing the tag runs
[`.github/workflows/release.yml`](../.github/workflows/release.yml), which builds the binaries,
creates the GitHub release and publishes the crates. All five crates (`nix-assay`, `nixq`,
`nixdrv`, `nixstore`, `nixfetch`) share one version and one `CHANGELOG.md`
([Keep a Changelog](https://keepachangelog.com/en/1.1.0/)).

## Prepare the release PR

1. Pick the version from the `## [Unreleased]` entries ([SemVer](https://semver.org/); while on
   `0.x`, a breaking change bumps the minor). `cargo semver-checks` in CI is a second opinion.
2. On a branch off an up-to-date `main`:
   - set `[workspace.package] version` in `Cargo.toml`, and the `version` of the internal
     crates in `[workspace.dependencies]`;
   - set `version` in `flake.nix`;
   - rename `## [Unreleased]` in `CHANGELOG.md` to `## [<version>] - <YYYY-MM-DD>`, start a new
     empty `## [Unreleased]` above it, and add the compare link at the bottom;
   - `cargo update --workspace` so the lockfile records the new versions.
3. Commit `chore(release): <version>`, open a PR, and merge it once `ci-pass` is green.

## Tag

```bash
git switch main && git pull --ff-only
v=0.3.1   # the new version
git tag -a "v$v" -m "v$v"
git push origin "v$v"
```

The workflow then runs, in order:

| Job | What it does |
|-----|--------------|
| `verify tag` | fails unless the tag equals `[workspace.package] version` and the `flake.nix` version, and `CHANGELOG.md` has a `## [<version>]` section; `cargo package --workspace --locked` |
| `build` | `assay-<target>.tar.gz` (holding `assay-<target>/assay`) plus a `.sha256` for `x86_64`/`aarch64` `unknown-linux-gnu` and `x86_64`/`aarch64` `apple-darwin` |
| `GitHub release` | runs only when every build passed: checks the checksums, writes `SHA256SUMS`, attests build provenance, creates the release with the CHANGELOG section as notes |
| `crates.io` | `cargo publish --workspace --locked`, in dependency order, from the `crates-io` environment |

## One-time setup

- Secret `CARGO_REGISTRY_TOKEN` in the `crates-io` environment: a crates.io token with
  publish rights on all five crates. Add required reviewers to the environment to make
  publishing a manual approval.
- Branch protection on `main` requiring the `ci-pass` check.

## Verify a release

```bash
# GitHub release assets, checksums and provenance
gh release download "v$v" --repo Industrial/assay --pattern 'assay-x86_64-unknown-linux-gnu.tar.gz*'
sha256sum -c assay-x86_64-unknown-linux-gnu.tar.gz.sha256
gh attestation verify assay-x86_64-unknown-linux-gnu.tar.gz --repo Industrial/assay

# crates.io and the flake
cargo install nix-assay --locked --version "$v" && assay --version
nix run "github:Industrial/assay/v$v" -- --version
```

docs.rs builds each crate on publish; check `https://docs.rs/nix-assay/<version>`.

## Nix consumers

The `program-assay.nix` module in
[`Industrial/cursor-setup`](https://github.com/Industrial/cursor-setup) pins a version and the
hash of `assay-x86_64-unknown-linux-gnu.tar.gz` (`releaseHash`); with `releaseHash = null` it
falls back to `nix run github:Industrial/assay/v<version>`. After a release, bump both:

```bash
nix store prefetch-file --json \
  "https://github.com/Industrial/assay/releases/download/v$v/assay-x86_64-unknown-linux-gnu.tar.gz" \
  | jq -r .hash
```

## When something fails

- **`verify tag` failed**: delete the tag (`git push --delete origin "v$v"; git tag -d "v$v"`),
  fix `main`, and tag again. Nothing was published.
- **A build failed**: nothing was released or published. Fix, delete and re-push the tag, or
  re-run the failed jobs from the Actions tab if the failure was transient.
- **`crates.io` failed part-way**: crates.io versions are immutable, so re-running stops at the
  first crate already published. Publish the rest by hand, in order, after `cargo login`:

  ```bash
  cargo publish --locked -p nixfetch   # each crate not yet on crates.io:
                                       # nixstore → nixq → nixdrv → nixfetch → nix-assay
  ```

- **A bad version shipped**: never re-publish it. Yank it
  (`cargo yank --version <v> <crate>`) and release a new patch.
