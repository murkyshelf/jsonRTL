# `jsonrtl` CLI Reference

## Commands

| Command | Description |
|---------|-------------|
| `validate CIRCUIT` | Validate a canonical circuit document against schema and semantic rules. |
| `compile CIRCUIT` | Compile a canonical circuit document to deterministic Verilog-2001. |
| `import PROJECT_DIR` | Import a foreign project (e.g. DLS) and compile each unit to Verilog. |
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

## Import Options

`import` converts a third-party project format into canonical circuit documents
(via a *profile*) and compiles each unit to Verilog. See `docs/profiles.md`.

| Option | Description |
|--------|-------------|
| `--profile ID` | Import profile id (e.g. `dls`). Auto-detected from the directory when omitted. |
| `--out DIR` | Write one `<UnitName>.v` per compiled unit into DIR. Required unless `--stdout`. |
| `--chip NAME` | Restrict to a single unit by name. |
| `--stdout` | Write a single unit's Verilog to stdout. Requires `--chip`; conflicts with `--out`. |
| `--emit-canonical DIR` | Also write the intermediate canonical JSON per unit into DIR. |
| `--force` | Permit existing output files to be atomically replaced. |

Output files are written with the same atomic, overwrite-protected policy as
`compile`. Conversion failures (unsupported constructs, malformed input) exit
with code `2`; unreadable inputs or unwritable outputs exit with code `3`.

```sh
# Import a DLS project: one <ChipName>.v per chip, mirroring the project.
jsonrtl import profiles/dls/example --out build/verilog

# Print a single chip to stdout.
jsonrtl import profiles/dls/example --chip AND --stdout

# Also emit the canonical JSON handed to the compiler.
jsonrtl import profiles/dls/example --out build/v --emit-canonical build/canon
```

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
jsonrtl validate half-adder.json

# Validate with JSON diagnostics
jsonrtl --diagnostics json validate half-adder.json

# Compile to stdout
jsonrtl compile half-adder.json --stdout > half-adder.v

# Compile to file (refuses if file exists)
jsonrtl compile half-adder.json --output half-adder.v

# Compile to file (force overwrite)
jsonrtl compile half-adder.json --output half-adder.v --force

# Print the canonical schema
jsonrtl schema
```
