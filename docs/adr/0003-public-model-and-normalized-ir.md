# ADR 0003: Separate the Public Model from Normalized IR

- Status: Accepted
- Scope: Core library data flow

## Context

The external document must preserve source identity, versioned fields, and useful
locations for diagnostics. Verilog generation instead needs resolved references,
validated widths, catalog-defined ports, unique sanitized names, and stable ordering.
One structure cannot serve both roles without leaking compiler implementation into
the public contract or allowing unvalidated state into generation.

## Decision

Deserialize canonical JSON into a public model that mirrors the schema. After schema
and semantic validation, explicitly normalize it into a strongly typed IR whose
construction and fields remain private. Read-only IR views are public for integration
and testing, but callers cannot construct invalid IR. The generator accepts only
normalized IR. `editorMetadata` and raw display names are never copied into IR.

## Alternatives considered

- Generate directly from deserialized JSON: rejected because invalid or ambiguous
  graph state could reach emission.
- Expose normalized IR as a writable or serialized input contract: rejected because
  internal optimization and generation needs would become compatibility constraints
  and callers could bypass validation.
- Mutate the public model in place: rejected because it obscures which invariants
  have been established and weakens source diagnostics.

## Consequences

There is an explicit conversion step and some duplicated types. In return, compiler
invariants are encoded, raw input cannot bypass validation, source origins can be
retained deliberately, and the IR can evolve privately.
