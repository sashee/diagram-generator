let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.11";
  pkgs = import nixpkgs { config = {}; overlays = []; };
  packages = import ./default.nix;
  versions = packages.supported_versions;
  fontconfig = import ./tests/default-fontconfig.nix { inherit pkgs; };
  fontconfigProfileA = import ./tests/fontconfig-profile-a.nix { inherit pkgs; };
  fontconfigProfileB = import ./tests/fontconfig-profile-b.nix { inherit pkgs; };
  diagramGenerator = packages.diagram_generator {
    debug = false;
    inherit pkgs fontconfig versions;
  };
  diagramGeneratorA = packages.diagram_generator {
    debug = false;
    inherit pkgs versions;
    fontconfig = fontconfigProfileA;
  };
  diagramGeneratorB = packages.diagram_generator {
    debug = false;
    inherit pkgs versions;
    fontconfig = fontconfigProfileB;
  };
  svgToPng = packages.svg_to_png { debug = false; inherit pkgs; };
  svgFontInliner = packages.svg_font_inliner {
    debug = false;
    inherit pkgs fontconfig;
  };
  svgFontInlinerA = packages.svg_font_inliner {
    debug = false;
    inherit pkgs;
    fontconfig = fontconfigProfileA;
  };
  svgFontInlinerB = packages.svg_font_inliner {
    debug = false;
    inherit pkgs;
    fontconfig = fontconfigProfileB;
  };

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
      TEST_OUT_DIR="$out" DIAGRAM_GENERATOR_BIN="${diagramGenerator}/bin/diagram-generator" DIAGRAM_GENERATOR_BIN_A="${diagramGeneratorA}/bin/diagram-generator" DIAGRAM_GENERATOR_BIN_B="${diagramGeneratorB}/bin/diagram-generator" SVG_TO_PNG_BIN="${svgToPng}/bin/svg-to-png" SVG_FONT_INLINER_BIN="${svgFontInliner}/bin/svg-font-inliner" SVG_FONT_INLINER_BIN_A="${svgFontInlinerA}/bin/svg-font-inliner" SVG_FONT_INLINER_BIN_B="${svgFontInlinerB}/bin/svg-font-inliner" SUPPORTED_VERSIONS_JSON="${./supported-versions.json}" node "${testsDir}/${file}"
    '';

  testNames = map (file: pkgs.lib.removeSuffix ".test.mjs" file) testFiles;
  testDerivationsByName = builtins.listToAttrs (map (file: {
    name = pkgs.lib.removeSuffix ".test.mjs" file;
    value = mkTestDerivation file;
  }) testFiles);

  allTests = let
    testsLinkFarm = pkgs.linkFarm "diagram-generator-tests" (
      (map (testName: {
        name = testName;
        path = testDerivationsByName.${testName};
      }) testNames)
      ++ [
        {
          name = "svg-font-inliner";
          path = svgFontInliner;
        }
        {
          name = "svg-to-png";
          path = svgToPng;
        }
      ]
    );
  in
    pkgs.runCommand "diagram-generator-tests-with-index" {
      nativeBuildInputs = [ pkgs.bash ];
    } ''
      mkdir -p "$out"
      cp -r ${testsLinkFarm}/* "$out/"
      chmod -R u+w "$out"

      cp "${./tests/generate-central-index.sh}" "$out/generate-index.sh"
      chmod +x "$out/generate-index.sh"
      "${pkgs.bash}/bin/bash" "$out/generate-index.sh" "$out"
    '';

  shell = pkgs.mkShell {
    nativeBuildInputs = [
      pkgs.nodejs_latest
      pkgs.rustc
      pkgs.cargo
      pkgs.python3Packages.fonttools
      pkgs.fontconfig
    ];
    shellHook = ''
      export DIAGRAM_GENERATOR_BIN="${diagramGenerator}/bin/diagram-generator"
      export DIAGRAM_GENERATOR_BIN_A="${diagramGeneratorA}/bin/diagram-generator"
      export DIAGRAM_GENERATOR_BIN_B="${diagramGeneratorB}/bin/diagram-generator"
      export SVG_TO_PNG_BIN="${svgToPng}/bin/svg-to-png"
      export SVG_FONT_INLINER_BIN="${svgFontInliner}/bin/svg-font-inliner"
      export SVG_FONT_INLINER_BIN_A="${svgFontInlinerA}/bin/svg-font-inliner"
      export SVG_FONT_INLINER_BIN_B="${svgFontInlinerB}/bin/svg-font-inliner"
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
        TEST_OUT_DIR="$test_out_dir" DIAGRAM_GENERATOR_BIN="$DIAGRAM_GENERATOR_BIN" DIAGRAM_GENERATOR_BIN_A="$DIAGRAM_GENERATOR_BIN_A" DIAGRAM_GENERATOR_BIN_B="$DIAGRAM_GENERATOR_BIN_B" SVG_TO_PNG_BIN="$SVG_TO_PNG_BIN" SVG_FONT_INLINER_BIN="$SVG_FONT_INLINER_BIN" SVG_FONT_INLINER_BIN_A="$SVG_FONT_INLINER_BIN_A" SVG_FONT_INLINER_BIN_B="$SVG_FONT_INLINER_BIN_B" SUPPORTED_VERSIONS_JSON="$SUPPORTED_VERSIONS_JSON" node "$test_file"
        status="$?"
        echo "output dir: $test_out_dir"
        return "$status"
      }

      echo "DIAGRAM_GENERATOR_BIN=$DIAGRAM_GENERATOR_BIN"
      echo "DIAGRAM_GENERATOR_BIN_A=$DIAGRAM_GENERATOR_BIN_A"
      echo "DIAGRAM_GENERATOR_BIN_B=$DIAGRAM_GENERATOR_BIN_B"
      echo "SVG_TO_PNG_BIN=$SVG_TO_PNG_BIN"
      echo "SVG_FONT_INLINER_BIN=$SVG_FONT_INLINER_BIN"
      echo "SVG_FONT_INLINER_BIN_A=$SVG_FONT_INLINER_BIN_A"
      echo "SVG_FONT_INLINER_BIN_B=$SVG_FONT_INLINER_BIN_B"
      echo "SUPPORTED_VERSIONS_JSON=$SUPPORTED_VERSIONS_JSON"
      echo "PYFTSUBSET_BIN=$PYFTSUBSET_BIN"
      echo "DG_TEST_TMP=$DG_TEST_TMP"
      echo "use: run-test tests/<name>.test.mjs"
    '';
  };
in
if pkgs.lib.inNixShell then shell else allTests
