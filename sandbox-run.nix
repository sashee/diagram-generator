{
  debug,
  pkgs,
}:
pkgs.rustPlatform.buildRustPackage {
  pname = "sandbox-run";
  version = "0.1.0";
  buildType = "release";

  src = pkgs.nix-gitignore.gitignoreSourcePure [ ./.gitignore ] ./src/sandbox-run;
  cargoLock = {
    lockFile = ./src/sandbox-run/Cargo.lock;
  };

  nativeBuildInputs = [ pkgs.pkg-config pkgs.coreutils ];
  buildInputs = [ pkgs.libseccomp ];

  NIX_STORE_DIR = builtins.storeDir;
}
