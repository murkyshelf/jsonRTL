# ADR 0004: Generate Deterministic Verilog-2001 First

- Status: Accepted
- Scope: Initial compiler output

## Context

The first compiler milestone supports a small combinational catalog and must produce
auditable, synthesizable output suitable for golden tests and common synthesis tools.
Input arrays and display names are not reliable ordering or identity mechanisms.

## Decision

Generate a fixed structural/continuous-assignment subset of Verilog-2001 from
normalized IR. Order entities by stable IDs, sanitize names through one deterministic
algorithm, resolve collisions in stable order, and use fixed formatting. Do not emit
timestamps, random suffixes, UI metadata, or raw user text.

## Alternatives considered

- SystemVerilog: valuable later, but unnecessary for the V1 combinational catalog and
  less conservative across toolchains.
- Behavioral procedural blocks: rejected initially because continuous assignments
  express V1 semantics more directly and audibly.
- Preserve input order for readability: rejected because semantically irrelevant
  edits would change output.

## Consequences

Golden files can be byte-for-byte stable and output is straightforward to inspect and
cross-check with Yosys. Formatting changes become intentional release decisions.
Later constructs may require new emission strategies without changing this V1 rule.

