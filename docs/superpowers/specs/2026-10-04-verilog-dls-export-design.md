# Verilog to Digital-Logic-Sim export

The user wants to convert Verilog source such as `top.v` into a project that
opens in Digital-Logic-Sim, including registers and clocks. The first exporter
targets DLS only. The seven earlier fixes remain intact.

## Command and output

`jsonrtl export top.v --top top --profile dls --out build/top`
creates a new project directory containing `ProjectDescription.json` and
`Chips/*.json`. Multiple source files are accepted. `--system-verilog` enables
Yosys's supported SystemVerilog subset. `--yosys PATH` selects the executable;
`--timeout-seconds` bounds synthesis, default 60 and range 1 through 3600.
The destination must not exist. All files are prepared before publishing the
project. Existing directories and files are preserved on failure.

`--clock clk` additionally creates a wrapper with DLS's built-in CLOCK connected
to that single-bit input. Without it, clock pins remain external inputs.

## Conversion

The CLI invokes Yosys directly, without a shell: read sources, elaborate the
chosen hierarchy and processes, snapshot source JSON before optimization, flatten
the chosen top, synthesize without ABC, unmap register enables and synchronous resets into
input logic, then map combinational logic to NAND and NOT. The profile library
reads the resulting JSON and generates DLS descriptions and layouts. Sequential
export bypasses the combinational canonical kernel; it does not change the
kernel's schema or validation policy.

Native DLS pins have widths 1, 4, or 8. Wider ports are divided into supported
chunks with original index labels; connectivity follows Yosys's least-significant
bit ordering and DLS's most-significant-first split/merge ordering. Aliases,
constants, unused inputs, and fan-out retain their meaning. Registers use nested
master/slave NAND latches. Rising and falling edges and asynchronous reset to
either value with either polarity are supported. Unknown cells, unresolved
signals, conflicting drivers, unknown/high-impedance constants, unsupported
initial state, and combinational feedback produce explicit failures.

Both the source snapshot and mapped graph are checked. This prevents Yosys's
don't-care optimization from silently replacing X/Z branches or unknown reset
values. Case statements need a defined default when Yosys introduces an unknown
default in the elaborated graph, even for an exhaustive binary case list.

Descriptions follow DLS 2.1.6's real JSON fields, positive unique IDs, wire
addresses, case-insensitive chip names, project preferences and collections.
Helper names and top names cannot shadow built-in chips. Outputs are deterministic.

## Bounds and compatibility

Rust 1.85 remains the minimum version. Synthesis stays in the CLI; the core has
no external-tool dependency. Source files are limited to 8 MiB each and 32 MiB
total; Yosys JSON is capped at 32 MiB, cells at 10,000 and individual ports at
4,096 bits. Generated chips are capped at 8 MiB each and the project at 64 MiB.
Timeouts reap the synthesis process and its process group on Unix.

## Verification

Tests cover successful exports, complete project fields and addresses, wide and
reversed-index ports, constants and aliases, errors and preserved destinations,
missing tools and timeouts. Combinational projects round-trip through the
existing importer. Sequential and combinational scenarios run through actual
upstream DLS simulation code and compare against Icarus Verilog, with enough
simulation steps for latch propagation between clock transitions. CI installs
Yosys and Icarus for CLI integration tests and retains stable and MSRV checks.

Format and simulation references are the official Digital-Logic-Sim source:
<https://github.com/SebLague/Digital-Logic-Sim> and Yosys command documentation:
<https://yosyshq.readthedocs.io/projects/yosys/en/latest/cmd/index_passes_techmap.html>.
