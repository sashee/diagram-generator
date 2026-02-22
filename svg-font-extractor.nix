{pkgs ? import (fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.11") { config = {}; overlays = []; }}:
pkgs.rustPlatform.buildRustPackage {
  pname = "svg-font-extractor";
  version = "0.1.0";

  nativeBuildInputs = [pkgs.makeWrapper];

  src = ./svg-font-extractor;
  cargoLock = {
    lockFile = ./svg-font-extractor/Cargo.lock;
  };

  postFixup = ''
    wrapProgram $out/bin/svg-font-extractor \
      --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.fontconfig]}
  '';
}
