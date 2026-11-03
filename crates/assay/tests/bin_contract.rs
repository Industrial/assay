//! Pins the `assay` binary's exit codes and stdout across the id_effect 0.13 port.
//! The smoke/dogfood goldens double as the verdict baseline for the in-repo DSL suites.

use bin_contract_support::{self as support, bin_case};

const BIN: &str = env!("CARGO_BIN_EXE_assay");

/// Like `bin_case!`, but skipped when `nix` is not on PATH.
macro_rules! nix_case {
  ($name:ident, [$($arg:expr),* $(,)?], exit: $exit:expr, golden: $golden:expr) => {
    #[test]
    fn $name() {
      if !support::has_program("nix") {
        eprintln!("skipping {}: nix not on PATH", stringify!($name));
        return;
      }
      support::check(
        BIN,
        env!("CARGO_MANIFEST_DIR"),
        &support::Case { name: stringify!($name), args: &[$($arg),*], stdin: None, exit: $exit, golden: $golden },
      );
    }
  };
}

const SMOKE: &str = "../../nix/assay/tests/smoke.assay.nix";
const DOGFOOD: &str = "../../nix/assay/tests/dogfood.assay.nix";
const MIXED: &str = "tests/fixtures/mixed.assay.nix";

nix_case!(run_smoke_text, ["run", SMOKE], exit: 0, golden: true);
nix_case!(run_smoke_json, ["run", SMOKE, "--json"], exit: 0, golden: true);
nix_case!(run_smoke_no_batch, ["run", SMOKE, "--no-batch", "--json"], exit: 0, golden: true);
nix_case!(run_dogfood_json, ["run", DOGFOOD, "--json"], exit: 0, golden: true);
nix_case!(run_mixed_text, ["run", MIXED], exit: 1, golden: true);
nix_case!(run_mixed_json, ["run", MIXED, "--json"], exit: 1, golden: true);
// Message text comes from nix itself and varies by nix version: pin the exit only.
nix_case!(run_load_error, ["run", "tests/fixtures/load-error.assay.nix"], exit: 1, golden: false);

bin_case!(run_missing_path, ["run", "/nonexistent"], stdin: None, exit: 1, golden: true);
// A malformed JSON suite is a load error (every decode issue goes to stderr).
bin_case!(run_malformed_suite, ["run", "tests/fixtures/malformed.assay.json"], stdin: None, exit: 1, golden: false);
bin_case!(discover_dsl_tests, ["discover", "../../nix/assay/tests"], stdin: None, exit: 0, golden: true);
bin_case!(laws_seed_1, ["laws", "--seed", "1"], stdin: None, exit: 0, golden: true);
bin_case!(laws_seed_1_json, ["laws", "--seed", "1", "--json"], stdin: None, exit: 0, golden: true);
bin_case!(version, ["--version"], stdin: None, exit: 0, golden: false);
bin_case!(usage_error, ["bogus"], stdin: None, exit: 2, golden: false);
