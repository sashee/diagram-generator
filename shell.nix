let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-26.05";
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
  ];

  shellHook = ''
    export HB_SUBSET_BIN="${pkgs.harfbuzz.dev}/bin/hb-subset"
    export NIX_STORE_DIR="${builtins.storeDir}"
    export LD_LIBRARY_PATH="${pkgs.lib.makeLibraryPath [pkgs.libseccomp]}${pkgs.lib.optionalString (builtins.getEnv "LD_LIBRARY_PATH" != "") ":$LD_LIBRARY_PATH"}"
    echo "HB_SUBSET_BIN=$HB_SUBSET_BIN"
    echo "NIX_STORE_DIR=$NIX_STORE_DIR"
    echo "LD_LIBRARY_PATH=$LD_LIBRARY_PATH"
  '';
}
