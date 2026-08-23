{
  assertSupportedSystem = system:
    if system == "x86_64-linux"
    then system
    else builtins.throw "unsupported system";
  assertNonEmptyString = s:
    if s == ""
    then builtins.throw "empty string"
    else s;
  assertMatches = pattern: input:
    if builtins.match pattern input == null
    then builtins.throw "pattern mismatch"
    else input;
  assertIntInRange = low: high: input:
    if input < low || input > high
    then builtins.throw "out of range"
    else input;
  assertListOfType = typ: list: let
    check = x:
      if typ == "string"
      then
        if !x.isString
        then builtins.throw "not a string"
        else x
      else x;
  in
    list.map check list;
  assertHasAttrs = attrs: set: let
    hasAll = builtins.all (attr: builtins.hasAttr attr set) attrs;
  in
    if !hasAll
    then builtins.throw "missing attrs"
    else set;
  assertElem = elem: list: let
    contains = builtins.any (x: x == elem) list;
  in
    if !contains
    then builtins.throw "element not found"
    else elem;
}
