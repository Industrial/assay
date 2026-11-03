## Summary

<!-- What changes and why. Link issues: "Fixes #123". -->

## Checklist

- [ ] PR title and commits follow [Conventional Commits](https://www.conventionalcommits.org/) (`feat:`, `fix:`, `docs:` …; `!` for breaking)
- [ ] User-visible changes have an entry under `## [Unreleased]` in `CHANGELOG.md`
- [ ] Gates pass locally: `moon run :ci-format :check :lint :lint-effect :test :check-docs :deny` (the pre-push hook runs them)
- [ ] `moon run :assay` passes, if the runner or the Nix DSL changed
- [ ] Tests added or updated (see [TESTING.md](../TESTING.md))
- [ ] Docs updated (`docs/`, crate READMEs, rustdoc) for user-visible changes
- [ ] Goldens re-blessed with `BLESS=1` / `ID_EFFECT_BLESS=1`, not hand-edited, and the diff reviewed
- [ ] Exit codes or bin stdout changed? `docs/cli.md` updated and the change called out below

## Notes for reviewers

<!-- Anything non-obvious: trade-offs, follow-ups, contract changes. -->
