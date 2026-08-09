# Assay Nix DSL

Author-facing claim algebra for [Assay](https://github.com/Industrial/assay).
Tests are **data**: plain Nix attrsets the Rust runner evaluates in isolation.

## Import

From this repository:

```nix
let assay = import ./nix/assay/default.nix; # path relative to your suite
in
  assay.suite "my-suite" { /* cases */ }
```

## Suites and cases

```nix
assay.suite "arithmetic" {
  add = assay.eq (builtins.add 1 1) 2;
  divideByZero = assay.throws "1 / 0" ".*";
}
```

## Claim constructors

| Constructor | Meaning |
|-------------|---------|
| `assay.eq actual expected` | Equality after JSON normalize |
| `assay.throws expr pattern` | Eval fails; optional message match |
| `assay.subset actual expected` | Sub-attrset |
| `assay.hasAttrs actual attrs` | Keys present |
| `assay.snapshot name expr` | Golden compare |
| `assay.module args` | Module eval claims |
| `assay.drv args` | Derivation projection |
| `assay.forces expr paths` | Force-set restriction |

See [docs/authoring.md](../../docs/authoring.md) and in-tree
[`tests/smoke.assay.nix`](tests/smoke.assay.nix).
