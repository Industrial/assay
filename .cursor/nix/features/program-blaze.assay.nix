let
  assay = import ../lib/assay.nix;
  h = import ../lib/feature-eval.nix {};
  off = h.feature ./program-blaze.nix {};
  on = h.feature ./program-blaze.nix {cursor.features.program-blaze.enable = true;};
  stopOnExit = h.feature ./program-blaze.nix {
    cursor.features.program-blaze = {
      enable = true;
      daemonStopOnExit = true;
    };
  };
  has = name: names: builtins.elem name names;
in
  assay.suite "program-blaze" {
    disabled = assay.eq off.packages [];
    disabledNoScripts = assay.eq off.scripts [];
    disabledNoEnterShell = assay.eq off.enterShell "";

    hasBinary = assay.eq (has "blaze" on.packages) true;
    onlyBlaze = assay.eq (builtins.length on.packages) 1;

    # blaze is its own toolchain: the feature must not drag cargo or rustc in,
    # and must not touch languages.rust (rust-analyzer still needs that).
    leavesRustAlone = assay.eq on.languages.rust.enable false;
    setsNoEnv = assay.eq on.env {};

    doctorScript = assay.eq (has "blaze-doctor" on.scripts) true;
    doctorReportsVersion = assay.eq (builtins.match ".*blaze --version.*" on.scriptExec.blaze-doctor != null) true;

    # The daemon is shared across checkouts, so killing it on shell exit is
    # opt-in, never the default.
    keepsDaemonByDefault = assay.eq on.enterShell "";
    stopOnExitTraps = assay.eq (builtins.match ".*blaze stop.*" stopOnExit.enterShell != null) true;
  }
