# Diagnostic Contract

Status: Phase 3 implemented contract

## Purpose

Diagnostics are stable machine-readable results that clients can render in a CLI,
REST response, editor, or test. A human-readable message supplements the stable code;
clients must not parse messages to determine behavior.

Semantic validation is exposed through `Kernel::validate(&CircuitDocument)` and
returns a `ValidationReport`. Validation collects independent problems rather than
stopping at the first semantic error. It suppresses work that would be ambiguous or
unsafe after duplicate identities or resource-limit failures.

## Structure

```json
{
  "code": "NET_MULTIPLE_DRIVERS",
  "severity": "error",
  "message": "Net 'sum-net' has 2 drivers.",
  "source": {
    "circuitId": "half-adder",
    "netId": "sum-net",
    "field": "connectivity"
  },
  "relatedSources": [
    {
      "circuitId": "half-adder",
      "componentId": "xor-1",
      "netId": "sum-net",
      "field": "connections.Y"
    }
  ],
  "help": "Ensure every V1 net has exactly one driver.",
  "orderingKey": "00000000|0|NET_MULTIPLE_DRIVERS|..."
}
```

- `code`: `DiagnosticCode`, with stable uppercase serialized spelling
- `severity`: `error`, `warning`, or `info`
- `message`: current human explanation; wording may improve compatibly
- `source`: narrowest stable source identity
- `relatedSources`: sorted and deduplicated identities needed to explain a relation
- `help`: optional likely correction
- `orderingKey`: deterministic report ordinal plus encoded comparison fields

`SourceReference` contains optional `circuitId`, `componentId`, `netId`, `portId`,
and `field`. Semantic diagnostics never use an array index as their only identity and
never depend on `editorMetadata` coordinates.

## Severity and compilation gating

- `error`: the document cannot safely proceed to normalization or generation.
- `warning`: the document remains compilable but deserves attention.
- `info`: non-blocking information reserved for future checks.

`ValidationReport::has_errors()` is true exactly when at least one error exists.
`errors()` and `warnings()` provide allocation-free filtered iterators. Warnings and
info never block future compilation.

## Semantic diagnostic registry

| Code | Severity | Trigger | Example | Likely fix |
| --- | --- | --- | --- | --- |
| `ID_DUPLICATE_COMPONENT` | error | A component ID occurs more than once. | Two components use `gate-1`. | Assign distinct stable component IDs. |
| `ID_DUPLICATE_NET` | error | A net ID occurs more than once. | Two nets use `net-sum`. | Assign distinct stable net IDs. |
| `ID_DUPLICATE_PORT` | error | A module-port ID occurs more than once. | Two ports use `input-a`. | Assign distinct stable module-port IDs. |
| `NAME_DUPLICATE_PORT` | error | External port names are not unique. | Two inputs are both named `data`. | Rename one external port. |
| `NAME_EMPTY` | error | A circuit, port, component, or net name is empty/whitespace. | Component name is `" "`. | Supply a non-empty display name. |
| `NAME_INVALID` | error | A name contains a control character. | Port name contains a newline. | Remove control characters. |
| `NAME_REQUIRES_SANITIZATION` | warning | A display name is not already an ASCII Verilog identifier. | `data in` becomes `data_in`. | Prefer an ASCII identifier if renaming is undesirable. |
| `NAME_VERILOG_KEYWORD` | warning | A sanitized name is a Verilog-2001 keyword. | Circuit name is `module`. | Choose a non-keyword name. |
| `NAME_SANITIZATION_COLLISION` | warning | Multiple names sanitize to the same identifier in one namespace. | `data-in` and `data in`. | Choose names that remain distinct after sanitization. |
| `NET_UNKNOWN_REFERENCE` | error | A module port or recognized component port references an undeclared net ID. | `connections.A` is `missing`. | Reference a declared net. |
| `COMPONENT_MISSING_CONNECTION` | error | A catalog-required logical port is absent. | AND has no `B`. | Connect every required catalog port. |
| `COMPONENT_UNKNOWN_CONNECTION` | error | A connection name is not in that component's catalog entry. | AND has connection `Q`. | Remove it or use `A`, `B`, or `Y`. |
| `COMPONENT_UNKNOWN_TYPE` | error | A typed model bypassing schema contains a type outside V1. | Defensive `Unknown` component type. | Use a canonical V1 component type. |
| `COMPONENT_MISSING_PARAMETER` | error | A required catalog parameter is absent. | CONST has no `value`. | Supply the required parameter. |
| `COMPONENT_UNKNOWN_PARAMETER` | error | A parameter is not allowed by the catalog entry. | AND has `delay`. | Remove the unsupported parameter. |
| `WIDTH_ZERO` | error | A typed model bypassing schema uses width zero. | Net width is `0`. | Use a positive width. |
| `WIDTH_EXCEEDS_LIMIT` | error | A port, component, or net exceeds `KernelLimits.max_width`. | Width is 4097 under default limits. | Reduce width or change trusted limits. |
| `WIDTH_PORT_NET_MISMATCH` | error | A module port width differs from its referenced net width. | 8-bit port connects to 1-bit net. | Make widths identical. |
| `WIDTH_COMPONENT_NET_MISMATCH` | error | A recognized component port's net width differs from component width. | 8-bit XOR input uses a 4-bit net. | Make all V1 gate connection widths identical. |
| `CONST_LITERAL_MALFORMED` | error | CONST `value` is not a non-empty string of only `0`/`1`. | `"10x1"` or numeric `3`. | Use an exact binary string. |
| `CONST_VALUE_WIDTH_MISMATCH` | error | A valid binary CONST literal does not contain exactly `width` digits. | `"11"` at width 4. | Include one digit per bit, including leading zeroes. |
| `NET_MULTIPLE_DRIVERS` | error | More than one external input/component output drives a net. | Input and CONST both drive `net-a`. | Leave exactly one driver. |
| `NET_NO_DRIVER` | error | A net has consumers but no driver. Related sources identify external outputs and/or component inputs. | Output port consumes an undriven net. | Add an external input, component output, or CONST driver. |
| `NET_NO_CONSUMERS` | warning | A driven net has no external-output/component-input consumers. This is also the V1 policy for an unused required component output. | Gate output is unobserved. | Connect a useful consumer or remove the driver. |
| `NET_UNUSED` | warning | A declared net has neither drivers nor consumers. | Orphan net declaration. | Remove or connect the net. |
| `GRAPH_COMBINATIONAL_CYCLE` | error | A strongly connected component has multiple nodes or a self-edge. | Buffers feed each other. | Break feedback; V1 is acyclic combinational logic. |
| `LIMIT_PORTS` | error | Module-port count exceeds the configured maximum. | 257 ports under defaults. | Reduce the circuit or raise trusted limits. |
| `LIMIT_COMPONENTS` | error | Component count exceeds the configured maximum. | 10,001 components under defaults. | Reduce the circuit or raise trusted limits. |
| `LIMIT_NETS` | error | Net count exceeds the configured maximum. | 20,001 nets under defaults. | Reduce the circuit or raise trusted limits. |
| `LIMIT_PARAMETERS` | error | A component parameter count exceeds the configured maximum. | 33 parameters under defaults. | Remove parameters or raise trusted limits. |
| `LIMIT_STRING_LENGTH` | error | A name, ID, reference, logical-port key, or parameter key exceeds the configured character limit. | 129-character ID under defaults. | Shorten the string or raise trusted limits. |
| `INTERNAL_INVARIANT` | error | A post-validation compiler invariant unexpectedly fails. This indicates a kernel defect, not user input. | Validated IR cannot resolve a required connection. | Preserve diagnostics and report the reproducible document to maintainers. |

