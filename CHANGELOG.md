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
- **Logisim / Logisim Evolution import profile** (`logisim`). Parses `.circ`
  XML and rebuilds connectivity geometrically: port positions are recomputed
  from each component's anchor, facing, size, and input count, and points that
  touch — including a wire ending part-way along another — merge into one net.
  Supports `Pin` and the basic gates, folding gates wider than two inputs into
  the 2-input catalog. Multi-bit signals, other libraries, and subcircuit
  instances are rejected with a diagnostic naming the component and coordinate.
  Marked experimental until the geometry is calibrated against real exports.
- `roxmltree` dependency for reading `.circ` XML.
- **Canonical schema v1.1: sliced connections.** A component connection may now
  be `{ net, msb, lsb }` naming a contiguous inclusive bit range instead of a
  bare net ID, which is what makes a bus splitter expressible. `Connection` is
  untagged, so every v1.0 document parses unchanged, and a slice inside a
  document declaring `1.0` is rejected. The single-driver rule now resolves per
  bit, because a bus merger legitimately compiles to
  `assign d[0] = ...; assign d[1] = ...;`, and combinational-cycle edges require
  driver and consumer bit ranges to intersect. New diagnostics
  `SLICE_OUT_OF_RANGE` and `SLICE_REQUIRES_SCHEMA_1_1`.
- **Multi-bit buses in the DLS profile (Phase M).** Pins wider than one bit keep
  their width to the module boundary, so a 16-bit adder exposes
  `input wire [7:0] A0;` rather than eight scalar ports. Elaboration now runs
  union-find over single bits, which makes DLS's `X-YBIT` splitters and mergers,
  `BUS-N` aliases, and `BUS-TERMINUS-N` sinks pure re-labelling that emits no
  logic. Bit ordering is most significant first, verified against the carry
  chain of a real 16-bit adder.

### Fixed

- **`--chip` blocked by unrelated chips.** `import` converted the whole project
  before applying `--chip`, so one unsupported chip anywhere failed every other
  chip: on a real DLS project `--chip AND` reported an error about a 16-bit
  adder and exited 2. `Profile::convert_unit` now elaborates only the requested
  unit's dependency closure.

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
