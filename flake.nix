{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";
    crane.url = "github:ipetkov/crane";
    crane.inputs.nixpkgs.follows = "nixpkgs";
    flake-utils.url = "github:numtide/flake-utils";
  };

  outputs =
    {
      self,
      nixpkgs,
      crane,
      flake-utils,
      ...
    }:
    flake-utils.lib.eachDefaultSystem (
      system:
      let
        pkgs = nixpkgs.legacyPackages.${system};
        craneLib = crane.mkLib pkgs;

        buildDeps = (
          with pkgs;
          [
            pkg-config
            makeWrapper
            clang
            mold
          ]
        );

        runtimeDeps = (
          with pkgs;
          [
            pkg-config
            libxkbcommon
            alsa-lib
            udev
            vulkan-loader
            wayland
          ]
          ++ (with xorg; [
            libXcursor
            libXrandr
            libXi
            libX11
          ])
        );
      in
      {
        devShells.default = craneLib.devShell {
          RUST_SRC_PATH = "${pkgs.rustPlatform.rustLibSrc}";
          LD_LIBRARY_PATH = "${pkgs.lib.makeLibraryPath runtimeDeps}";

          packages =
            with pkgs;
            [
              rustfmt
              rust-analyzer
              rustPackages.clippy
              rustup
            ]
            ++ runtimeDeps;
        };
      }
    );
}
