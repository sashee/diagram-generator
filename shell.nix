let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.11";
  pkgs = import nixpkgs { config = {}; overlays = []; };
in
pkgs.mkShell {
  nativeBuildInputs = [
    pkgs.rustc
    pkgs.cargo
    pkgs.cargo-outdated
    pkgs.fontconfig
    pkgs.python3Packages.fonttools
  ];

  shellHook = ''
    export PYFTSUBSET_BIN="${pkgs.python3Packages.fonttools}/bin/pyftsubset"
    export NIX_STORE_DIR="${builtins.storeDir}"
    echo "PYFTSUBSET_BIN=$PYFTSUBSET_BIN"
    echo "NIX_STORE_DIR=$NIX_STORE_DIR"
  '';
}
