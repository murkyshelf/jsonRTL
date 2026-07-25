# Digital Logic Kernel

A UI-independent Rust kernel for canonical digital-logic documents. The project is
being built phase by phase from the contracts in `PRODUCT_SPEC.md` and
`ARCHITECTURE.md`.

## Current status

Phases 1 through 3 are implemented:

- Canonical JSON Schema v1.0, plus v1.1 sliced connections for multi-bit buses
- Strongly typed public document model
- Stable parse and schema diagnostics
- Central V1 component catalog
- Configurable resource limits
- Semantic electrical and logical validation with stable source-linked diagnostics
- Non-recursive combinational cycle detection
- Validated normalized compiler IR with deterministic, collision-free identifiers
- Synthesizable continuous-assignment Verilog-2001 for every V1 component
- Stable one-based generated-line source maps
- Byte-exact golden coverage for gates, adders, vectors, constants, and sanitization
- CLI and Axum service shells over the reusable core crate

The **format-import profiles** subsystem (`jsonrtl-profiles` crate)
converts third-party project formats into canonical documents. The first profile
imports Sebastian Lague's Digital-Logic-Sim (DLS) and Logisim / Logisim Evolution
(`.circ`): the CLI `import` command reads a project, flattens it to primitive
gates, and compiles one `.v` per chip or circuit. DLS buses are supported and
keep their width, so a 16-bit adder compiles to a module with `[7:0]` ports.
See `docs/profiles.md`.

Simulation and physical-design execution are intentionally not implemented yet.

## Workspace

- `crates/jsonrtl`: transport-free model, parser, validator, IR, and compiler
- `crates/jsonrtl-cli`: command-line shell depending on `jsonrtl`
- `crates/jsonrtl-api`: Axum service shell depending on `jsonrtl`
- `crates/jsonrtl-profiles`: foreign-format import profiles (e.g. DLS)
  depending on `jsonrtl`
- `schemas`: canonical schema and illustrative contract examples
- `tests/fixtures`: parser, semantic-validation, and compiler fixtures
- `tests/golden`: byte-exact Verilog-2001 compiler expectations

## Library example

```rust
use jsonrtl::CircuitDocument;

let input = include_str!("schemas/examples/minimal-and.json");
let document = CircuitDocument::from_json(input)?;
assert_eq!(document.schema_version.as_str(), "1.0");
let report = jsonrtl::Kernel::default().validate(&document);
assert!(!report.has_errors());

let result = jsonrtl::Kernel::default().compile_verilog(
    &document,
    &jsonrtl::CompileOptions::default(),
);
assert!(result.has_output());
# Ok::<(), jsonrtl::ParseError>(())
```

## Checks

```text
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --doc --workspace
```

See `PRODUCT_SPEC.md` for the public contract, `ARCHITECTURE.md` for dependency
direction, `docs/compiler.md` for the Phase 3 compiler contract, and
`docs/diagnostics.md` for diagnostic conventions.
