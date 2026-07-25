# Design: Format Profiles + DLS Import + CLI `import`

Date: 2026-07-25
Status: Approved (design) — pending written-spec review
Owner: jsonrtl

## Goal

Let the kernel compile *other* digital-logic JSON formats to Verilog by first
converting them to canonical circuit JSON v1.0, then reusing the existing
compiler. Ship this as:

1. A **profiles** subsystem — a `profiles/` folder plus a `jsonrtl-profiles`
   crate — that converts a foreign project into canonical documents.
2. A first profile for **Sebastian Lague's Digital-Logic-Sim (DLS)**.
3. A CLI `import` command: *give it a project directory, get Verilog back*,
   emitted as one `.v` per chip, mirroring the input project layout.

Plus a **production-hardening** pass on the kernel (CI, error-path audit,
release metadata, docs).

This is **Phase 1 (the spine)**. Three larger kernel feature-phases are staged
after it, each with its own spec: **M** multi-bit buses (audit — largely already
works), **H** hierarchical modules (schema v1.1), **S** sequential (schema v2).

## Non-goals (this phase)

- No hierarchical Verilog emission. DLS chips are flattened to primitives.
- No sequential, tri-state, bus/merge-split, display, or memory support in the
  DLS profile. These are diagnosed and rejected, not silently skipped.
- No new kernel logical features. The DLS profile targets the *existing* flat
  canonical model and the existing `Kernel::compile_verilog`.

## Background: the two models

**Canonical kernel model** (`jsonrtl`) is flat: a `Circuit` has `ports`
(module boundary), `components` (primitive gates: AND/OR/XOR/XNOR/NAND/NOR/NOT/
BUFFER/CONST), and `nets`. NAND's logical ports are `A`,`B` (in) and `Y` (out),
uniform width. Multi-bit (`width > 1`) already compiles end-to-end. There is no
sub-module instantiation.

**DLS project** is a directory:
- `ProjectDescription.json` — metadata + `AllCustomChipNames`.
- `Chips/<Name>.json` — one hierarchical chip each: `InputPins[]` / `OutputPins[]`
  (module boundary, each with a unique integer `ID`, a `Name`, `BitCount`),
  `SubChips[]` (instances of other chips referenced **by `Name`**, each with a
  unique instance `ID`), and `Wires[]` connecting pins addressed as
  `{PinID, PinOwnerID}`.

DLS bottoms out at one true combinational primitive, **NAND** (input pins `0`,`1`;
output pin `2`). Every custom chip composes down to NAND.

### Pin addressing (decoded from real fixtures)

For a wire endpoint `{PinID, PinOwnerID}` inside chip `C`:
- If `PinOwnerID` matches one of `C`'s own `InputPins[].ID` / `OutputPins[].ID`
  → it is a **module-boundary pin**; `PinID` is `0` and ignored.
- If `PinOwnerID` matches a `SubChips[].ID` → it is a **subchip pin**:
  - subchip type is the referenced chip's `Name`;
  - if the subchip is the built-in **NAND**, `PinID` `0`/`1` = inputs, `2` = output;
  - if the subchip is a **custom chip `D`**, `PinID` equals one of `D`'s own
    `InputPins[].ID` (a sink into the subchip) or `OutputPins[].ID` (a driver out
    of the subchip). It selects the matching boundary pin of the inlined copy of `D`.

Driver/sink roles: module **input** pins and NAND **output** pins are drivers;
module **output** pins and NAND **input** pins are sinks.

## Architecture

```
crates/
  jsonrtl/            (unchanged public API)
  jsonrtl-cli/        (+ `import` subcommand)
  jsonrtl-api/        (unchanged this phase)
  jsonrtl-profiles/   NEW library: Profile trait + registry + dls module
                           depends on jsonrtl (inward only)
profiles/                  NEW folder
  dls/
    profile.toml           manifest: id, name, source-format, supported subset
    README.md              format docs + mapping rules + limitations
    fixtures/              sample DLS project(s) + expected canonical/Verilog
```

Dependency direction is preserved: `jsonrtl-profiles → jsonrtl`. The
core library gains no dependency on profiles, transports, or the DLS format.

### `Profile` trait

```rust
pub trait Profile {
    /// Stable id, e.g. "dls".
    fn id(&self) -> &'static str;
    /// True if `path` looks like a project this profile can convert.
    fn detect(&self, path: &Path) -> bool;
    /// Convert a project directory into canonical documents.
    fn convert(&self, path: &Path) -> Result<ProjectConversion, ProfileError>;
}

pub struct ProjectConversion {
    pub project_name: String,
    /// One canonical document per emittable chip, keyed by chip name.
    pub circuits: Vec<NamedCircuit>,   // { name: String, document: CircuitDocument }
}
```

A small `registry()` maps `id -> Box<dyn Profile>`, and `detect_profile(path)`
picks one when `--profile` is omitted. `ProfileError` carries a foreign-format
diagnostic (which chip / pin / unsupported construct) distinct from kernel
diagnostics.

