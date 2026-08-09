{
  lib,
  config,
  pkgs,
  ...
}: let
  cfg = config.cursor.features.env-bindgen;
  soft = lib.mkOverride 1500;
  gccForBindgen = pkgs.stdenv.cc.cc;
  gccBindgenVer = gccForBindgen.version;
  bindgenExtraClangArgs = "-isystem ${gccForBindgen}/include/c++/${gccBindgenVer} -isystem ${gccForBindgen}/include/c++/${gccBindgenVer}/${pkgs.stdenv.hostPlatform.config} -isystem ${gccForBindgen}/lib/gcc/${pkgs.stdenv.hostPlatform.config}/${gccBindgenVer}/include -isystem ${pkgs.glibc.dev}/include";
in {
  options.cursor.features.env-bindgen.enable = lib.mkEnableOption "LIBCLANG_PATH + BINDGEN_EXTRA_CLANG_ARGS for -sys crates";

  config = lib.mkIf cfg.enable {
    env = {
      BINDGEN_EXTRA_CLANG_ARGS = soft bindgenExtraClangArgs;
      LIBCLANG_PATH = soft "${pkgs.llvmPackages.libclang.lib}/lib";
    };
  };
}
