{pkgs ? import (fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.11") { config = {}; overlays = []; }}:
pkgs.rustPlatform.buildRustPackage {
  pname = "svg-to-png";
  version = "0.1.0";

  src = ./src;
  cargoRoot = "svg-to-png";
  buildAndTestSubdir = "svg-to-png";
  cargoLock = {
    lockFile = ./src/svg-to-png/Cargo.lock;
  };
}
