let
  assay = import ../../nix/assay/default.nix;
in
  assay.suite "quickstart" {
    onePlusOne = assay.eq (1 + 1) 2;
    hello = assay.eq ("hel" + "lo") "hello";
  }
