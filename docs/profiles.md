# Format Import Profiles

Import profiles convert third-party digital-logic project formats into canonical
circuit JSON v1.0 so the existing kernel can validate them and compile Verilog.
They live in the `jsonrtl-profiles` crate and depend only on the public
`jsonrtl` contract — the core library has no knowledge of any foreign
format.

Profiles are the server-side analogue of a UI adapter (see `ARCHITECTURE.md`):
they map a foreign document onto canonical JSON and never ask the kernel to infer
logic from layout.

## The `Profile` trait

```rust
pub trait Profile {
    fn id(&self) -> &'static str;                 // stable CLI id, e.g. "dls"
    fn detect(&self, path: &Path) -> bool;        // does this dir look like ours?
    fn convert(&self, path: &Path)                // foreign project -> canonical
        -> Result<ProjectConversion, ProfileError>;
}
```

Profiles also describe themselves — `source`, `input_hint`, `supports`, and a
`status` of `stable` or `experimental` — which is what `jsonrtl profiles`
prints.

- `registry()` lists every profile in the build.
- `profile_by_id(id)` selects one explicitly.
- `detect_profile(path)` auto-selects when the caller does not name one.
- `units(path)` lists a project's units without converting any.
- `convert_unit(path, unit)` converts one unit and its dependencies, so a
  broken chip elsewhere in the project cannot fail it.

A successful `convert` returns a `ProjectConversion { project_name, circuits }`
where each `NamedCircuit { name, document }` is one canonical
`CircuitDocument` — for DLS, one per chip.

`ProfileError` reports *conversion* failures (distinct from kernel validation
diagnostics), always naming the offending source unit:

| Variant | Meaning |
| --- | --- |
| `Io` | a file or directory could not be read |
| `Parse` | a foreign file was present but invalid for its format |
| `Unsupported` | a construct outside the profile's supported subset |
| `Structure` | dangling reference, cycle, excess depth, missing/duplicate driver |
| `Limit` | conversion would exceed a profile resource bound |
| `UnknownUnit` | the caller asked for a unit the project does not contain |

## Status

| Piece | State |
| --- | --- |
| `Profile` trait, registry, `ProfileError` | landed |
| DLS project detection (`detect`) | landed |
| DLS serde model + project loader | landed |
| DLS flatten-to-NAND elaboration + lowering | landed |
| CLI `import` command | landed |
| `profiles/dls/` manifest, docs, example | landed |
| Logisim/Evolution profile (`.circ`, gates + pins) | landed, experimental |
| Logisim subcircuit instances | blocked on port-geometry calibration |
| Multi-bit buses (Phase M) | landed for DLS (schema v1.1 slices); Logisim rejects |
| Hierarchical Verilog emission (Phase H) | staged |
| Sequential / clock (Phase S) | staged |

Canonical schema v1.1 adds sliced connections (`{ net, msb, lsb }`), which is
what lets a bus splitter be expressed at all. See
`docs/superpowers/specs/2026-07-25-multi-bit-buses-design.md`.

Design and task breakdown:
`docs/superpowers/specs/2026-07-25-profiles-and-dls-import-design.md` and
`docs/superpowers/plans/2026-07-25-profiles-and-dls-import.md`.

## The DLS profile (Sebastian Lague's Digital-Logic-Sim)

### Source layout

A DLS project is a directory:

- `ProjectDescription.json` — metadata plus `AllCustomChipNames`.
- `Chips/<Name>.json` — one hierarchical chip each: `InputPins[]` / `OutputPins[]`
  (module boundary; each has a unique integer `ID`, a `Name`, and a `BitCount`),
  `SubChips[]` (instances of other chips referenced **by `Name`**, each with a
  unique instance `ID`), and `Wires[]` connecting pins addressed as
  `{PinID, PinOwnerID}`.

Every custom chip composes down to one true combinational primitive, **NAND**
(input pins `0`,`1`; output pin `2`).

### Pin addressing

For a wire endpoint `{PinID, PinOwnerID}` inside chip `C`:

- If `PinOwnerID` matches one of `C`'s own `InputPins[].ID` / `OutputPins[].ID`,
  it is a **module-boundary pin**; `PinID` is `0` and ignored.
- If `PinOwnerID` matches a `SubChips[].ID`, it is a **subchip pin**:
  - the subchip type is the referenced chip's `Name`;
  - for the built-in **NAND**, `PinID` `0`/`1` are inputs and `2` is the output;
  - for a **custom subchip `D`**, `PinID` equals one of `D`'s own
    `InputPins[].ID` (a sink into the subchip) or `OutputPins[].ID` (a driver out
    of it), selecting the matching boundary pin of the inlined copy of `D`.

Module **input** pins and NAND **outputs** are drivers; module **output** pins
and NAND **inputs** are sinks.

### Conversion (flatten to NAND)

1. Load the project into a `name -> ChipDef` map.
2. For each chip, elaborate a flat NAND netlist: union-find over wire endpoints
   builds nets, and custom subchips are recursively inlined with fresh instance
   namespacing.
3. Lower to canonical: each NAND instance becomes a `NAND` component (`A`,`B`→`Y`);
   each boundary pin becomes an input/output `ModulePort`; pass-throughs insert a
   `BUFFER`. Stable canonical IDs derive from DLS integer IDs.
