let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.11";
  pkgs = import nixpkgs { config = {}; overlays = []; };
in
pkgs.mkShell {
  nativeBuildInputs = [
    pkgs.rustc
    pkgs.cargo
    pkgs.coreutils
    pkgs.pkg-config
    pkgs.libseccomp
    pkgs.cargo-outdated
    pkgs.fontconfig
    pkgs.python3Packages.fonttools
  ];

  shellHook = ''
    export PYFTSUBSET_BIN="${pkgs.python3Packages.fonttools}/bin/pyftsubset"
    export NIX_STORE_DIR="${builtins.storeDir}"
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [pkgs.libseccomp]}${pkgs.lib.optionalString (builtins.getEnv "LD_LIBRARY_PATH" != "") ":$LD_LIBRARY_PATH"}"
    echo "PYFTSUBSET_BIN=$PYFTSUBSET_BIN"
    echo "NIX_STORE_DIR=$NIX_STORE_DIR"
    echo "LD_LIBRARY_PATH=$LD_LIBRARY_PATH"
  '';
}
