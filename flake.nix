{
  description = "Assay — Nix unit testing (claim algebra + isolated eval runner)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
  };

  outputs = {
    self,
    nixpkgs,
  }: let
    systems = [
      "x86_64-linux"
      "aarch64-linux"
      "x86_64-darwin"
      "aarch64-darwin"
    ];
    forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f system);
  in {
    packages = forAllSystems (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      mkCrate = {
        pname,
        cargoPackage ? pname,
        mainProgram ? pname,
      }:
        pkgs.rustPlatform.buildRustPackage {
          inherit pname;
          version = "0.2.0";
          src = pkgs.lib.cleanSourceWith {
            src = ./.;
            filter = path: type: let
              base = baseNameOf path;
            in
              !(pkgs.lib.hasInfix "/.git/" path)
              && !(pkgs.lib.hasInfix "/target/" path)
              && !(pkgs.lib.hasInfix "/.maestro/" path)
              && base != "result";
          };
          cargoLock.lockFile = ./Cargo.lock;
          cargoBuildFlags = ["-p" cargoPackage];
          cargoTestFlags = ["-p" cargoPackage];
          doCheck = false;
          nativeBuildInputs = [pkgs.pkg-config];
          meta = with pkgs.lib; {
            description = "Assay workspace crate: ${cargoPackage}";
            homepage = "https://github.com/Industrial/assay";
            license = licenses.mit;
            inherit mainProgram;
          };
        };
    in {
      assay = mkCrate {
        pname = "assay";
        cargoPackage = "nix-assay";
        mainProgram = "assay";
      };
      nixq = mkCrate {pname = "nixq";};
      nixdrv = mkCrate {pname = "nixdrv";};
      nixstore = mkCrate {pname = "nixstore";};
      nixfetch = mkCrate {pname = "nixfetch";};
      default = self.packages.${system}.assay;
    });

    apps = forAllSystems (system: {
      assay = {
        type = "app";
        program = "${self.packages.${system}.assay}/bin/assay";
      };
      default = self.apps.${system}.assay;
    });

    checks = forAllSystems (system: {
      assay = self.packages.${system}.assay;
    });

    devShells = forAllSystems (system: let
      pkgs = nixpkgs.legacyPackages.${system};
    in {
      default = pkgs.mkShell {
        packages = [
          pkgs.cargo
          pkgs.rustc
          pkgs.pkg-config
          pkgs.nix
          self.packages.${system}.assay
        ];
      };
    });
  };
}