### DLS profile conversion (flatten to NAND)

1. **Load** `ProjectDescription.json` and every `Chips/*.json` into a
   `name -> ChipDef` map (serde structs mirroring the DLS shape).
2. **For each custom chip** to emit, **elaborate** a flat primitive netlist:
   - Union-find over wire endpoints builds nets: each `Wire` unions its source and
     target endpoints. Recursively inline custom subchips by instantiating a fresh
     copy of the subchip's internal graph (freshly-namespaced endpoint keys) and
     stitching the parent wire endpoints to the copy's boundary pins.
   - After elaboration every connected component of endpoints is one **net** with
     exactly one driver (a module input pin or a NAND `Y`) and zero-or-more sinks.
3. **Lower to canonical**:
   - each NAND instance → a `NAND` `Component` (width 1) with `A`,`B` → its input
     nets, `Y` → its output net;
   - each module `InputPin` → an `input` `ModulePort` bound to its net;
   - each module `OutputPin` → an `output` `ModulePort` bound to its net;
   - a module-input-to-module-output pass-through inserts a `BUFFER` so the output
     port has a component driver (avoids relying on port-drives-port);
   - build the `Net` list; assign stable canonical IDs derived from DLS integer IDs.
4. **Validate + compile** each `CircuitDocument` with `Kernel::compile_verilog`.
   The kernel enforces single-driver, no-cycle, width agreement. Combinational DLS
   chips satisfy these.

### Rejected (diagnosed, not skipped)

Encountering any of the following stops conversion of that chip with a precise
diagnostic naming the chip and offending pin/subchip:
- any pin with `BitCount != 1` (multi-bit — Phase M);
- any built-in other than NAND: CLOCK, PULSE, KEY, 3-STATE BUFFER, BUS-*,
  merge/split (`1-4BIT` …), 7-SEGMENT / displays, ROM / memory;
- a combinational cycle or missing/multiple driver (surfaced via kernel
  diagnostics).

### CLI `import`

```
jsonrtl import <project-dir>
    [--profile <id>]         # default: auto-detect
    [--out <dir>]            # write one <ChipName>.v per emittable chip
    [--chip <name>]          # restrict to one chip
    [--stdout]               # print a single chip's Verilog (requires --chip)
    [--emit-canonical <dir>] # also dump intermediate canonical JSON per chip
    [--force]                # allow overwriting existing outputs
```

Default: convert all custom chips and write `<out>/<ChipName>.v`, mirroring the
`Chips/*.json` layout ("Verilog in the same format" = a Verilog project mirroring
the DLS project). Reuses the existing atomic-write, diagnostics (`--diagnostics
human|json`), and exit-code machinery. `--emit-canonical` makes the
"foreign JSON → canonical kernel JSON" step inspectable.

## Production hardening (kernel)

- **CI** (GitHub Actions): `cargo fmt --check`, `cargo clippy --all-targets -D
  warnings`, `cargo test --workspace`, `cargo build --release`.
- **Error-path audit**: no `unwrap`/`expect`/`panic` on untrusted input paths;
  profile + CLI return diagnostics, never panic.
- **Release metadata**: workspace release profile, version, `CHANGELOG.md`.
- **Docs**: `docs/profiles.md`, update `docs/cli.md` (the `import` command) and
  `README.md`.

## Testing

- **Unit**: pin-address decoding; union-find net building; single-level inline;
  recursive multi-level inline; rejection paths (multi-bit, non-NAND built-in).
- **Golden**: convert the bundled DLS `test` project (AND/OR/XOR/NOT/1-bit adder)
  → canonical JSON and Verilog; commit both as golden and diff in CI. The AND
  chip must flatten to two NANDs and compile; the 1-bit adder must compile with
  correct sum/carry structure.
- **CLI integration**: `import <fixture> --out tmp` writes the expected file set;
  `--chip AND --stdout` prints the AND module; unsupported fixture fails with a
  clear diagnostic and the documented exit code.

## Staged follow-up phases (separate specs)

- **M — Multi-bit buses**: audit current width>1 support (compiles today);
  extend the DLS profile to accept `BitCount > 1` and DLS bus/merge/split.
- **H — Hierarchical modules**: canonical schema v1.1 + IR + Verilog module
  instantiation; DLS profile emits one module per chip instead of flattening.
- **S — Sequential**: schema v2 registers/clock; DLS CLOCK/PULSE support.
```

## Risks & mitigations

- **Pin-ID model wrong for an untested case** → validated against the real
  bundled fixtures (AND = 2×NAND, 1-bit adder) as golden tests before shipping.
- **Recursive inlining blowup / cycles in the chip graph** → bound recursion by a
  chip-dependency depth limit and detect chip-reference cycles with a clear error.
- **Pass-through / constant handling** → BUFFER insertion keeps every output port
  component-driven; DLS has no CONST in the combinational subset this phase.
