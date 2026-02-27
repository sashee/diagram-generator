{
  pkgs ? import (fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.11") { config = {}; overlays = []; },
  sandbox_run ? import ./sandbox-run.nix { inherit pkgs; },
}:
pkgs.rustPlatform.buildRustPackage {
  pname = "svg-font-inliner";
  version = "0.1.0";

  nativeBuildInputs = [
    pkgs.makeWrapper
    pkgs.fontconfig
    pkgs.python3Packages.fonttools
  ];

  src = ./src/svg-font-inliner;
  cargoLock = {
    lockFile = ./src/svg-font-inliner/Cargo.lock;
  };

  preCheck = ''
    export PYFTSUBSET_BIN=${pkgs.python3Packages.fonttools}/bin/pyftsubset
    export PATH=${pkgs.lib.makeBinPath [pkgs.fontconfig pkgs.python3Packages.fonttools]}:$PATH
  '';

  postFixup = ''
    mv $out/bin/svg-font-inliner $out/bin/.svg-font-inliner-real

    makeWrapper ${sandbox_run}/bin/sandbox-run $out/bin/svg-font-inliner \
      --add-flags -- \
      --add-flags $out/bin/.svg-font-inliner-real \
      --set PYFTSUBSET_BIN ${pkgs.python3Packages.fonttools}/bin/pyftsubset \
      --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.fontconfig pkgs.python3Packages.fonttools]}
  '';
}
