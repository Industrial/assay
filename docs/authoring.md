# Authoring suites

Suites are plain Nix attrsets. Import the in-repo DSL:

```nix
let
  assay = import ./nix/assay/default.nix; # adjust relative path
in
  assay.suite "arithmetic" {
    add = assay.eq (builtins.add 1 1) 2;
    boom = assay.throws "builtins.throw \"boom\"" "boom";
  }
```

Save as `*.assay.nix`. Discover and run:

```bash
assay discover .
assay run .
assay run path/to/suite.assay.nix
```

## Claim constructors

| Constructor | Meaning |
|-------------|---------|
| `assay.eq actual expected` | Structural equality (JSON-normalized) |
| `assay.throws expr pattern` | Eval must fail; optional message match |
| `assay.subset actual expected` | `expected` ⊆ `actual` |
| `assay.hasAttrs actual attrs` | Keys present |
| `assay.snapshot name expr` | Golden snapshot |
| `assay.module { … }` | `lib.evalModules` style claims |
| `assay.drv { … }` | Derivation projection |
| `assay.forces expr paths` | Force-set restriction |

Prefer first-class Nix values for `eq` / `subset` / `hasAttrs`. Keep `throws` as
**source strings** (or deferred lambdas) so building the suite does not throw.

Full DSL notes: [`nix/assay/README.md`](../nix/assay/README.md).
