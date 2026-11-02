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
  ];

  src = pkgs.nix-gitignore.gitignoreSourcePure [ ./.gitignore ] ./src/svg-font-inliner;
  cargoLock = {
    lockFile = ./src/svg-font-inliner/Cargo.lock;
  };

  preCheck = ''
    export FONTCONFIG_FILE=${fontconfig}
    export HB_SUBSET_BIN=${pkgs.harfbuzz.dev}/bin/hb-subset
    export PATH=${pkgs.lib.makeBinPath [pkgs.fontconfig]}:$PATH
  '';

  postFixup = ''
    mv $out/bin/svg-font-inliner $out/bin/.svg-font-inliner-real

    makeWrapper ${sandbox_run}/bin/sandbox-run $out/bin/svg-font-inliner \
      --add-flags "--keep-env SVG_FONT_EMBED_DEBUG --keep-env FONTCONFIG_FILE --keep-env HB_SUBSET_BIN" \
      --add-flags -- \
      --add-flags $out/bin/.svg-font-inliner-real \
      --set SVG_FONT_EMBED_DEBUG ${if debug then "1" else "0"} \
      --set FONTCONFIG_FILE ${fontconfig} \
      --set HB_SUBSET_BIN ${pkgs.harfbuzz.dev}/bin/hb-subset \
      --prefix PATH : ${pkgs.lib.makeBinPath [pkgs.fontconfig]}
  '';
}
