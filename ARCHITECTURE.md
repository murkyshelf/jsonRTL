# Digital Logic Kernel Architecture

Status: Phase 0 architecture contract

## System boundary

The kernel owns the canonical circuit contract and logical compilation behavior. It
does not own any client's native UI document. An adapter at the client boundary maps
native UI state into canonical JSON.

```text
Browser UI -----\
Desktop UI ------> client-owned adapter --> canonical JSON
Third-party tool /                              |
                                                v
                                    reusable jsonrtl library
                                      ^                    ^
                                      |                    |
                                     CLI               REST API
```

Positions, colors, selection state, zoom, routed wire geometry, and undo history may
live in the UI document or in optional `editorMetadata`. They never enter logical
validation, normalized IR, generation, or simulation semantics.

## Planned repository boundaries

```text
jsonrtl/
├── crates/
│   ├── jsonrtl/          public model, validation, IR, compiler; reusable library
│   ├── jsonrtl-cli/      local process boundary; depends on jsonrtl
│   ├── jsonrtl-api/      Axum transport boundary; depends on jsonrtl
│   └── jsonrtl-profiles/ foreign-format import profiles; depends on jsonrtl
├── profiles/
│   └── dls/                   per-profile manifest, docs, and fixtures
├── schemas/
│   ├── circuit-v1.schema.json
│   └── examples/
├── docs/
│   ├── adr/
│   └── diagnostics.md
├── tests/
│   └── fixtures/
├── PRODUCT_SPEC.md
└── ARCHITECTURE.md
```

The Phase 0 repository contains only specification artifacts. Crates, executable
code, the schema implementation, and test fixtures begin in Phase 1.

## Dependency direction

Arrows mean “depends on.” Every application-facing dependency points inward toward
the library.

```text
UI application --> UI adapter --> canonical JSON schema/model
CLI ---------------------------------------> jsonrtl
REST API ----------------------------------> jsonrtl
import profiles ---------------------------> jsonrtl
future WASM wrapper -----------------------> jsonrtl
future simulator extension ---------------> normalized kernel contracts
future physical-design orchestrator ------> compiler artifacts

jsonrtl -X-> CLI / REST / profiles / UI / Axum / Tokio / Clap / Yosys / LibreLane
```

Import profiles are the server-side analogue of a UI adapter: they map a foreign
project format onto canonical JSON. Like a UI adapter, a profile may not ask the
kernel to infer connectivity from coordinates or reinterpret layout as logic; it
produces canonical documents the kernel validates and compiles unchanged.

The core library may depend on narrowly scoped parsing, schema, serialization, error,
and data-structure crates. It may not call a transport, filesystem-specific service,
external tool, or frontend framework.

## Compiler pipeline

```text
bytes
  |
  v
JSON parse
  | malformed JSON / size diagnostics
  v
version + JSON Schema validation
  | schema diagnostics
  v
typed public CircuitDocument
  |
  v
semantic validation
  | reference / catalog / width / graph diagnostics
  v
normalization
  |
  v
private typed normalized IR
  |                 \
  v                  -> source-map builder
deterministic Verilog-2001
  |
  v
Verilog text + source map + warnings
```

Each stage consumes the successful output of the previous stage. Error diagnostics
stop all later stages. Warnings accompany a successful result. Verilog generation
has no code path from raw JSON or the public document model; it accepts only validated
normalized IR.

## Public model and normalized IR

The public model mirrors the versioned JSON contract and preserves stable source
identity. It is designed for validation and useful diagnostics, not convenient code
generation.

The normalized IR is private, strongly typed, validated, and generation-oriented. It
uses resolved references, catalog-defined ports, explicit widths, sanitized unique
identifiers, and stable ordering. It contains no `editorMetadata`. Changes to the IR
do not change the external contract unless observable behavior changes.

Keeping the two models separate prevents deserialization convenience, JSON ordering,
optional transport fields, or future schema compatibility fields from becoming
compiler invariants.

## Determinism rules

- Sort externally observable entities by stable canonical IDs before normalization
  and emission.
- Never use input array order as semantic priority.
- Use ordered collections where iteration affects diagnostics, IR, source maps, or
  output.
- Sanitize names through one documented function and resolve collisions by stable ID
  order.
