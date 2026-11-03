//! Pins the `nixq` binary's exit codes and stdout across the id_effect 0.13 port.

use bin_contract_support::bin_case;

const BIN: &str = env!("CARGO_BIN_EXE_nixq");
const STORE: &str = "../nixstore/fixtures/minimal";
const A: &str = "/nix/store/aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa-a";

bin_case!(get_found, ["get", "a.b"], stdin: Some(r#"{"a":{"b":1}}"#), exit: 0, golden: true);
bin_case!(get_missing, ["get", "a.zz"], stdin: Some(r#"{"a":{"b":1}}"#), exit: 1, golden: true);
bin_case!(has_attrs_true, ["has-attrs", "a", "b"], stdin: Some(r#"{"a":1,"b":2}"#), exit: 0, golden: true);
bin_case!(has_attrs_false, ["has-attrs", "a", "b"], stdin: Some(r#"{"a":1}"#), exit: 1, golden: true);
bin_case!(normalize, ["normalize"], stdin: Some(r#"{"b":2,"a":[3,1]}"#), exit: 0, golden: true);
bin_case!(normalize_bad_json, ["normalize"], stdin: Some("not json"), exit: 1, golden: true);
bin_case!(path_info_text, ["path-info", "--store", STORE, A], stdin: None, exit: 0, golden: true);
bin_case!(path_info_json, ["path-info", "--store", STORE, "--json", A], stdin: None, exit: 0, golden: true);
bin_case!(path_info_unknown, ["path-info", "--store", STORE, "/nix/store/missing"], stdin: None, exit: 2, golden: true);
bin_case!(usage_error, ["bogus"], stdin: None, exit: 2, golden: false);
