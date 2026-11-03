//! Pins the `nixstore` binary's exit codes and stdout across the id_effect 0.13 port.

use bin_contract_support::bin_case;

const BIN: &str = env!("CARGO_BIN_EXE_nixstore");
const STORE: &str = "fixtures/minimal";
const A: &str = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-a";
const C: &str = "/nix/store/cccccccccccccccccccccccccccccccc-c";

bin_case!(path_info_text, ["path-info", "--store", STORE, A], stdin: None, exit: 0, golden: true);
bin_case!(path_info_json, ["path-info", "--store", STORE, "--json", A], stdin: None, exit: 0, golden: true);
bin_case!(path_info_sizes_human, ["path-info", "--store", STORE, "-s", "-S", "-h", C], stdin: None, exit: 0, golden: true);
bin_case!(path_info_referrers_json, ["path-info", "--store", STORE, "--referrers", "--json", C], stdin: None, exit: 0, golden: true);
bin_case!(path_info_unknown, ["path-info", "--store", STORE, "/nix/store/missing"], stdin: None, exit: 1, golden: true);
bin_case!(path_info_db_missing, ["path-info", "--store", "/nonexistent", A], stdin: None, exit: 1, golden: true);
bin_case!(usage_error, ["bogus"], stdin: None, exit: 2, golden: false);
