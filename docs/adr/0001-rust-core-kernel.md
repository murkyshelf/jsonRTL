# ADR 0001: Use Rust for the Core Kernel

- Status: Accepted
- Scope: Core compiler and future simulator library

## Context

The kernel parses untrusted documents, validates large graphs, provides reusable CLI,
REST, and future WASM boundaries, and must remain deterministic and portable. The
core needs explicit data modeling and predictable resource use without being coupled
to a garbage-collected service runtime.

## Decision

Implement the reusable core in stable Rust. Keep transport and application behavior
in separate crates that depend on the core. Use a declared minimum supported Rust
version and minimal, audited dependencies.

## Alternatives considered

- TypeScript would integrate easily with browser clients but would blur the adapter
  boundary and make a native compiler/service core less predictable.
- Python would accelerate prototyping but is a weaker fit for a bounded reusable
  native library and browser compilation target.
- C++ offers native control but has a larger memory-safety and dependency-management
  burden for untrusted parsing.

## Consequences

Rust types and ownership help enforce validated state transitions and memory safety.
The same library can serve CLI, API, and future WASM wrappers. Contributors must
maintain Rust toolchain compatibility; clients still communicate through canonical
JSON and do not need to be written in Rust.

