# ADR 0002: Canonical JSON Is Separate from UI Documents

- Status: Accepted
- Scope: External contract and client integration

## Context

Editors store positions, colors, selection state, routed geometry, and framework-
specific node shapes. Treating any one editor document as compiler input would make
layout semantically significant and prevent unrelated clients from integrating.

## Decision

Use versioned canonical circuit JSON as the only external logical contract. Each UI
owns an adapter that converts its native document to that contract. Optional
`editorMetadata` is opaque and semantically ignored. Stable canonical IDs, not names,
positions, or array indices, define identity.

## Alternatives considered

- Compile each UI's native format directly: rejected because UI changes would become
  compiler changes.
- Put a universal canvas model in the kernel: rejected because visual concerns are
  not universal logical concerns.
- Infer logic from wire coordinates: rejected because geometry is ambiguous and
  cannot be a safe source of truth.

## Consequences

New clients can integrate without kernel changes, and equivalent circuits compile
identically across UIs. Every UI must implement and test an adapter. Diagnostics use
stable canonical IDs so clients can map results back to visual elements.

