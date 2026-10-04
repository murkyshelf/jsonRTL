# ADR 0005: Keep Yosys and LibreLane Outside the Core Library

- Status: Accepted
- Scope: Synthesis and physical-design integration

## Context

Yosys and LibreLane involve process execution, tool versions, PDK files, platform-
specific setup, long-running jobs, logs, reports, and large artifacts. Those concerns
are unrelated to parsing, validation, normalization, and deterministic RTL emission.

## Decision

Keep tool execution outside the core library. The CLI owns current Yosys synthesis,
Icarus checking/simulation, and DLS export, using a shared bounded process runner.
Future physical-design execution belongs in an orchestration crate or service. The
core library produces deterministic Verilog and source maps only. An orchestrator
accepts those artifacts plus explicit tool/PDK configuration and records all reports,
logs, and outputs.

## Alternatives considered

- Spawn tools from the core library: rejected because it adds filesystem, process,
  timeout, and platform behavior to every consumer.
- Treat command output as compiler diagnostics: rejected because tool failures and
  canonical-circuit errors have different ownership and lifecycles.
- Put generated PDK configuration in canonical JSON: rejected because physical setup
  is not V1 circuit semantics.

## Consequences

The core stays portable, testable, and suitable for WASM. Physical flows require an
additional service and explicit artifact protocol. Tool versions, PDK selection,
timeouts, and job isolation can evolve without contaminating the circuit contract.
