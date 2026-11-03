# Security policy

## Supported versions

Only the latest minor release line receives fixes. All five crates (`nix-assay`, `nixq`,
`nixdrv`, `nixstore`, `nixfetch`) share one version, so this applies to all of them.

| Version | Supported |
|---------|-----------|
| latest `0.x` minor (currently 0.3) | yes |
| older | no — upgrade |

## Reporting a vulnerability

Please **do not open a public issue**. Report privately through GitHub Security Advisories:
go to the repository's **Security** tab and click
[**Report a vulnerability**](https://github.com/Industrial/assay/security/advisories/new).

Include the affected version (`assay --version`), your platform and Nix version, and a minimal
reproduction (a suite file, command line or input) if you have one.

## What to expect

- Acknowledgement within 7 days.
- An assessment (accepted or declined, with reasoning) within 14 days.
- For accepted reports: a fix in a patch release on the supported line, a published advisory
  (with a CVE where warranted), and credit to the reporter unless you prefer otherwise.

This is a small, volunteer-maintained project; these are targets, not guarantees.

## Scope

In scope: the crates and binaries in this repository and the release artifacts built from
it. Of particular interest: `nixfetch` hash verification bypasses, path traversal or unsafe
temp-file handling, and argument injection into the `nix` / `git` subprocesses the tools run.

Out of scope: vulnerabilities in Nix itself or in the Nix expressions a suite evaluates —
`assay run` evaluates the suite you give it with your privileges, like `nix eval` does.

## Supply chain

Dependencies are checked by `cargo deny` (advisories, licenses, sources) in CI, a daily
`cargo audit` workflow, and weekly Dependabot updates for Cargo and GitHub Actions. Actions
are pinned by commit SHA.
