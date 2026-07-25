# Profiles + DLS Import + CLI `import` Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax.

**Goal:** Compile foreign digital-logic project formats to Verilog by converting them to canonical circuit JSON v1.0 and reusing the existing kernel compiler; first profile = Sebastian Lague Digital-Logic-Sim (DLS).

**Architecture:** New `jsonrtl-profiles` library (a `Profile` trait + registry + a `dls` module) that flattens a hierarchical DLS project down to NAND primitives, lowers to canonical `CircuitDocument`s, and hands them to `Kernel::compile_verilog`. A new CLI `import` subcommand drives it, emitting one `.v` per chip. Dependency direction is inward: `profiles → jsonrtl`.

**Tech Stack:** Rust (workspace), serde/serde_json, clap (CLI), existing `jsonrtl`.

## Global Constraints

- Canonical schema version: `1.0`. Component types: AND/OR/XOR/XNOR/NAND/NOR/NOT/BUFFER/CONST. NAND ports: `A`,`B` in, `Y` out; uniform width.
- The core `jsonrtl` crate gains **no** dependency on profiles/DLS.
- DLS subset this phase: **combinational, single-bit, primitive = NAND only.** Anything else (BitCount≠1, CLOCK/PULSE/KEY/3-STATE/BUS/merge-split/display/ROM) → precise diagnostic + stop. No silent skipping.
- No panics on untrusted input: profile + CLI return errors/diagnostics.
- Determinism: derive stable canonical IDs from DLS integer IDs; sort before emit.
- Commit after each task.

---

### Task 1: Scaffold `jsonrtl-profiles` crate + `Profile` trait

**Files:**
- Create: `crates/jsonrtl-profiles/Cargo.toml`
- Create: `crates/jsonrtl-profiles/src/lib.rs` (Profile trait, registry, ProfileError, ProjectConversion, NamedCircuit)
- Modify: `Cargo.toml` (workspace members)
- Test: inline `#[cfg(test)]` in lib.rs

**Interfaces produced:**
```rust
pub struct NamedCircuit { pub name: String, pub document: CircuitDocument }
pub struct ProjectConversion { pub project_name: String, pub circuits: Vec<NamedCircuit> }
pub enum ProfileError { /* Io, Parse{msg}, Unsupported{chip,detail}, Structure{chip,detail} */ }
pub trait Profile {
    fn id(&self) -> &'static str;
    fn detect(&self, path: &std::path::Path) -> bool;
    fn convert(&self, path: &std::path::Path) -> Result<ProjectConversion, ProfileError>;
}
pub fn registry() -> Vec<Box<dyn Profile>>;
pub fn detect_profile(path: &std::path::Path) -> Option<Box<dyn Profile>>;
```

- [ ] Write failing test: `registry()` contains a profile with id `"dls"`; `ProfileError` Display renders chip+detail.
- [ ] Run → fails to compile (types absent).
- [ ] Implement trait, error enum (`thiserror`-free, hand-rolled Display), empty-ish registry returning `DlsProfile` (stub `convert` = `unimplemented` behind a later task, but `id`/`detect` real).
- [ ] Run tests → pass. Commit.

### Task 2: DLS serde model + project loader

**Files:**
- Create: `crates/jsonrtl-profiles/src/dls/model.rs` (serde structs)
- Create: `crates/jsonrtl-profiles/src/dls/mod.rs`
- Test: fixtures under `crates/jsonrtl-profiles/tests/fixtures/dls/test/` (copied from the real bundled project)

**Interfaces produced:**
```rust
pub struct DlsProject { pub name: String, pub chip_names: Vec<String>, pub chips: BTreeMap<String, ChipDef> }
pub struct ChipDef { pub name: String, pub input_pins: Vec<PinDef>, pub output_pins: Vec<PinDef>, pub sub_chips: Vec<SubChip>, pub wires: Vec<Wire> }
pub struct PinDef { pub name: String, pub id: i64, pub bit_count: u32 }
pub struct SubChip { pub name: String, pub id: i64 }
pub struct Wire { pub source: PinAddress, pub target: PinAddress }
pub struct PinAddress { pub pin_id: i64, pub pin_owner_id: i64 }
pub fn load_project(dir: &Path) -> Result<DlsProject, ProfileError>;
```
(serde uses `#[serde(rename_all = "PascalCase")]` matching DLS keys; ignore unknown fields.)

