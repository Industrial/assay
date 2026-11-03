# Bin-contract fixture: a case value that throws while the suite loads.
let
  assay = import ../../../../nix/assay/default.nix;
in
  assay.suite "load-error" {
    passes = assay.eq (1 + 1) 2;
    errors = assay.eq (builtins.throw "kaboom") 1;
  }
