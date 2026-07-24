# Digital Logic Kernel Product Specification

Status: Phase 0 contract, ready for Phase 1 implementation

## Problem statement

Circuit editors commonly couple a visual document model to compilation. That makes
the editor, rather than the circuit, the source of truth and makes compiler behavior
depend on layout details. The Digital Logic Kernel is a headless, UI-independent
library that accepts one canonical, versioned circuit document and produces stable
diagnostics and, in a later phase, deterministic synthesizable Verilog.

The kernel does not understand canvases. A client that stores a native UI document
must translate it through an adapter before calling the kernel.

```text
UI document -- client-owned adapter --> canonical circuit document --> kernel
```

## Target users and integration clients

- Browser and desktop circuit editors
- Command-line workflows and automated tests
- Hosted or local REST services
- Educational tools and third-party design applications
- Future simulation and physical-design orchestration services

All clients use the same canonical document contract. No client receives privileged
compiler behavior based on its UI representation.

## Inputs and outputs

### Input

The external input is a UTF-8 JSON document with this root shape:

```json
{
  "schemaVersion": "1.0",
  "circuit": {
    "id": "stable-circuit-id",
    "name": "Display name",
    "ports": [],
    "components": [],
    "nets": []
  },
  "editorMetadata": {}
}
```

`schemaVersion` and `circuit` are required. `editorMetadata` is optional opaque JSON.
The kernel may preserve it at a transport boundary, but parsing, schema validation,
semantic validation, normalization, diagnostics, source maps, and generated output
must not depend on its contents.

Logical structures use camelCase field names only. Unknown fields are rejected in
the root logical contract and within `circuit`, module ports, components, and nets.
Unknown fields inside `editorMetadata` are accepted.

### Logical object shapes

A module port has:

- `id`: stable string identity
- `name`: display and eventual external signal name
- `direction`: `input` or `output`
- `width`: positive integer bit width
- `netId`: ID of the connected net

A component has:

- `id`: stable string identity
- `name`: display name; it is not identity
- `type`: one of the V1 component types
- `width`: positive integer applied to every logical port on that component
- `connections`: object mapping the component's logical port names to net IDs
- `parameters`: object containing only parameters defined by its catalog entry

A net has:

- `id`: stable string identity
- `name`: display name
- `width`: positive integer bit width

Connectivity is derived from module-port `netId` fields and component
`connections`. Array position is never identity or precedence. A module input is a
driver; a module output is a sink. Component input ports are sinks and component
output ports are drivers.

For a valid V1 circuit, every referenced ID exists, IDs are unique within their
kind, every required logical port is connected exactly once, connected widths are
equal, each net has exactly one driver and at least one sink, and the component graph
is acyclic. There are no implicit casts, slices, concatenations, width extension, or
truncation.

### Outputs by release stage

- Phase 1: typed public document model and structured parse/schema diagnostics
- Phase 2: structured semantic diagnostics
- Phase 3: normalized IR, deterministic synthesizable Verilog-2001, and source map
- Phase 4: equivalent CLI and REST boundaries over the same library

An error-level diagnostic blocks normalization and Verilog generation. A warning is
reported but does not block generation. Invalid input must never yield plausible but
misleading Verilog.

## Versioning policy

`schemaVersion` is a string in `MAJOR.MINOR` form. V1 implementation initially
accepts exactly `1.0`.

- A major version may remove fields, change meaning, or otherwise break clients.
- A minor version may add explicitly optional behavior without reinterpreting an
  existing valid document.
- Unsupported versions fail with a structured diagnostic; they are never guessed or
  silently coerced.
- A document is interpreted only under the version it declares.
- Migration is an explicit client or migration-tool operation. The kernel does not
  silently rewrite a document to a different version.
- Field names are camelCase only in V1; aliases are not accepted.
- Logical output compatibility is defined by canonical behavior, not by preserving
  display names or input array order.

## V1 component catalog

All gate operations are bitwise. Binary and unary component ports have the same
width as the component. Width must be at least one. Component output port `Y` is the
only output for every V1 component.

| Type | Inputs | Output | Parameters | Semantics |
| --- | --- | --- | --- | --- |
| `AND` | `A`, `B` | `Y` | none | `Y = A & B` |
| `OR` | `A`, `B` | `Y` | none | `Y = A | B` |
| `XOR` | `A`, `B` | `Y` | none | `Y = A ^ B` |
| `XNOR` | `A`, `B` | `Y` | none | `Y = ~(A ^ B)` |
| `NAND` | `A`, `B` | `Y` | none | `Y = ~(A & B)` |
| `NOR` | `A`, `B` | `Y` | none | `Y = ~(A | B)` |
| `NOT` | `A` | `Y` | none | `Y = ~A` |
| `BUFFER` | `A` | `Y` | none | `Y = A` |
| `CONST` | none | `Y` | `value` | `Y` is the literal value |

`CONST.parameters.value` is a string of exactly `width` ASCII `0` or `1`
characters, most-significant bit first. A string preserves leading zeroes and avoids
host-language number limits. No V1 component accepts extra logical ports or
parameters. Multi-input gates, literals containing X/Z, and per-port widths are
future contract changes.

## V1 scope

V1 provides a contract for one-bit and multi-bit combinational circuits using the
nine catalog components. It includes:

- Versioned canonical JSON
- Schema and typed public model
- Semantic validation and structured diagnostics
- A separate normalized compiler IR
- Deterministic structural/continuous-assignment Verilog-2001
- Source maps from output constructs to original stable IDs
- A reusable Rust library with CLI and REST wrappers
- Configurable resource limits suitable for untrusted input