- [ ] Copy the bundled DLS `test` project into the fixtures dir.
- [ ] Write failing test: `load_project(fixture)` → project name "test", 5 chips, AND chip has 2 input pins + 1 output pin + 2 NAND subchips + 5 wires.
- [ ] Run → fail.
- [ ] Implement structs + `load_project` (read `ProjectDescription.json` + each `Chips/*.json`).
- [ ] Run → pass. Commit.

### Task 3: Elaboration engine — flatten hierarchy to NAND netlist

**Files:**
- Create: `crates/jsonrtl-profiles/src/dls/elaborate.rs`
- Test: inline tests + uses fixtures.

**Interfaces produced:**
```rust
// Flat result for one top chip.
pub struct FlatNetlist {
    pub inputs: Vec<BoundaryPin>,   // name + net index
    pub outputs: Vec<BoundaryPin>,
    pub nands: Vec<NandInst>,       // a,b,y net indices
    pub net_count: usize,
}
pub fn elaborate(project: &DlsProject, chip: &str) -> Result<FlatNetlist, ProfileError>;
```

Algorithm: union-find over endpoint keys. Endpoint key = `(instance_path, pin_id)`. Built-in NAND primitive: pins 0,1 in / 2 out. Recursively inline custom subchips with a fresh instance-path prefix; stitch parent wire endpoints referencing subchip boundary pin IDs to the child copy's boundary pins. Reject non-NAND built-ins and BitCount≠1 with `ProfileError::Unsupported`. Detect chip-reference cycles + cap recursion depth (`ProfileError::Structure`).

- [ ] Write failing test: `elaborate(project, "AND")` → 2 NAND instances, 3 nets (2 inputs + 1 output), correct a/b/y wiring (NAND1 fed by both inputs; NAND2 fed twice by NAND1.Y; output = NAND2.Y).
- [ ] Run → fail.
- [ ] Implement union-find + pin resolution + recursive inline.
- [ ] Run → pass. Add test: `elaborate(project, "1-bit adder")` succeeds (all subchips resolve to NAND, single-driver per net). Commit.

### Task 4: Lower flat netlist → canonical `CircuitDocument`

**Files:**
- Create: `crates/jsonrtl-profiles/src/dls/lower.rs`
- Test: integration test that also compiles via the kernel.

**Interfaces produced:**
```rust
pub fn lower(chip_name: &str, flat: &FlatNetlist) -> CircuitDocument;
```
Each net index → a canonical `Net` (`n{idx}`, width 1). Each NAND → `Component{type:Nand, connections:{A,B,Y}}`. Each input `BoundaryPin` → input `ModulePort` bound to its net; each output → output `ModulePort` bound to its net. Insert a `BUFFER` when an output net's driver is a module input (pass-through). Sanitize chip name → module id/name.

- [ ] Write failing test: `lower("AND", flat)` → doc validates clean AND compiles: `Kernel::default().compile_verilog(&doc, default)` has output and zero error diagnostics; Verilog contains two `~(... & ...)` NAND assigns.
- [ ] Run → fail.
- [ ] Implement lowering.
- [ ] Run → pass. Commit.

### Task 5: Wire `DlsProfile::convert` + rejection diagnostics

**Files:**
- Modify: `crates/jsonrtl-profiles/src/dls/mod.rs`
- Test: inline + an unsupported fixture (a hand-made chip with a CLOCK subchip and a multi-bit pin).

**Interfaces:** consumes `load_project`, `elaborate`, `lower`. `DlsProfile::convert` = load → for each chip name: elaborate+lower → collect `NamedCircuit`; `detect` = dir has `ProjectDescription.json` + `Chips/`.

