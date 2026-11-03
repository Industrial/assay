//! Pins the `nixdrv` binary's exit codes and stdout across the id_effect 0.13 port.

use bin_contract_support::bin_case;

const BIN: &str = env!("CARGO_BIN_EXE_nixdrv");
const DRV: &str = include_str!("../fixtures/simple.drv");

bin_case!(parse_aterm_file, ["parse", "-f", "fixtures/simple.drv"], stdin: None, exit: 0, golden: true);
bin_case!(parse_json_file, ["parse", "-f", "fixtures/simple.json"], stdin: None, exit: 0, golden: true);
bin_case!(parse_stdin, ["parse"], stdin: Some(DRV), exit: 0, golden: true);
bin_case!(project_fields, ["project", "-f", "fixtures/simple.drv", "--fields", "outputs,system"], stdin: None, exit: 0, golden: true);
bin_case!(parse_missing_file, ["parse", "-f", "fixtures/missing.drv"], stdin: None, exit: 1, golden: true);
bin_case!(parse_garbage, ["parse"], stdin: Some("garbage\n"), exit: 1, golden: true);
bin_case!(store_path_parse, ["store-path", "parse", "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-a"], stdin: None, exit: 0, golden: true);
bin_case!(store_path_parse_invalid, ["store-path", "parse", "not-a-path"], stdin: None, exit: 1, golden: true);
bin_case!(
  store_path_make_fixed,
  ["store-path", "make-fixed", "--name", "foo", "--method", "flat", "--hash-algo", "sha256",
   "--digest-hex", "0000000000000000000000000000000000000000000000000000000000000000"],
  stdin: None, exit: 0, golden: true
);
bin_case!(usage_error, ["bogus"], stdin: None, exit: 2, golden: false);
