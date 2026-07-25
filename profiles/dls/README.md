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

Combinational, single-bit logic with **NAND** as the only primitive. Multi-bit
pins and every non-NAND built-in (`CLOCK`, `3-STATE BUFFER`, buses, displays,
memory, …) are rejected with a precise diagnostic. See `profile.toml`.
