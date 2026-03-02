{
  debug,
  fontconfig,
  pkgs,
}:
let
	sandbox_run = import ./sandbox-run.nix { inherit debug pkgs; };
in
pkgs.rustPlatform.buildRustPackage {
  pname = "svg-font-inliner";
  version = "0.1.0";
  buildType = "release";

  nativeBuildInputs = [
    pkgs.makeWrapper
    pkgs.fontconfig
    pkgs.python3Packages.fonttools
  ];

  src = pkgs.nix-gitignore.gitignoreSourcePure [ ./.gitignore ] ./src/svg-font-inliner;
  cargoLock = {
    lockFile = ./src/svg-font-inliner/Cargo.lock;
  };

  preCheck = ''
    export FONTCONFIG_FILE=${fontconfig}
    export PYFTSUBSET_BIN=${pkgs.python3Packages.fonttools}/bin/pyftsubset
    export PATH=${pkgs.lib.makeBinPath [pkgs.fontconfig pkgs.python3Packages.fonttools]}:$PATH
  '';

  postFixup = ''
    mv $out/bin/svg-font-inliner $out/bin/.svg-font-inliner-real

    makeWrapper ${sandbox_run}/bin/sandbox-run $out/bin/svg-font-inliner \
      --add-flags -- \
      --add-flags $out/bin/.svg-font-inliner-real \
      --set SVG_FONT_EMBED_DEBUG ${if debug then "1" else "0"} \
      --set FONTCONFIG_FILE ${fontconfig} \
      --set PYFTSUBSET_BIN ${pkgs.python3Packages.fonttools}/bin/pyftsubset \
      --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.fontconfig pkgs.python3Packages.fonttools]}
  '';
}
