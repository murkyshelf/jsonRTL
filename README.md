# Digital Logic Kernel

A UI-independent Rust kernel for canonical digital-logic documents. The project is
being built phase by phase from the contracts in `PRODUCT_SPEC.md` and
`ARCHITECTURE.md`.

## Current status

Phases 1 through 3 are implemented:

- Canonical JSON Schema v1.0
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

The **format-import profiles** subsystem (`logic-kernel-profiles` crate)
converts third-party project formats into canonical documents. The first profile
imports Sebastian Lague's Digital-Logic-Sim (DLS): the CLI `import` command reads
a project directory, flattens each hierarchical chip to NAND primitives, and
compiles one `.v` per chip. See `docs/profiles.md`.

Simulation and physical-design execution are intentionally not implemented yet.

## Workspace

- `crates/logic-kernel`: transport-free model, parser, validator, IR, and compiler
- `crates/logic-kernel-cli`: command-line shell depending on `logic-kernel`
- `crates/logic-kernel-api`: Axum service shell depending on `logic-kernel`
- `crates/logic-kernel-profiles`: foreign-format import profiles (e.g. DLS)
  depending on `logic-kernel`
- `schemas`: canonical schema and illustrative contract examples
- `tests/fixtures`: parser, semantic-validation, and compiler fixtures
- `tests/golden`: byte-exact Verilog-2001 compiler expectations

## Library example

```rust
use logic_kernel::CircuitDocument;

let input = include_str!("schemas/examples/minimal-and.json");
let document = CircuitDocument::from_json(input)?;
assert_eq!(document.schema_version.as_str(), "1.0");
let report = logic_kernel::Kernel::default().validate(&document);
assert!(!report.has_errors());

let result = logic_kernel::Kernel::default().compile_verilog(
    &document,
    &logic_kernel::CompileOptions::default(),
);
assert!(result.has_output());
# Ok::<(), logic_kernel::ParseError>(())
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
