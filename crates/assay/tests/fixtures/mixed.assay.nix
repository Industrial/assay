# Bin-contract fixture: a pass, an eq failure, and failing/erroring throws cases.
let
  assay = import ../../../../nix/assay/default.nix;
in
  assay.suite "mixed" {
    passes = assay.eq (1 + 1) 2;
    eqFails = assay.eq 1 2;
    throwsWrongMessage = assay.throws "builtins.throw \"a\"" "b";
    throwsButDoesNot = assay.throws "1 + 1" null;
    badExpr = assay.throws "this is not nix" null;
  }
