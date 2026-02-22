let
  nixpkgs = fetchTarball "https://github.com/NixOS/nixpkgs/tarball/nixos-25.05";
  pkgs = import nixpkgs { config = {}; overlays = []; };
  fontconfig = import ./tests/default-fontconfig.nix { inherit pkgs; };
  packages = import ./default.nix { inherit pkgs fontconfig; };
  testFiles = builtins.sort (a: b: a < b) (builtins.filter
    (name: pkgs.lib.hasSuffix ".test.mjs" name)
    (builtins.attrNames (builtins.readDir ./tests))
  );
  runAllTests = builtins.concatStringsSep "\n" (map (file: ''
    test_name="${pkgs.lib.removeSuffix ".test.mjs" file}"
    test_file="${./tests}/${file}"
    test_out_dir="$out/$test_name"
    mkdir -p "$test_out_dir"
    TEST_OUT_DIR="$test_out_dir" DIAGRAM_GENERATOR_BIN="${packages.bin}/bin/diagram-generator" SUPPORTED_VERSIONS_JSON="${./supported-versions.json}" node "$test_file"
  '') testFiles);
in
  pkgs.runCommand "diagram-generator-tests" {
    nativeBuildInputs = [
      pkgs.nodejs_latest
    ];
    shellHook = ''
      export DIAGRAM_GENERATOR_BIN="${packages.bin}/bin/diagram-generator"
      export SUPPORTED_VERSIONS_JSON="${./supported-versions.json}"
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
      echo "SUPPORTED_VERSIONS_JSON=$SUPPORTED_VERSIONS_JSON"
      echo "DG_TEST_TMP=$DG_TEST_TMP"
      echo "use: run-test tests/<name>.test.mjs"
    '';
  } ''
    set -euo pipefail

    if [ ${toString (builtins.length testFiles)} -eq 0 ]; then
      echo "no tests found in ./tests (*.test.mjs)" >&2
      exit 1
    fi

    mkdir -p "$out"
    ${runAllTests}
  ''