- Emit a fixed Verilog-2001 style with fixed whitespace and line endings.
- Never include timestamps, randomness, memory addresses, absolute paths, or UI data.
- Sort diagnostics by a documented tuple rather than discovery order.

Equivalent logical documents that differ only in array order or `editorMetadata`
must generate byte-for-byte identical Verilog and equivalent diagnostics.

## Diagnostic flow

All stages report the common diagnostic envelope specified in
`docs/diagnostics.md`. A diagnostic carries stable code, severity, message, primary
source reference, optional related references, and optional notes.

Source references progressively improve:

1. Parsing can identify byte/line/column and a best-effort JSON path.
2. Schema validation identifies a JSON path and stable ID when already readable.
3. Semantic validation identifies circuit, component, net, and module-port IDs.
4. Normalization and generation retain origin IDs on every relevant IR item.
5. The source map associates emitted module, signal, and assignment ranges with those
   origin IDs.

Diagnostics are data returned by the library. The CLI decides text rendering and exit
codes; the REST boundary decides HTTP representation. Neither boundary invents
compiler results.

## UI adapter boundary

An adapter is owned and versioned by its client. It may:

- Map UI node and wire records to components and nets
- Generate or persist stable canonical IDs
- Convert editor-specific gate names to catalog types
- Put semantically ignored UI state in `editorMetadata`
- Surface kernel diagnostics back on visual elements using stable IDs

An adapter may not ask the kernel to infer connectivity from coordinates, silently
fix invalid widths, or interpret layout as logic. A second unrelated UI must be able
to produce the same canonical circuit without kernel changes.

## Source maps

Phase 3 defines the serialized source-map format. Architecturally, generation records
an origin set for each emitted module port, internal signal, literal, and assignment.
Origins use canonical stable IDs and logical port names, never UI coordinates. Source
maps are compiler artifacts; they do not feed back into compilation.

## Future extension points

- A simulation engine reuses validated circuit semantics and stable IDs but has its
  own event/state model. Four-state logic is not smuggled into V1 `CONST` semantics.
- Sequential and hierarchical schema versions add explicit public constructs and new
  normalized IR variants.
- A WASM wrapper depends on the library without moving browser behavior into it.
- A physical-design orchestrator consumes generated RTL and configuration, invokes
  Yosys/LibreLane, and tracks artifacts outside the core library.
- Testbench, VCD, synthesis reports, and GDSII are separate artifact types.

## Failure modes and containment

| Failure | Required behavior |
| --- | --- |
| Document exceeds configured byte limit | Reject before full parsing where the boundary can measure bytes. |
| Malformed JSON | Return structured parse diagnostic; never panic. |
| Unsupported schema version | Return version diagnostic; do not guess or migrate. |
| Unknown logical field | Return schema diagnostic; do not ignore it. |
| Missing/duplicate/dangling ID | Return semantic diagnostic with related sources where possible. |
| Port, parameter, or width mismatch | Return catalog/width diagnostic and block compilation. |
| Multiple or missing net drivers | Return connectivity diagnostic and block compilation. |
| Combinational cycle | Return cycle diagnostic with participating component/net IDs. |
| Resource count or width exceeds a limit | Stop bounded processing and return a limit diagnostic. |
| Identifier collision after sanitization | Resolve deterministically; warn only if the public policy calls for it. |
| Internal invariant fails after validation | Return a distinct internal error at the boundary; never emit partial Verilog. |
| External Yosys/LibreLane failure | Future orchestrator records logs/artifacts; core kernel remains unaffected. |

## Security posture

Input is untrusted. Algorithms are bounded by `KernelLimits`; graph traversal avoids
unbounded recursion; serialization and diagnostic sizes are bounded; user names are
never copied into Verilog without sanitization. The core library performs no network
access, process execution, template evaluation, or arbitrary code execution.

## Architecture conformance gate

- The canonical document is understandable without a UI model.
- Application crates depend on `jsonrtl`; the reverse dependency is forbidden.
- Raw JSON cannot reach the generator without all validation and normalization stages.
- The normalized IR is private and contains no UI metadata.
- Simulation and physical design are future consumers/layers, not V1 dependencies.
- Every observable ordering decision has a deterministic rule.

