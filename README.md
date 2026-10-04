# jsonRTL

**A Rust circuit kernel with circuit JSON compilation, DLS/Logisim import, Verilog-to-DLS export, and Yosys/Icarus CLI tools.**

[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.85%2B-orange.svg)](https://www.rust-lang.org)
[![Schema](https://img.shields.io/badge/schema-v1.1-informational.svg)](schemas/circuit-v1.schema.json)
[![Verilog](https://img.shields.io/badge/output-Verilog--2001-success.svg)](docs/compiler.md)

<!-- Once this repo has a GitHub remote, add the CI badge:
[![CI](https://github.com/<owner>/<repo>/actions/workflows/ci.yml/badge.svg)](https://github.com/<owner>/<repo>/actions/workflows/ci.yml)
-->

---

Describe a circuit as JSON. Get Verilog that a synthesizer accepts.

<table>
<tr><th>Canonical circuit JSON</th><th>Generated Verilog-2001</th></tr>
<tr><td>

```json
{
  "schemaVersion": "1.1",
  "circuit": {
    "id": "minimal-and",
    "name": "Minimal AND",
    "ports": [
      { "id": "in-a", "name": "a", "direction": "input",
        "width": 1, "netId": "net-a" },
      { "id": "in-b", "name": "b", "direction": "input",
        "width": 1, "netId": "net-b" },
      { "id": "out-y", "name": "y", "direction": "output",
        "width": 1, "netId": "net-y" }
    ],
    "components": [
      { "id": "and-1", "name": "and_gate",
        "type": "AND", "width": 1,
        "connections": { "A": "net-a", "B": "net-b",
                         "Y": "net-y" },
        "parameters": {} }
    ],
    "nets": [
      { "id": "net-a", "name": "a", "width": 1 },
      { "id": "net-b", "name": "b", "width": 1 },
      { "id": "net-y", "name": "y", "width": 1 }
    ]
  }
}
```

</td><td>

```verilog
module Minimal_AND (
    a,
    b,
    y
);

input wire a;
input wire b;
output wire y;

wire a__2;
wire b__2;
wire y__2;

assign a__2 = a;
assign b__2 = b;
assign y = y__2;

assign y__2 = a__2 & b__2;

endmodule
```

</td></tr>
</table>

## Why

Circuit editors, simulators, and teaching tools each invent their own project
format, and each one eventually wants real hardware output. jsonRTL is the piece
in the middle: **one canonical document contract**, a kernel that validates it
strictly, and a compiler that emits the same bytes every time.

- **Deterministic.** Same input, same output, byte for byte. No timestamps, no
  hash-order, no randomness. Golden tests pin it.
- **Strict, and specific about it.** 35 diagnostic codes with stable machine
  readable IDs, source-linked back to the offending element — not a stack trace.
- **UI-independent.** The core crate knows nothing about editors, transports, or
  foreign formats. Dependencies point strictly inward.
- **Honest about limits.** Anything outside the supported subset is rejected with
  a diagnostic naming it. Nothing is silently skipped or guessed.

## Install

```sh
cargo install --path crates/jsonrtl-cli --locked
```

Installs the `jsonrtl` binary into `$CARGO_HOME/bin` (usually `~/.cargo/bin`),
which must be on your `PATH`. Install elsewhere with `--root ~/.local`.

> The crate is `publish = false`, so `--path` is the only route — there is no
> registry release. For development, `cargo run -p jsonrtl-cli -- <args>` needs
> no install and always rebuilds first.

**Requires** Rust 1.85+ (edition 2024).

## Quick start

```sh
jsonrtl profiles                        # what foreign formats can be imported?
jsonrtl tools                           # installed Yosys, Icarus, and VVP versions
jsonrtl check top.v --top top            # check with both HDL compilers
jsonrtl compile circuit.json --stdout   # canonical JSON -> Verilog
jsonrtl export top.v --top top --out build/top  # Verilog -> Digital-Logic-Sim
jsonrtl validate circuit.json           # just check it
jsonrtl schema                          # print the JSON Schema
```

### Importing from another tool

`import` reads a foreign project, converts it to canonical documents, and
compiles each unit — in one step. Point it at the project **directory**; the
profile is auto-detected.

```sh
# One chip to the terminal.
jsonrtl import ~/path/to/DLS/Projects/test --chip AND --stdout

# A whole project: one .v per chip.
jsonrtl import ~/path/to/DLS/Projects/test --out build/

# Emit everything that compiles; report the rest and still exit non-zero.
jsonrtl import ~/path/to/DLS/Projects/test --out build/ --skip-unsupported
```

A Digital-Logic-Sim `AND` chip is built from NAND gates, and that is exactly what
comes out:

```verilog
module AND ( IN, IN_2, OUT );
...
assign net3 = ~(net0 & net1);
assign net2 = ~(net3 & net3);
endmodule
```

Buses survive the trip. A 16-bit adder imports as a module with real bus ports —
`input wire [7:0] A0;` — not 34 scalar wires.

### Exporting Verilog to Digital-Logic-Sim

Install **Yosys with ABC** for this command (on Debian/Ubuntu: `sudo apt install
yosys`). The exporter creates a native DLS 2.1.6 project with NAND logic, bus
converters, and reusable register/latch chips. It supports rising/falling
registers, enables, and synchronous/asynchronous resets.

```sh
# Keep the clock as a manually controlled input.
jsonrtl export top.v --top top --profile dls --out build/top

# Connect the clk input to DLS's native CLOCK in an additional wrapper chip.
jsonrtl export examples/verilog/counter.v --top counter --out build/counter --clock clk
```

Copy the complete generated directory into DLS's `Projects` save directory,
restart DLS, select the project, and open the chip named by the command. The
directory contains `ProjectDescription.json` and `Chips/*.json`; existing
directories are refused. Wider ports use labeled 8/4/1-bit chunks. See the
[CLI reference](docs/cli.md#export-options) for save locations, limits, and
unsupported HDL constructs. The [upstream simulator verification
suite](scripts/dls-verification/README.md) compares exported behavior with Icarus.

### Checking, synthesis, and test-bench simulation

Install Yosys and Icarus Verilog (on Debian/Ubuntu: `sudo apt install yosys
iverilog`). Icarus includes the `vvp` runtime. The bundled counter has a test
bench that checks counting, enable hold, and asynchronous reset:

```sh
jsonrtl tools
jsonrtl check examples/verilog/counter.v --top counter
jsonrtl synth examples/verilog/counter.v --top counter --out build/counter-synth
jsonrtl simulate examples/verilog/counter.v examples/verilog/counter_tb.v \
  --top counter_tb --out build/counter-sim
# Run the same assertions against the synthesized gate netlist.
jsonrtl simulate build/counter-synth/netlist.v examples/verilog/counter_tb.v \
  --top counter_tb --out build/counter-netlist-sim
```

Synthesis saves `netlist.v`, `netlist.json`, `statistics.json`, and `report.log`.
Simulation saves the compiled program, compiler/runtime logs, and test-bench
waveforms such as `counter.vcd`. Read `stdout.txt` for the assertion result;
`$fatal`, `$stop`, and deadlines produce failures without publishing a result
directory. Each `--out` must be new. Use `--tool iverilog` to check test benches,
and `--system-verilog` to enable the installed compiler's SystemVerilog subset.

All HDL commands use a shared bounded process runner; see the [CLI
guide](docs/cli.md#hdl-tool-options) for executable overrides, plus arguments,
resource limits, and the scope of trusted local execution.

## Import profiles

| Profile | Source tool | Input | Status |
|---------|-------------|-------|--------|
| `dls` | Sebastian Lague's [Digital-Logic-Sim](https://github.com/SebLague/Digital-Logic-Sim) | project directory | **stable** |
| `logisim` | Logisim and Logisim Evolution | a single `.circ` file | experimental |

`dls` handles combinational logic of any width: chips flatten to NAND, and DLS's
bus splitters and mergers become canonical bit slices that emit no logic at all.

`logisim` is marked experimental because Logisim connectivity is *geometric* —
a wire is a pair of coordinates — and the port-geometry rules were written from
the format rather than calibrated against real exports.

Adding a profile means implementing one trait; see [`docs/profiles.md`](docs/profiles.md).

## Using it as a library

```rust
use jsonrtl::CircuitDocument;

let input = include_str!("schemas/examples/minimal-and.json");
let document = CircuitDocument::from_json(input)?;
assert_eq!(document.schema_version.as_str(), "1.0");
let report = jsonrtl::Kernel::default().validate(&document);
assert!(!report.has_errors());

let result = jsonrtl::Kernel::default().compile_verilog(
    &document,
    &jsonrtl::CompileOptions::default(),
);
assert!(result.has_output());
# Ok::<(), jsonrtl::ParseError>(())
```

## Workspace

```
crates/
  jsonrtl/           transport-free model, parser, validator, IR, compiler
  jsonrtl-cli/       command-line boundary          -> depends on jsonrtl
  jsonrtl-api/       Axum service boundary          -> depends on jsonrtl
  jsonrtl-profiles/  foreign-format import/export   -> depends on jsonrtl
profiles/            per-profile manifest, docs, and example projects
schemas/             canonical JSON Schema and contract examples
tests/golden/        byte-exact Verilog expectations
```

Dependencies point strictly inward. Nothing the core depends on knows about
Clap, Axum, or any foreign format.

The API runs parsing, kernel work, and response serialization on blocking
workers, with two jobs per router by default. Embedded services can configure
this through `RouterBuilder::max_concurrent_kernel_work`; waiting requests yield
to the async runtime, and `/health` remains independent of kernel capacity.

## What is implemented

- Canonical JSON Schema **v1.0**, plus **v1.1** sliced connections for multi-bit buses
- Strongly typed public document model with stable parse and schema diagnostics
- Component catalog: `AND` `OR` `XOR` `XNOR` `NAND` `NOR` `NOT` `BUFFER` `CONST`
- Semantic validation: per-bit single-driver, width agreement, catalog conformance
- Non-recursive combinational cycle detection
- Deterministic, collision-free identifier normalization
- Synthesizable continuous-assignment Verilog-2001 with one-based source maps
- Configurable resource limits, enforced before any deep work
- Import profiles for DLS and Logisim
- Verilog-to-DLS export through Yosys, including registers, latches, and clocks
- Yosys synthesis/checking and Icarus test-bench simulation through the CLI
- Bounded tool output, deadlines, Unix process cleanup, and protected result publication
- CLI and Axum shells over the reusable core

The canonical kernel and import path remain combinational: sequential elements,
clocks, hierarchical module instantiation, and tri-state buffers require future
schema versions. The Verilog exporter handles registers and clocks directly in
the DLS profile without changing that contract. HDL simulation is delegated to
Icarus in the CLI; simulation and physical design remain outside the kernel.

## Documentation

| Document | Contents |
|----------|----------|
| [`PRODUCT_SPEC.md`](PRODUCT_SPEC.md) | The public contract and versioning policy |
| [`ARCHITECTURE.md`](ARCHITECTURE.md) | Layering and dependency direction |
| [`docs/cli.md`](docs/cli.md) | Every command, option, and exit code |
| [`docs/profiles.md`](docs/profiles.md) | Import profiles and how to add one |
| [`docs/compiler.md`](docs/compiler.md) | The emitted Verilog subset |
| [`docs/diagnostics.md`](docs/diagnostics.md) | All 35 diagnostic codes |
| [`CHANGELOG.md`](CHANGELOG.md) | Release history |

## Development

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo test --doc --workspace
```

CI runs these checks on every push and pull request and also checks the locked
workspace with Rust 1.85. Golden Verilog is byte-exact, so a formatting change to
the emitter fails the build until the goldens are updated deliberately.

## License

Licensed under the [Apache License, Version 2.0](LICENSE).
