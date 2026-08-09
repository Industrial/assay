---
slug: assay-public-release
title: "Assay public package: install, docs, and distribution"
acceptance_criteria:
  - 'A stranger with Nix + rustc can install assay via at least one documented one-liner (nix run / cargo install / GH release binary) and run a smoke suite successfully.'
  - 'Root README.md is the public landing page: what Assay is, install matrix, 5-minute quickstart, link to full docs.'
  - 'Nix DSL lives in-repo under nix/assay/ (vendored from ~/.dotfiles/common/assay) and is imported by the quickstart.'
  - 'Workspace crates resolve without absolute /data/Code/rust/id_effect paths (crates.io and/or git deps).'
  - 'flake.nix exposes packages.assay (and optionally nixq/nixdrv/nixstore/nixfetch) and apps.assay.'
  - 'GitHub Actions builds release binaries on tag v* and attaches linux-gnu (and darwin when feasible) artifacts.'
  - 'github.com/Industrial/assay exists with origin set; tags and Releases documented.'
  - 'docs/ covers install, authoring suites (DSL), CLI reference (run/discover/laws), and comparison vs nix-unit/runTests.'
  - 'cursor-setup gains opt-in cursor.features.program-assay (pinned GH release binary) plus a small dogfood suite path for .cursor/nix tests.'
  - 'moon :assay (or equivalent) runs in-repo DSL smoke/dogfood suites; documented for consumers.'
non_goals:
  - Full id_effect 0.5.0 ecosystem republish if 0.4.0 crates.io pins suffice for v0.1 (prefer pin published versions; bump later).
  - Publishing id_effect_process or enabling process-pool for the public default binary.
  - nixpkgs upstream PR in v0.1 (flake + GH releases first).
  - Windows native support in v0.1.
  - Rewriting the claim algebra / runner architecture.
risk_class: high
mode: heavy
work_type: initiative
dependencies: []
---

# Assay public package: install, docs, and distribution

## Context

Assay is a hybrid Nix unit-testing tool (Nix DSL + Rust runner). Today it only builds on machines with absolute `id_effect` path deps, has no flake/remote/CI, and the authoring DSL lives outside the repo. Goal: make Assay an official Industrial product anyone can install, learn from docs, and optionally enable in `.cursor` devenv shells to unit-test Nix modules.

## Decisions (locked)

| Decision | Choice | Delta if wrong |
|----------|--------|----------------|
| Primary install surfaces | Flake `nix run` + GH release binary + docs for `cargo install` after crates.io | Drop cargo until later |
| crates.io in v0.1 | Yes for assay workspace crates once id_effect pins are crates.io | Git-only install if publish blocked |
| id_effect pin | Prefer crates.io `0.4.0` if API-compatible; else git tag of id_effect; no absolute paths | May need API shim if 0.5-only APIs used |
| DSL home | Vendor into `nix/assay/` in this repo | Keep external = broken for strangers |
| GitHub | `Industrial/assay` | Other name needs README/branding swap |
| .cursor integration | Opt-in `program-assay` + dogfood suites | Skip = no Industrial dogfood |
| Default features public binary | `cli-exit` + `optics`; no `process-pool` | Smaller binary, no unpublished dep |

## Architecture (target)

```text
User suite (*.assay.nix) --import--> nix/assay (DSL)
                              |
                              v
                         assay run <path>   (Rust CLI from flake/GH/crates.io)
                              |
                              v
                         nix eval (on PATH) --> Pass|Fail|…
```

## Success metric

Cold-machine install → `assay run` on documented smoke suite → green exit in ≤15 minutes following README only.
