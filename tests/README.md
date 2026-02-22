# Tests

This repository tests `diagram-generator` as a CLI contract (stdin JSON in, stdout JSON out), not as in-process unit tests.

## Test Layout

- Test files live in `tests/` and use the naming pattern `*.test.mjs`.
- `test.nix` auto-discovers all `tests/*.test.mjs` and runs them.
- Shared helpers are in `tests/_helpers.mjs`.

Current grouped tests:

- `tests/contract-input-validation.test.mjs`
- `tests/contract-renderer-list.test.mjs`
- `tests/contract-batch-behavior.test.mjs`
- `tests/contract-render-matrix.test.mjs`
- `tests/contract-font-inline.test.mjs`

## Running Tests

Run the full suite:

```bash
nix-build test.nix
```

This produces a `result/` symlink with one output directory per test.

## Output Artifacts

Each test writes debug artifacts into its own output directory under `result/`:

- request payloads (`*.stdin.json`)
- raw CLI outputs (`*.stdout.json`, `*.stderr.txt`)
- rendered artifacts (`*.svg`, `*.png`) for render-matrix tests
- `success.txt` when the test completes

## Running a Single Test (Interactive)

Open the test shell:

```bash
nix-shell test.nix
```

Inside the shell, run one test by full path:

```bash
run-test tests/contract-renderer-list.test.mjs
```

The helper prints the output directory path (under a temporary directory).

## Debugging Failures

Recommended flow:

1. Run all tests with `nix-build test.nix`.
2. Inspect artifacts under `result/<test-name>/`.
3. Re-run only the failing test with `nix-shell test.nix` + `run-test <full-path>`.

Useful notes:

- `test.nix` exports these environment variables for tests:
  - `DIAGRAM_GENERATOR_BIN`
  - `SUPPORTED_VERSIONS_JSON`
  - `TEST_OUT_DIR` (set per test by runner)
- In interactive mode, `run-test` also creates a per-shell temp base directory (`DG_TEST_TMP`) and per-test output subdirectories.
