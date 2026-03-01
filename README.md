# Development Testing Guide

This repo has a lightweight `shell.nix` for fast Rust iteration.

## Enter the Dev Shell

```bash
nix-shell
```

The shell provides:

- `rustc`
- `cargo`
- `fontconfig`
- `pyftsubset` (via `fonttools`)
- `PYFTSUBSET_BIN` and `NIX_STORE_DIR` environment variables

## Run Tests Per Rust Project

From the repository root:

### `diagram-generator`

```bash
cd src/diagram-generator
cargo test
```

### `svg-font-inliner`

```bash
cd src/svg-font-inliner
cargo test
```

### `svg-to-png`

```bash
cd src/svg-to-png
cargo test
```

### `sandbox-run`

```bash
cd src/sandbox-run
cargo test
```

## Run All Rust Tests in One Command

From the repository root:

```bash
nix-shell --run 'set -e; for p in src/diagram-generator src/svg-font-inliner src/svg-to-png src/sandbox-run; do echo "==> $p"; (cd "$p" && cargo test); done'
```

## Check for Rust Dependency Updates

The dev shell includes `cargo-outdated`.

Check one project:

```bash
cd src/svg-font-inliner
cargo outdated --root-deps-only
```

Check all Rust projects from the repository root:

```bash
nix-shell --run 'set -e; for p in src/diagram-generator src/svg-font-inliner src/svg-to-png src/sandbox-run; do echo "==> $p"; (cd "$p" && cargo outdated --root-deps-only); echo; done'
```

## Full Contract Test Suite (Node-based)

For CLI contract tests, use the existing test harness:

```bash
nix-build test.nix
```

For details on contract tests and `run-test`, see `tests/README.md`.
