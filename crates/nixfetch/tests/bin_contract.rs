//! Pins the `nixfetch` binary's exit codes and stdout across the id_effect 0.13 port.
//! fetch-url / fetch-git need network or git and stay covered by unit tests.

#[macro_use]
#[path = "../../../tests/support/bin_contract.rs"]
mod support;

const BIN: &str = env!("CARGO_BIN_EXE_nixfetch");
const ZERO_SRI: &str = "sha256-AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";

bin_case!(hash_flat, ["hash", "fixtures/flat/hello", "--flat"], stdin: None, exit: 0, golden: true);
bin_case!(hash_recursive, ["hash", "fixtures/nar/tree", "--recursive"], stdin: None, exit: 0, golden: true);
bin_case!(hash_missing, ["hash", "fixtures/flat/missing", "--flat"], stdin: None, exit: 2, golden: true);
bin_case!(verify_mismatch, ["verify", "fixtures/flat/hello", "--flat", "--expected", ZERO_SRI], stdin: None, exit: 1, golden: true);
bin_case!(verify_bad_hash, ["verify", "fixtures/flat/hello", "--flat", "--expected", "garbage"], stdin: None, exit: 2, golden: true);
bin_case!(store_path, ["store-path", "--name", "hello", "--hash", ZERO_SRI], stdin: None, exit: 0, golden: true);
bin_case!(usage_error, ["bogus"], stdin: None, exit: 2, golden: false);
