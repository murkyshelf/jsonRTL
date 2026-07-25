# DLS Import Profile

Converts a **Sebastian Lague Digital-Logic-Sim** project into canonical circuit
documents and compiles each chip to Verilog. Full reference (pin-addressing
model, flatten-to-NAND algorithm, rejection list) lives in
[`../../docs/profiles.md`](../../docs/profiles.md); this folder holds the
manifest and a runnable example.

## Run the example

```sh
# Build the CLI, then import the bundled example project.
cargo build -p jsonrtl-cli
./target/debug/jsonrtl import profiles/dls/example --out /tmp/dls-verilog

# One <ChipName>.v is written per chip:
#   AND.v  OR.v  NOT.v  XOR.v  "1-bit adder.v"
```

Other modes:

```sh
# Print a single chip's Verilog:
jsonrtl import profiles/dls/example --chip AND --stdout

# Also emit the intermediate canonical JSON (the kernel contract):
jsonrtl import profiles/dls/example --out out --emit-canonical canon
```

The profile is auto-detected from the directory layout; pass `--profile dls` to
force it.

## What the example produces

`AND` is defined in DLS as two NANDs. It flattens to:

```verilog
module AND (
    IN,
    IN_2,
    OUT
);

input wire IN;
input wire IN_2;
output wire OUT;

wire net0;
wire net1;
wire net2;
wire net3;

assign net0 = IN;
assign net1 = IN_2;
assign OUT = net2;

assign net3 = ~(net0 & net1);
assign net2 = ~(net3 & net3);

endmodule
```

## Supported subset

Combinational logic of any width, with **NAND** as the only gate primitive.

Pins wider than one bit keep their width to the module boundary, so a 16-bit
adder compiles to `input wire [7:0] A0;` rather than eight scalar ports. DLS
splits and merges buses with built-in `X-YBIT` chips ("convert X-bit to Y-bit",
splitting when X > Y), ordered most significant first; `BUS-N` is a fan-out
alias and `BUS-TERMINUS-N` a dropped sink. None of these emit logic — under a
bit-level union-find a narrow pin simply *is* a bit of the wide one.

Rejected with a precise diagnostic, never silently skipped:

- built-ins outside that set: `CLOCK`, `PULSE`, `KEY`, `3-STATE BUFFER`,
  `7-SEGMENT` and other displays, `ROM` and other memory;
- a wire joining pins of different widths.

Latch structures built from NAND feedback — DLS registers and RAM — are genuine
combinational cycles for this kernel and are reported as such. See
`profile.toml` and `../../docs/profiles.md`.
