let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.11";
  pkgs = import nixpkgs { config = {}; overlays = []; };
  fontconfig = import ./tests/default-fontconfig.nix { inherit pkgs; };
  packages = import ./default.nix { debug = false; inherit pkgs fontconfig; };

  testFiles = builtins.sort (a: b: a < b) (builtins.filter
    (name: pkgs.lib.hasSuffix ".test.mjs" name)
    (builtins.attrNames (builtins.readDir ./tests))
  );

  _ = if builtins.length testFiles == 0
    then throw "no tests found in ./tests (*.test.mjs)"
    else null;

  mkTestDerivation = file:
    let
      testName = pkgs.lib.removeSuffix ".test.mjs" file;
      testsDir = ./tests;
    in
    pkgs.runCommand "diagram-generator-test-${testName}" {
      nativeBuildInputs = [ pkgs.nodejs_latest pkgs.python3Packages.fonttools pkgs.fontconfig ];
    } ''
      set -euo pipefail
      mkdir -p "$out"
      TEST_OUT_DIR="$out" DIAGRAM_GENERATOR_BIN="${packages.bin}/bin/diagram-generator" SVG_TO_PNG_BIN="${packages.svg_to_png}/bin/svg-to-png" SVG_FONT_INLINER_BIN="${packages.svg_font_inliner}/bin/svg-font-inliner" SUPPORTED_VERSIONS_JSON="${./supported-versions.json}" node "${testsDir}/${file}"
    '';

  testNames = map (file: pkgs.lib.removeSuffix ".test.mjs" file) testFiles;
  testDerivationsByName = builtins.listToAttrs (map (file: {
    name = pkgs.lib.removeSuffix ".test.mjs" file;
    value = mkTestDerivation file;
  }) testFiles);

  allTests = pkgs.linkFarm "diagram-generator-tests" (
    (map (testName: {
      name = testName;
      path = testDerivationsByName.${testName};
    }) testNames)
    ++ [
      {
        name = "svg-font-inliner";
        path = packages.svg_font_inliner;
      }
      {
        name = "svg-to-png";
        path = packages.svg_to_png;
      }
    ]
  );

  shell = pkgs.mkShell {
    nativeBuildInputs = [
      pkgs.nodejs_latest
      pkgs.rustc
      pkgs.cargo
      pkgs.python3Packages.fonttools
      pkgs.fontconfig
    ];
    shellHook = ''
      export DIAGRAM_GENERATOR_BIN="${packages.bin}/bin/diagram-generator"
      export SVG_TO_PNG_BIN="${packages.svg_to_png}/bin/svg-to-png"
      export SVG_FONT_INLINER_BIN="${packages.svg_font_inliner}/bin/svg-font-inliner"
      export SUPPORTED_VERSIONS_JSON="${./supported-versions.json}"
      export PYFTSUBSET_BIN="${pkgs.python3Packages.fonttools}/bin/pyftsubset"
      export DG_TEST_TMP="$(mktemp -d "''${TMPDIR:-/tmp}/diagram-generator-tests.XXXXXX")"

      run-test() {
        if [ "$#" -ne 1 ]; then
          echo "usage: run-test <path/to/test.test.mjs>"
          return 2
        fi

        test_file="$1"
        case "$test_file" in
          /*) ;;
          *) test_file="$PWD/$test_file" ;;
        esac

        if [ ! -f "$test_file" ]; then
          echo "test file not found: $test_file"
          return 1
        fi

        base_name="$(basename "$test_file")"
        test_name="''${base_name%.test.mjs}"
        test_out_dir="$DG_TEST_TMP/$test_name"
        mkdir -p "$test_out_dir"

        echo "running $test_file"
        TEST_OUT_DIR="$test_out_dir" DIAGRAM_GENERATOR_BIN="$DIAGRAM_GENERATOR_BIN" SUPPORTED_VERSIONS_JSON="$SUPPORTED_VERSIONS_JSON" node "$test_file"
        status="$?"
        echo "output dir: $test_out_dir"
        return "$status"
      }

      echo "DIAGRAM_GENERATOR_BIN=$DIAGRAM_GENERATOR_BIN"
      echo "SVG_TO_PNG_BIN=$SVG_TO_PNG_BIN"
      echo "SVG_FONT_INLINER_BIN=$SVG_FONT_INLINER_BIN"
      echo "SUPPORTED_VERSIONS_JSON=$SUPPORTED_VERSIONS_JSON"
      echo "PYFTSUBSET_BIN=$PYFTSUBSET_BIN"
      echo "DG_TEST_TMP=$DG_TEST_TMP"
      echo "use: run-test tests/<name>.test.mjs"
    '';
  };
in
if pkgs.lib.inNixShell then shell else allTests
