{
  pkgs ? import (fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.11") { config = {}; overlays = []; },
  sandbox_run ? import ./sandbox-run.nix { inherit pkgs; },
}:
pkgs.rustPlatform.buildRustPackage {
  pname = "svg-to-png";
  version = "0.1.0";

  nativeBuildInputs = [ pkgs.makeWrapper ];

  src = ./src;
  cargoRoot = "svg-to-png";
  buildAndTestSubdir = "svg-to-png";
  cargoLock = {
    lockFile = ./src/svg-to-png/Cargo.lock;
  };

  postFixup = ''
    mv $out/bin/svg-to-png $out/bin/.svg-to-png-real
    makeWrapper ${sandbox_run}/bin/sandbox-run $out/bin/svg-to-png \
      --add-flags -- \
      --add-flags $out/bin/.svg-to-png-real
  '';
}
