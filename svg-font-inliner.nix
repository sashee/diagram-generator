{pkgs ? import (fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.11") { config = {}; overlays = []; }}:
pkgs.rustPlatform.buildRustPackage {
  pname = "svg-font-inliner";
  version = "0.1.0";

  nativeBuildInputs = [pkgs.makeWrapper];

  src = ./src/svg-font-inliner;
  cargoLock = {
    lockFile = ./src/svg-font-inliner/Cargo.lock;
  };

  postFixup = ''
    wrapProgram $out/bin/svg-font-inliner \
      --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.fontconfig]}
  '';
}
