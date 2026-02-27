{
  debug,
  pkgs,
}:
let
  sandbox_run = import ./sandbox-run.nix { inherit debug pkgs; };
  srcRoot = ./src;
  gitignorePredicate =
    pkgs.nix-gitignore.gitignoreFilter (builtins.readFile ./.gitignore) srcRoot;
  includePrefixes = [
    "svg-to-png"
    "svg-font-inliner"
  ];
  filteredSrc = builtins.path {
    path = srcRoot;
    name = "src-svg-to-png";
    filter = path: type:
      let
        pathStr = toString path;
        rootStr = toString srcRoot;
        relPath =
          if pathStr == rootStr then
            ""
          else
            pkgs.lib.removePrefix "${rootStr}/" pathStr;
        isIncluded =
          relPath == ""
          || pkgs.lib.any (
            prefix:
            relPath == prefix || pkgs.lib.hasPrefix "${prefix}/" relPath
          ) includePrefixes;
      in
      isIncluded && gitignorePredicate pathStr type;
  };
in
pkgs.rustPlatform.buildRustPackage {
  pname = "svg-to-png";
  version = "0.1.0";
  buildType = "release";

  nativeBuildInputs = [ pkgs.makeWrapper ];

  src = filteredSrc;
  cargoRoot = "svg-to-png";
  buildAndTestSubdir = "svg-to-png";
  cargoLock = {
    lockFile = ./src/svg-to-png/Cargo.lock;
  };

  postFixup = ''
    mv $out/bin/svg-to-png $out/bin/.svg-to-png-real
    makeWrapper ${sandbox_run}/bin/sandbox-run $out/bin/svg-to-png \
      --add-flags -- \
      --set SVG_FONT_EMBED_DEBUG ${if debug then "1" else "0"} \
      --add-flags $out/bin/.svg-to-png-real
  '';
}