## Explicit non-goals

The following are not V1 and must not leak into the V1 public contract:

- Canvas layout, colors, selection, zoom, routing points, or editor history
- Sequential elements, clocks, resets, latches, or state
- Simulation, test vectors, waveform history, or VCD
- Hierarchy or reusable submodules
- Arbitrary user-authored Verilog
- Variable-input gates, slicing, concatenation, or implicit width conversion
- WebAssembly and a visual editor
- Invoking Yosys or LibreLane, choosing a PDK, or producing GDSII

These are future layers or schema versions, not hidden V1 behavior.

## Functional requirements

1. Parse well-formed JSON without panicking.
2. Reject unsupported schema versions and unknown logical fields.
3. Validate references, connectivity, widths, catalog ports, parameters, drivers,
   sinks, and combinational acyclicity.
4. Attach diagnostics to original circuit, component, net, and module-port IDs when
   possible.
5. Prevent normalization and generation when any error diagnostic exists.
6. Normalize valid public documents into a typed, deterministic internal IR.
7. Generate synthesizable Verilog-2001 only from normalized IR.
8. Produce a source map linking generated constructs to canonical IDs.
9. Ensure CLI and REST clients call the same library APIs.
10. Treat logically equivalent reordering and all `editorMetadata` changes as
    semantically irrelevant.

## Non-functional requirements

- Identical logical input produces byte-for-byte identical Verilog.
- Reordering arrays without changing IDs or connectivity does not change Verilog.
- Output contains no timestamp, randomness, absolute path, hash-map iteration order,
  or UI metadata.
- Validation is bounded by configurable limits and uses non-recursive or explicitly
  depth-bounded graph processing.
- Hostile input returns structured failure without panic or partial success.
- Public library code has no dependency on HTTP, CLI, UI, Yosys, LibreLane, or a
  filesystem-specific execution model.
- Diagnostics and output ordering are stable and documented.

## Limits and security assumptions

The kernel treats every document as untrusted. Phase 1 will expose `KernelLimits`.
The provisional defaults are:

| Limit | Default |
| --- | ---: |
| UTF-8 request/document bytes | 1,048,576 |
| Module ports | 256 |
| Components | 10,000 |
| Nets | 20,000 |
| Width of any port/component/net | 4,096 bits |
| ID or name length | 128 Unicode scalar values |
| Parameters per component | 32 |

Limits are checked before expensive work. Deployments may choose stricter values,
but changing limits never changes the meaning of a document that is within both
limit sets. The library does not open files, access the network, spawn processes, or
execute user text. API request-body and timeout limits are additional outer-layer
controls.

## Compatibility rules

- Stable IDs, not names or positions, define identity.
- Display-name changes may change sanitized Verilog identifiers but not connectivity
  or diagnostic identity.
- A deterministic collision resolver handles names that sanitize identically.
- `editorMetadata` never affects logical equality or compiler output.
- Unknown logical fields fail closed; they are not ignored as forward compatibility.
- New component semantics require an explicit schema-version decision.
- The normalized IR is private and may change without changing canonical JSON.
- Generated Verilog formatting becomes release-tested behavior for a supported
  compiler release, but it is not a field in the JSON contract.

## Clarifications and working assumptions

| Question | Working assumption | Implementation impact |
| --- | --- | --- |
| Is this the intended repository, and must existing code be preserved? | Use the supplied `/home/murky-ai/projects/digital-logic_vo.1/v0.1/kernal` directory despite the `kernal` spelling; it was empty and not a Git repository when Phase 0 began. | Phase 1 creates the workspace here unless the owner supplies another location; there is currently no implementation to migrate or preserve. |
| What is the minimum Rust version? | Rust 1.85, enabling Rust 2024 edition, until CI policy is chosen. | Phase 1 records `rust-version` and tests that toolchain. Raising it later requires an explicit support decision. |
| Is REST deployed locally, hosted, or both? | Both; the API crate is environment-neutral. | Configuration and authentication stay outside the core library. |
| Are the resource limits final? | Use the provisional defaults above and make every limit configurable. | Phase 1 aligns schema bounds where practical and tests smaller injected limits. |
| Are aliases such as snake_case accepted? | No; V1 accepts camelCase only. | Parsing fails closed and adapters perform any legacy conversion. |
| What is the repository license and visibility? | Private and all-rights-reserved until the owner selects a license. | Do not add a public license or publish packages in Phase 1. |
| Which PDK is targeted later? | Sky130 is the planning baseline, not a V1 dependency. | Phase 9 must re-confirm installed Yosys/LibreLane/PDK versions before implementation. |
| Are names required and unique? | Names are required, non-empty display strings but need not be unique; IDs are required and unique within their kind. | Sanitization and collision resolution remain deterministic; references never use names. |

## Phase 1 entry criteria

Phase 1 may begin when reviewers agree that:

- UI data and canonical logical data are clearly separated.
- The three logical object shapes and connectivity rules are implementable without a
  visual editor model.
- Every V1 component's ports, width rule, parameters, and Boolean semantics are
  unambiguous.
- V1 non-goals and future layers are explicit.
- Dependency direction, diagnostics, deterministic output, and failure behavior match
  `ARCHITECTURE.md` and the ADRs.
- Remaining choices above are acceptable as working assumptions or are replaced by
  recorded decisions.
