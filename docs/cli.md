# CLI reference

```text
assay <COMMAND>

Commands:
  run       Run suites under PATH
  discover  List discoverable suites
  laws      Emit/check law catalogue helpers
```

## `assay run`

```text
assay run [OPTIONS] <PATH>

Options:
      --json                  JSON report
      --format <FORMAT>       human | json | tap
      --update-snapshots      Refresh goldens
      --case-timeout-ms <MS>  Per-case timeout
      --retry-flaky-eval      Retry flaky eval failures
      --no-batch              One nix process per claim
```

`PATH` may be a single `*.assay.nix` file or a directory (recursive discovery).

## `assay discover`

```text
assay discover <PATH>
```

Prints suite paths and kinds (e.g. `AssayNix`).

## Exit codes

With the default `cli-exit` feature, non-zero exits follow `id_effect_cli`
cause mapping (failures / infra errors).