4. Hand each document to `Kernel::compile_verilog`, which enforces single-driver,
   no-cycle, and width agreement.

### Supported subset and rejections

This phase supports **combinational** logic of any width, with **NAND** as the
only gate primitive.

### Buses

Pins wider than one bit keep their width all the way to the Verilog module
boundary, so a 16-bit adder exposes `input wire [7:0] A0;` rather than eight
scalar ports. DLS decomposes buses with built-in splitter and merger chips
named `X-YBIT`, meaning "convert X-bit to Y-bit":

| Chip | Role | Input pins | Output pins |
| --- | --- | --- | --- |
| `8-1BIT` | split 8-bit into 8 bits | `0` | `1..8` |
| `1-8BIT` | merge 8 bits into 8-bit | `0..7` | `8` |
| `8-4BIT` | split 8-bit into two nibbles | `0` | `1..2` |
| `4-8BIT` | merge two nibbles into 8-bit | `0..1` | `2` |

Both orderings are **most significant first**: split output pin `k` (1-based) is
bit `N - k`, and merge input pin `j` (0-based) is bit `N - 1 - j`.

Two further built-ins are pure routing: `BUS-N` is a fan-out alias (pin `0` in,
pin `1` out), and `BUS-TERMINUS-N` is the far end of a drawn bus line, which
drives nothing and is dropped.

None of these emit any logic. Elaboration runs union-find over **single bits**,
so a narrow pin simply *is* a particular bit of its wide pin and both sides
share a node. Gates then address individual bits through v1.1 slices.

### Rejections

The following are rejected with a precise `ProfileError`, never silently
skipped:

- any built-in outside the set above: `CLOCK`, `PULSE`, `KEY`,
  `3-STATE BUFFER`, `7-SEGMENT` / displays, `ROM` / memory;
- a wire joining pins of different widths;
- combinational cycles or missing/multiple drivers (surfaced by the kernel).
  Latch structures built from NAND feedback — DLS registers and RAM — are
  genuine cycles for a combinational kernel and are reported as such.

## Untrusted-input rules

Project files are untrusted. The profile enforces these before doing any work:

| Rule | Why |
| --- | --- |
| A chip name must be a single ordinary path component | Names are joined onto both the `Chips/` input directory and the output directory. `..`, `a/b`, absolute paths, and Windows drive/separator forms are rejected so conversion can never read or write outside the directories it was given. |
| Chip names must be unique | A name listed twice in `AllCustomChipNames` would otherwise collide on output. |
| Boundary-pin ids and sub-chip instance ids must be distinct within a chip | A shared id makes a wire resolve to the wrong endpoint, silently mis-wiring the circuit. |
| Flattening is capped at 50,000 NAND instances and 2,000,000 signal bits | Each level that instantiates its child twice doubles the instance count, so nesting depth alone is not a bound; wide pins multiply node count by their width, so an instance cap alone no longer bounds memory. Both caps sit far above the kernel's component limit, so no compilable circuit is affected. |

The CLI independently re-checks unit names before writing and refuses the whole
run if any destination exists without `--force`, so a failed import never leaves
a partial set of files behind.

## The Logisim profile (Logisim and Logisim Evolution)

Both tools share the `.circ` XML format, so one profile with id `logisim`
handles them; the variant is reported from the file's `source` attribute. A
project is a single `.circ` file, and every `<circuit>` in it becomes one
canonical document.

**Status: experimental.** Port geometry has not been calibrated against real
exports — see `profiles/logisim/samples/README.md`.

### Geometric connectivity

This is the structural difference from DLS. DLS wires name the pin ids they
join; a Logisim wire is only a pair of coordinates and connects to whatever port
shares that point. Conversion therefore:

1. recomputes each component's port positions from its `loc`, `facing`, `size`,
   and input count (`logisim/geometry.rs`);
2. merges every point that touches, including a wire ending part-way along
   another wire, which Logisim treats as a junction;
3. reads module ports off `Pin` components — a pin that drives the sheet is a
   module input, one that reads it is a module output.

Because the geometry rules are assumptions until real files confirm them, they
live in one small module, each is pinned by a test, and a mis-resolved wire
shows up as an undriven-net error from the kernel rather than as wrong Verilog.

### Supported subset

Single-bit combinational logic: `Pin` and the basic gates (`AND`, `OR`, `XOR`,
`NAND`, `NOR`, `XNOR`, `NOT`, `Buffer`). Logisim gates accept more than two
inputs; those fold into the kernel's 2-input catalog gates, with inverting forms
folded on their non-inverting base and inverted once at the end.

Rejected with a diagnostic naming the component and its coordinate: every other
library (plexers, arithmetic, memory, splitters, tunnels, clocks), multi-bit
signals, and — for now — subcircuit instances, whose port layout depends on the
instance appearance and cannot be reconstructed safely without a reference file.

## Adding a profile

1. Add a module under `crates/jsonrtl-profiles/src/` implementing `Profile`.
2. Register it in `registry()`.
3. Add `profiles/<id>/` with a `profile.toml` manifest, a `README.md` documenting
   the source format and supported subset, and test fixtures.
4. Cover conversion with unit tests and byte-exact golden Verilog.
