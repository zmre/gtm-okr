{
  description = "gtm-okr flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    # rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    # naersk.url = "github:nix-community/naersk";
  };

  outputs = { self, nixpkgs, flake-utils, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let pkgs = nixpkgs.legacyPackages.${system};
      in rec {
        # `nix build`
        packages = {
          gtm-okr = pkgs.rustPlatform.buildRustPackage {
            pname = "gtm-okr";
            version = "0.1.0";
            src = ./.;
            # src = pkgs.nix-gitignore.gitignoreSource [
            #   ./.gitignore
            #   "flake.nix"
            #   "result"
            # ] ./.;
            cargoLock.lockFile = ./Cargo.lock;
            nativeBuildInputs = [ pkgs.pkg-config ];
            buildInputs = [ pkgs.openssl ]
              ++ pkgs.lib.optionals pkgs.stdenv.isDarwin
              [ pkgs.darwin.apple_sdk.frameworks.Security ];
          };
        };
        defaultPackage = packages.gtm-okr;

        # nix develop
        devShell = pkgs.mkShell {
          buildInputs = with pkgs;
            [ openssl pkg-config ] ++ pkgs.lib.optionals pkgs.stdenv.isDarwin
            [ pkgs.darwin.apple_sdk.frameworks.Security ];
        };
      });
}
