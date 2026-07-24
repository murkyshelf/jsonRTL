# `logic-kernel` CLI Reference

## Commands

| Command | Description |
|---------|-------------|
| `validate CIRCUIT` | Validate a canonical circuit document against schema and semantic rules. |
| `compile CIRCUIT` | Compile a canonical circuit document to deterministic Verilog-2001. |
| `schema` | Print the canonical circuit JSON Schema v1.0 to stdout. |

## Global Options

| Option | Values | Default |
|--------|--------|---------|
| `--diagnostics` | `human`, `json` | `human` |

Diagnostics are always written to **stderr**. Generated Verilog and schema output go to **stdout**.

## Exit Codes

| Code | Category | Meaning |
|------|----------|---------|
| 0 | success | Document valid / compilation successful / schema printed. |
| 2 | invalid | Document has semantic, schema, or parse errors; CLI argument or usage errors. |
| 3 | io | Input file unreadable, output file unwritable, output exists without `--force`. |
| 4 | internal | Unexpected kernel defect or invariant failure. |

## Compile Options

| Option | Description |
|--------|-------------|
| `--output FILE` | Atomically write Verilog to FILE. Required unless `--stdout`. |
| `--stdout` | Write only generated Verilog to stdout (no diagnostic text). |
| `--force` | Permit replacing an existing `--output` file. Requires `--output`. |

At least one of `--output` or `--stdout` must be provided. `--stdout` and `--output` are mutually exclusive.

## Overwrite Policy

By default, `compile --output` refuses to overwrite an existing file. Pass `--force` to atomically replace it.
The output is written through a temporary file in the same directory and renamed (or hard-linked) on success,
so partial output is never left at the target path on failure. Temporary files are cleaned up on every
error path.

## Default Kernel Limits

| Limit | Default |
|-------|---------|
| Maximum document bytes | 1,048,576 |
| Maximum ports | 256 |
| Maximum components | 10,000 |
| Maximum nets | 20,000 |
| Maximum width (bits) | 4,096 |
| Maximum string length (Unicode scalars) | 128 |
| Maximum parameters per component | 32 |

## Examples

```sh
# Validate a circuit
logic-kernel validate half-adder.json

# Validate with JSON diagnostics
logic-kernel --diagnostics json validate half-adder.json

# Compile to stdout
logic-kernel compile half-adder.json --stdout > half-adder.v

# Compile to file (refuses if file exists)
logic-kernel compile half-adder.json --output half-adder.v

# Compile to file (force overwrite)
logic-kernel compile half-adder.json --output half-adder.v --force

# Print the canonical schema
logic-kernel schema
```
