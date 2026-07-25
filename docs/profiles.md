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

- `registry()` lists every profile in the build.
- `profile_by_id(id)` selects one explicitly.
- `detect_profile(path)` auto-selects when the caller does not name one.

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

## Status

| Piece | State |
| --- | --- |
| `Profile` trait, registry, `ProfileError` | landed |
| DLS project detection (`detect`) | landed |
| DLS serde model + project loader | landed |
| DLS flatten-to-NAND elaboration + lowering | landed |
| CLI `import` command | landed |
| `profiles/dls/` manifest, docs, example | landed |
| Multi-bit buses (Phase M) | kernel supports width>1; DLS profile rejects (staged) |
| Hierarchical Verilog emission (Phase H) | staged |
| Sequential / clock (Phase S) | staged |

The kernel already compiles multi-bit (`width > 1`) circuits — see
`tests/golden/eight-bit.v`. The DLS profile does not yet map DLS buses onto that
capability; multi-bit pins are rejected until Phase M.

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

This phase supports **combinational, single-bit** logic with **NAND** as the only
primitive. The following are rejected with a precise `ProfileError`, never
silently skipped:

- any pin with `BitCount != 1` (multi-bit buses — staged Phase M);
- any built-in other than NAND: `CLOCK`, `PULSE`, `KEY`, `3-STATE BUFFER`,
  `BUS-*`, merge/split (`1-4BIT`, …), `7-SEGMENT` / displays, `ROM` / memory;
- combinational cycles or missing/multiple drivers (surfaced by the kernel).

## Adding a profile

1. Add a module under `crates/jsonrtl-profiles/src/` implementing `Profile`.
2. Register it in `registry()`.
3. Add `profiles/<id>/` with a `profile.toml` manifest, a `README.md` documenting
   the source format and supported subset, and test fixtures.
4. Cover conversion with unit tests and byte-exact golden Verilog.
