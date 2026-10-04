# Verilog DLS Export Implementation Plan

> For agentic workers: use the native execution workflow for orchestration, with
> independent project-format implementation and simulator verification tasks.

**Goal:** Export synthesizable Verilog, including registers and clocks, as a
Digital-Logic-Sim project.

**Architecture:** The CLI runs bounded Yosys synthesis and publishes the result.
The DLS profile serializes a mapped JSON netlist as native project/chip files.
An independent harness runs upstream DLS simulation code to verify behavior.

**Tech Stack:** Rust 1.85, Clap, serde_json, Yosys/ABC, Icarus Verilog; isolated
.NET SDK for upstream simulator verification.

**Spec:** `docs/superpowers/specs/2026-10-04-verilog-dls-export-design.md`

## Global constraints

- Preserve the seven existing fixes and the core's combinational-only contract.
- Only DLS export is supported in this version.
- Rust 1.85; source 8 MiB/file and 32 MiB total; JSON 32 MiB; 10,000 cells;
  4,096 bits/port; chip files 8 MiB and project 64 MiB.
- Publish only new output directories; do not overwrite existing data.
- Do not invoke a shell for synthesis or silently replace unsupported logic.

## Review focus

- Bus chunk labels and connectivity preserve ascending ranges and nonzero offsets.
- Reset deassertion and clock changes preserve register state after settling.
- Case-insensitive chip-name collisions never replace a DLS built-in.
- Tool failures, timeout and hostile filenames cannot publish partial output.
- Constants and feedback registers work in DLS's actual scheduling model.

## Task 1: Native DLS project generation

Files: `crates/jsonrtl-profiles/src/dls/export.rs`, module registration in
`dls/mod.rs`, and `crates/jsonrtl-profiles/tests/dls_export.rs`.

Interface:

```rust
pub struct ExportProject {
    pub top_chip: String,
    pub project_description: String,
    pub chips: std::collections::BTreeMap<String, String>,
}
pub fn from_yosys_json(
    json: &str, top: &str, project_name: &str, clock: Option<&str>,
) -> Result<ExportProject, crate::ProfileError>;
```

- [x] Write and run failing tests for shape, primitive NAND/NOT connectivity,
  constants, aliases, chunked and reversed ports, name collisions and invalid graphs.
- [x] Implement bounded parsing, graph validation, native fields and layout.
- [x] Test and implement nested latches and edge/register reset helpers.
- [x] Test and implement the optional built-in-clock wrapper.
- [x] Run profile tests and review every emitted pin/wire address.

## Task 2: CLI synthesis and publication

Files: new `crates/jsonrtl-cli/src/export.rs`, registration in `main.rs`,
`Cargo.toml` for Unix process management, `tests/export.rs`, CLI docs and CI.

- [x] Write CLI tests expecting `export` to create a valid project and reject
  malformed input, wrong tops, unsupported profiles and existing destinations.
- [x] Run them against the old binary and verify failures.
- [x] Add `ExportArgs`, direct `Command` invocation, bounded log/JSON reads,
  timeout with child cleanup, and staged publication.
- [x] Add real Yosys and importer round-trip tests plus fake-tool failure/timeout
  tests; test filenames containing spaces and parser punctuation.
- [x] Document commands, DLS installation/opening steps and supported semantics;
  install HDL tools in the CI test job.

## Task 3: Independent simulator verification and integration

Files: reusable verification harness under `scripts/dls-verification/` if feasible.

- [x] Compile upstream simulator code with minimal Unity/UI stubs in an isolated
  harness; deserialize generated descriptions and connect exact wire addresses.
- [x] Run Boolean/arithmetic/bus/constant scenarios and compare with Icarus output.
- [x] Run rising/falling registers, enable, synchronous/asynchronous resets,
  counters and clock-wrapper scenarios over many clock transitions.
- [x] Fix discrepancies in product code and retain regressions.
- [x] Run workspace tests on stable and Rust 1.85, formatting, Clippy, doc tests,
  release build and an independent combined review.

## Completion evidence

All three tasks are complete. The final verification record is
[`../../verilog-dls-export-verification.md`](../../verilog-dls-export-verification.md).
The seven earlier fixes were preserved. All changes remain in the working tree;
no commit, push, or external publication was requested.

The source-state regression failed when Yosys optimized an X/Z mux branch into
ordinary logic. The final pipeline snapshots elaborated source before mapping
and rejects unknown states before publishing; both workspace suites and the
50,180 native comparisons passed after that fix.

Conservative decision: require a defined case default when Yosys introduces an
unknown one. This prevents arbitrary X replacement, at the cost of requiring a
default in some otherwise exhaustive binary case lists.

Independent review found no critical or important defect. Deferred minor:
cap temporary synthesis log disk consumption; its displayed tail is bounded,
but the log file itself is not. Unity GUI and Windows/macOS execution remain
outside the verified scope.
