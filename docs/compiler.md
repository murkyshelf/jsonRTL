# Phase 3 Compiler Contract

Status: Implemented

## Public entry point

`Kernel::compile_verilog(&CircuitDocument, &CompileOptions)` always performs semantic
validation before normalization. Error diagnostics return neither Verilog nor a
source map. Warnings remain in `CompileResult` and do not block complete output.
`CompileOptions::default()` enables the source map; callers may disable only that
artifact without changing diagnostics or Verilog bytes.

## Normalized IR invariants

The compiler converts only validated documents into `NormalizedCircuit`,
`NormalizedPort`, `NormalizedComponent`, `NormalizedConnection`, and `NormalizedNet`.
Their fields and constructors are private and their accessors are read-only.

- Every component connection resolves to a declared normalized net.
- Widths, required ports, component parameters, drivers, and acyclicity have already
  passed semantic validation.
- Ports, nets, and components are sorted by canonical stable ID.
- `editorMetadata` and raw display names are absent.
- Every generated name is represented by `VerilogIdentifier`.

If an invariant unexpectedly fails after validation, compilation returns the
`INTERNAL_INVARIANT` diagnostic and no partial artifacts.

## Identifier policy

One policy handles module, port, net, and component identifiers:

1. Replace every non-ASCII letter, digit, or underscore with `_`.
2. Use `unnamed` if the result is empty.
3. Prefix `n_` when the first character is a digit.
4. Append `_id` when the result is a Verilog-2001 keyword.
5. Allocate all identifiers in stable-ID order and append `__2`, `__3`, and so on
   for collisions.

The final value is non-empty, begins with an ASCII letter or underscore, contains
only ASCII letters, digits, and underscores, and is not a Verilog-2001 keyword.
Deserialization of `VerilogIdentifier` rechecks these invariants.

## Emission format

The emitter accepts only normalized IR and writes a fixed continuous-assignment
subset of Verilog-2001:

- a module header ordered by port stable ID;
- explicit `input wire`, `output wire`, and internal `wire` declarations;
- `[width-1:0]` ranges only when width is greater than one;
- boundary assignments between module ports and normalized internal nets;
- one continuous assignment per component, including width-qualified binary CONST
  literals;
- fixed spaces, blank lines, and LF line endings.

The supported expressions cover `AND`, `OR`, `XOR`, `XNOR`, `NAND`, `NOR`, `NOT`,
`BUFFER`, and `CONST`. Array order, UI metadata, clocks, timestamps, randomness, and
host tool versions never affect the output.

## Source map

`SourceMapEntry` records an inclusive one-based generated line range, a construct
kind, its canonical `SourceReference`, and the safe generated identifier. Entries are
emitted in generated order for:

- module declarations;
- module-port declarations;
- internal-net declarations;
- module-boundary assignments;
- component assignments.

## Authoritative checks

Pure-Rust tests are the compatibility authority. Checked-in fixtures and byte-exact
goldens cover a minimal AND, half adder, full adder, 8-bit logic, CONST, every V1
component type, sanitization, keyword handling, and collisions. Tests also verify
reorder determinism, warning/error gating, exact source ranges, and panic safety.

Verilator and Yosys may be used as optional smoke checks on the goldens. Those tools
remain outside the core library and their availability does not affect compilation.

## Phase 4 handoff

Phase 4 may consume normalized contracts for pure-Rust four-state simulation, but it
must not weaken Phase 3 validation, naming, determinism, or compiler-output tests.
