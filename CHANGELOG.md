# Changelog

All notable changes to this project are documented here. The format is loosely
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- **Format import profiles** (`logic-kernel-profiles` crate): a `Profile` trait,
  registry, and `ProfileError` taxonomy for converting third-party project
  formats into canonical circuit documents. Profiles depend only on the public
  `logic-kernel` contract.
- **DLS import profile** for Sebastian Lague's Digital-Logic-Sim: loads a project
  directory (`ProjectDescription.json` + `Chips/*.json`), flattens each
  hierarchical chip down to NAND primitives, and lowers it to canonical JSON.
  Supports the combinational, single-bit subset; multi-bit pins and every
  non-NAND built-in are rejected with a precise diagnostic.
- **CLI `import` command**: `logic-kernel import <project-dir> [--profile ID]
  [--out DIR] [--chip NAME] [--stdout] [--emit-canonical DIR] [--force]`. Emits
  one `<ChipName>.v` per chip, mirroring the project layout; reuses the existing
  diagnostics, atomic writes, and exit codes.
- `profiles/dls/` folder with the profile manifest, documentation, and a runnable
  example project.
- `docs/profiles.md` reference and byte-exact golden Verilog coverage for the DLS
  example project.
- CI workflow (`fmt --check`, `clippy -D warnings`, `test`, doc tests, release
  build).

## [0.1.0]

- Canonical JSON Schema v1.0, typed public model, parse/schema diagnostics,
  component catalog, resource limits, semantic validation, normalized IR,
  deterministic Verilog-2001 compiler, source maps, and CLI/Axum shells.
