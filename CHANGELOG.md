# Changelog

All notable changes to this project are documented here. The format is loosely
based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).

## [Unreleased]

### Added

- **Format import profiles** (`jsonrtl-profiles` crate): a `Profile` trait,
  registry, and `ProfileError` taxonomy for converting third-party project
  formats into canonical circuit documents. Profiles depend only on the public
  `jsonrtl` contract.
- **DLS import profile** for Sebastian Lague's Digital-Logic-Sim: loads a project
  directory (`ProjectDescription.json` + `Chips/*.json`), flattens each
  hierarchical chip down to NAND primitives, and lowers it to canonical JSON.
  Supports the combinational, single-bit subset; multi-bit pins and every
  non-NAND built-in are rejected with a precise diagnostic.
- **CLI `import` command**: `jsonrtl import <project-dir> [--profile ID]
  [--out DIR] [--chip NAME] [--stdout] [--emit-canonical DIR] [--force]`. Emits
  one `<ChipName>.v` per chip, mirroring the project layout; reuses the existing
  diagnostics, atomic writes, and exit codes.
- `profiles/dls/` folder with the profile manifest, documentation, and a runnable
  example project.
- `docs/profiles.md` reference and byte-exact golden Verilog coverage for the DLS
  example project.
- CI workflow (`fmt --check`, `clippy -D warnings`, `test`, doc tests, release
  build).

### Fixed

- **Path traversal in `import`.** Chip names from an untrusted
  `ProjectDescription.json` were joined unsanitized onto the input and output
  directories, so a name like `../escaped` read and wrote outside them — and
  exited `0` reporting success. Names must now be a single ordinary path
  component; the CLI re-checks them before writing.
- **Unbounded memory during flattening.** Each level that instantiates its child
  twice doubles the instance count, so a depth-20 hierarchy allocated ~2 GB over
  24 s before the kernel's limits could reject it. Flattening is now capped at
  50,000 NAND instances and fails fast (~80 MB, ~1 s).
- **Silent mis-wiring on id collision.** A sub-chip instance id equal to one of
  its chip's boundary-pin ids resolved every wire on that sub-chip to the pin,
  surfacing as misleading `NET_NO_DRIVER` errors. Colliding and duplicate ids are
  now rejected with a diagnostic naming the id.
- **Partial output on a failed import.** Verilog files were written one at a
  time, so a failure part-way left earlier units on disk. `import` now refuses
  the run if any destination exists without `--force`.
- **Misleading duplicate-name error.** A chip listed twice in
  `AllCustomChipNames` reported `already exists; pass --force` against an empty
  output directory instead of naming the real cause.

## [0.1.0]

- Canonical JSON Schema v1.0, typed public model, parse/schema diagnostics,
  component catalog, resource limits, semantic validation, normalized IR,
  deterministic Verilog-2001 compiler, source maps, and CLI/Axum shells.
