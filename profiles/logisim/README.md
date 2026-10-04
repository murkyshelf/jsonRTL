# Logisim / Logisim Evolution Import Profile

Converts a Logisim `.circ` file into canonical circuit documents and compiles
each contained circuit to Verilog. Full reference in
[`../../docs/profiles.md`](../../docs/profiles.md).

> **Status: experimental.** The port-geometry rules were written from knowledge
> of the format, not from real exports. Drop sample files in `samples/` (see
> `samples/README.md`) so they can be calibrated.

## Run it

```sh
cargo build -p jsonrtl-cli
./target/debug/jsonrtl import path/to/project.circ --out /tmp/verilog
```

The profile is detected from the `.circ` extension; pass `--profile logisim` to
force it. `--chip <name>` selects a single circuit, `--stdout` prints it.

## Why this profile is different from DLS

DLS records connectivity explicitly: every wire names the pin ids it joins.
**Logisim records geometry** — a wire is a pair of coordinates, and it connects
to whatever port happens to share that point. Conversion therefore has to
reconstruct where each component's ports sit from its `loc`, `facing`, `size`,
and input count, and merge points that touch (including a wire that ends
part-way along another wire).

That reconstruction lives in one place, `geometry.rs`, so it can be corrected in
isolation. Every rule is documented as an explicit assumption and pinned by a
test.

## Supported subset

Single-bit combinational logic: `Pin`, the basic gates
(`AND`/`OR`/`XOR`/`NAND`/`NOR`/`XNOR`/`NOT`/`Buffer`), and nothing else yet.
Logisim gates may take more than two inputs; those fold into the kernel's
2-input catalog gates, with inverting forms inverted once at the end.

Anything outside the subset — other libraries, multi-bit signals, and for now
subcircuit instances — is rejected with a diagnostic naming the component and
its coordinate, rather than silently dropped.

Imports are bounded to 8 MiB, 100,000 XML nodes, and project totals of 256
circuits, 20,000 components, and 20,000 wires. Gates accept 1–64 inputs and
positive sizes up to 1,000; coordinates must lie within +/-1,000,000. Conversion
also caps component ports and geometric connectivity work. Malformed explicit
geometry attributes are rejected instead of replaced with defaults. Geometry
checks run when the selected circuit is elaborated, so an invalid sibling does
not block valid circuits. Byte, XML node, and project count limits are global. See
the [input limits](../../docs/profiles.md#input-limits) for the complete bounds.
