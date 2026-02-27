{pkgs ? import (fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.11") { config = {}; overlays = []; }}:
pkgs.rustPlatform.buildRustPackage {
  pname = "sandbox-run";
  version = "0.1.0";

  src = ./src;
  cargoRoot = "sandbox-run";
  buildAndTestSubdir = "sandbox-run";
  cargoLock = {
    lockFile = ./src/sandbox-run/Cargo.lock;
  };

  nativeBuildInputs = [ pkgs.pkg-config ];
  buildInputs = [ pkgs.libseccomp ];

  NIX_STORE_DIR = builtins.storeDir;
}