Parser/schema diagnostics retain the Phase 1 families `PARSE_*`, `VERSION_*`, and
`SCHEMA_*`. `ParseError` also carries structured `SchemaDiagnostic` and
`LimitDiagnostic` values before a typed document exists.

## Electrical-role policy

- External inputs and component outputs are drivers.
- External outputs and component inputs are consumers.
- Every consumer net needs at least one driver.
- More than one driver is an error.
- A driven net without consumers is a warning.
- A fully unused declared net is a warning.
- The single `NET_NO_DRIVER` diagnostic uses related sources to distinguish an
  undriven external output from an undriven required component input, avoiding
  redundant cascaded errors for the same electrical fact.

## Cycle strategy

The validator constructs a component dependency graph from recognized input/output
roles on unique nets. It computes finishing order and reverse-graph components using
iterative stacks (Kosaraju's algorithm), never recursive DFS. Nodes and edges use
ordered collections. One `GRAPH_COMBINATIONAL_CYCLE` is emitted per cyclic strongly
connected component, with the lowest component ID as primary source and all remaining
component IDs and internal net IDs as related sources. Self-loops are cyclic.

Cycle analysis is skipped when component or net IDs are duplicated because graph
identity is ambiguous; the duplicate-ID errors are the useful root cause.

## Ordering and serialization

Before serialization, diagnostics are sorted by:

1. severity (`error`, `warning`, `info`)
2. stable code spelling
3. circuit ID
4. component ID
5. net ID
6. module-port ID
7. field
8. message
9. related sources

Related sources are independently sorted and deduplicated. After sorting, each item
receives a zero-padded report ordinal in `orderingKey`. Input array order, map/hash
iteration, discovery order, and UI metadata never affect the report. Permutation tests
compare both the Rust report and its serialized JSON.

## Resource and cascade policy

Port, component, and net counts are checked before index or graph allocation. A count
violation returns limit diagnostics immediately. Parameter counts are checked next;
violations likewise stop deeper work. This deliberately prioritizes bounded behavior
over collecting more diagnostics from an already over-limit typed model.

Widths are compared numerically and never used to size buffers. Connection inspection
is bounded by the fixed V1 catalog. The parser remains responsible for the raw
document-byte limit because a typed `CircuitDocument` no longer knows its original
serialized size.

## Boundary behavior

The library returns structured diagnostics. The CLI may render them as text or JSON
and use a nonzero exit code for errors. The REST API may serialize them into an HTTP
response. Those full validation endpoints remain Phase 4 work; boundaries must
preserve codes, severities, sources, related sources, help, and ordering.
