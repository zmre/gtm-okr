{
  description = "gtm-okr flake";

  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-unstable";
    rust-overlay.url = "github:oxalica/rust-overlay";
    flake-utils.url = "github:numtide/flake-utils";
    naersk.url = "github:nix-community/naersk";
  };

  outputs = { self, nixpkgs, rust-overlay, flake-utils, naersk, ... }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ (import rust-overlay) ];
        pkgs = import nixpkgs { inherit system overlays; };
        rusttoolchain =
          pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        naersk-lib = naersk.lib."${system}";
      in rec {
        # `nix build`
        packages.gtm-okr = naersk-lib.buildPackage {
          pname = "gtm-okr";
          root = ./.;
          buildInputs = with pkgs; [ openssl pkg-config rusttoolchain ];
        };
        defaultPackage = packages.gtm-okr;

        # nix develop
        devShell = pkgs.mkShell {
          buildInputs = with pkgs; [ openssl pkg-config rusttoolchain ];
        };
      });
}