- [ ] Write failing test: `DlsProfile.convert(fixture)` → `ProjectConversion` with 5 named circuits, each compiling clean. Add: unsupported fixture → `ProfileError::Unsupported` naming the chip + `CLOCK`/BitCount.
- [ ] Run → fail.
- [ ] Implement.
- [ ] Run → pass. Commit.

### Task 6: Golden — canonical JSON + Verilog for the whole project

**Files:**
- Create: `crates/jsonrtl-profiles/tests/golden.rs`
- Create golden outputs under `crates/jsonrtl-profiles/tests/golden/` (`AND.v`, `OR.v`, `XOR.v`, `NOT.v`, `1-bit-adder.v`).

- [ ] Write test that converts the fixture project, compiles each chip, and asserts byte-equality against committed golden `.v` (regenerate-on-first-run guard documented).
- [ ] Run → generate + eyeball each `.v` for correctness (AND = 2 NAND; adder sum/carry structure). Commit golden + test.

### Task 7: CLI `import` subcommand

**Files:**
- Modify: `crates/jsonrtl-cli/Cargo.toml` (dep on `jsonrtl-profiles`)
- Modify: `crates/jsonrtl-cli/src/main.rs` (add `Import` command + handler)
- Test: `crates/jsonrtl-cli/tests/import.rs`

CLI: `import <dir> [--profile id] [--out DIR] [--chip NAME] [--stdout] [--emit-canonical DIR] [--force]`. Default → all chips to `<out>/<ChipName>.v` via existing atomic write; `--chip X --stdout` prints one module; `--emit-canonical` writes canonical JSON per chip; auto-detect profile when omitted; reuse `--diagnostics human|json` + exit codes (2 invalid, 3 io, 4 internal).

- [ ] Write failing integration test: run binary `import <fixture> --out tmp` → creates `AND.v`,`OR.v`,`XOR.v`,`NOT.v`,`1-bit adder.v`; `--chip AND --stdout` stdout contains `module AND`; unsupported fixture → nonzero exit + diagnostic.
- [ ] Run → fail.
- [ ] Implement command + handler.
- [ ] Run → pass. Commit.

### Task 8: `profiles/dls/` folder (manifest, docs, fixtures)

**Files:**
- Create: `profiles/dls/profile.toml` (id, name, source-format, supported subset, entry detection)
- Create: `profiles/dls/README.md` (format decode, mapping rules, limitations)
- Create: `profiles/dls/fixtures/test/...` (reference project; may symlink/copy crate fixture)

- [ ] Write manifest + README documenting the pin-address model + rejection list.
- [ ] Commit.

### Task 9: Production hardening

**Files:**
- Create: `.github/workflows/ci.yml` (fmt-check, clippy -D warnings, test, build --release)
- Modify: root `Cargo.toml` (release profile, workspace version), add `CHANGELOG.md`
- Create: `docs/profiles.md`; Modify: `docs/cli.md` (+import), `README.md`
- Audit: grep for `unwrap(`/`expect(`/`panic!` in profiles + cli new code; replace on input paths with diagnostics.

- [ ] Add CI workflow. Run `cargo fmt --check`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo test --workspace` locally → all green.
- [ ] Write CHANGELOG + docs.
- [ ] Commit.

### Task 10: Multi-bit audit (Phase M entry, lightweight)

- [ ] Confirm `tests/golden/eight-bit.v` still builds via `cargo test` and document in `docs/profiles.md` that width>1 is supported by the kernel but not yet by the DLS profile (staged). Commit.

## Self-Review

- **Spec coverage:** profiles folder+crate (T1,T8), DLS convert (T2–T6), CLI import (T7), hardening (T9), multi-bit audit (T10) — all spec sections mapped.
- **Placeholder scan:** none; each task has concrete deliverable + test intent + commands.
- **Type consistency:** `Profile`/`ProjectConversion`/`NamedCircuit`/`FlatNetlist`/`elaborate`/`lower`/`load_project` names consistent across T1–T7.
