# `jsonrtl` CLI Reference

## Installation

```sh
cargo install --path crates/jsonrtl-cli --locked
```

This builds in release mode and installs the binary as `jsonrtl` in
`$CARGO_HOME/bin` (usually `~/.cargo/bin`), which must be on your `PATH`.
Install elsewhere with `--root`, e.g. `--root ~/.local`.

The crate is `publish = false`, so `cargo install jsonrtl-cli` from a registry
does not work; `--path` is the only route.

For development, run without installing:

```sh
cargo run -p jsonrtl-cli -- import <PROJECT> --out build/
```

If you invoke `target/release/jsonrtl` directly, remember it is only as current
as your last `cargo build --release` — `cargo run` and `cargo install` rebuild
for you, a stale path does not.

## Commands

| Command | Description |
|---------|-------------|
| `validate CIRCUIT` | Validate a canonical circuit document against schema and semantic rules. |
| `compile CIRCUIT` | Compile a canonical circuit document to deterministic Verilog-2001. |
| `import PROJECT_DIR` | Import a foreign project (e.g. DLS) and compile each unit to Verilog. |
| `export SOURCE... --top MODULE --out DIR` | Synthesize Verilog into a native Digital-Logic-Sim project. |
| `tools` | Report installed Yosys, Icarus Verilog, and VVP versions. |
| `check SOURCE... --top MODULE` | Check with Icarus and Yosys, or select one compiler. |
| `synth SOURCE... --top MODULE --out DIR` | Produce a flattened Yosys gate netlist and statistics. |
| `simulate SOURCE... --top TESTBENCH --out DIR` | Compile and run an Icarus test bench, saving logs and generated waveforms. |
| `profiles` | List the import profiles available in this build. |
| `schema` | Print the canonical circuit JSON Schema to stdout. |

## Which command do I want?

The commands split by **what kind of file you have**.

| You have | Use | Note |
|----------|-----|------|
| A DLS project directory, or a Logisim `.circ` | `import` | Converts *and* compiles in one step |
| A canonical circuit JSON document | `validate` or `compile` | The kernel's own format |
| Synthesizable Verilog source files | `export` | DLS project, including registers and clocks |
| Verilog needing checks or a gate netlist | `check` or `synth` | Native Icarus/Yosys tool execution |
| Verilog design and test-bench sources | `simulate` | Select the test-bench module with `--top` |

`import` takes the **project directory**, not one chip file, and picks the
profile automatically:

```sh
jsonrtl import ~/path/to/DLS/Projects/test --chip OR --stdout
```

`--profile` belongs to `import` and `export`; it is not a global option, and
`validate` has no use for it. To validate a foreign chip you must convert it
first:

```sh
jsonrtl import <PROJECT> --chip OR --out build/ --emit-canonical canon/
jsonrtl validate canon/OR.json
```

## Global Options

| Option | Values | Default |
|--------|--------|---------|
| `--diagnostics` | `human`, `json` | `human` |
| `--list-profiles` | flag | off |

`--list-profiles` prints the import profiles in this build and exits. It is
global, so it works bare (`jsonrtl --list-profiles`) or alongside the command it
informs (`jsonrtl import --list-profiles`), and in that form needs neither a
project nor `--out`.

Diagnostics are written to **stderr**. Generated Verilog, schema, profile
listings, and `tools` reports go to **stdout**. HDL simulation output is saved
in the result directory rather than mixed into CLI diagnostics.

## Exit Codes

| Code | Category | Meaning |
|------|----------|---------|
| 0 | success | The command completed successfully. |
| 2 | invalid | Invalid input/arguments; compiler or test-bench failure; deadline or resource limit exceeded. |
| 3 | io | Input unreadable, output unwritable or protected, tool executable unavailable. |
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
| `--skip-unsupported` | Emit every unit that compiles rather than failing on the first that does not. Conflicts with `--chip`. |

Output files are written with the same atomic, overwrite-protected policy as
`compile`. Conversion failures (unsupported constructs, malformed input) exit
with code `2`; unreadable inputs or unwritable outputs exit with code `3`.

`--chip` converts only that unit and its dependencies, so an unsupported chip
elsewhere in the project cannot block it.

### Partial imports

By default a whole-project import fails on the first unit it cannot handle, and
writes nothing. Real projects accumulate work-in-progress chips, so
`--skip-unsupported` emits everything that does compile instead. It is never
silent: each skipped unit is listed with its reason, the summary gives a count,
and the run **still exits `2`** so a script cannot mistake a partial import for
a complete one. If nothing at all compiles, the run fails with
`no_supported_units`.

```sh
jsonrtl import ~/my-dls-project --out build/ --skip-unsupported
# compiled unit '16-BIT ADDER'
# ...
# skipped unit '1-bit reg': GRAPH_COMBINATIONAL_CYCLE: Combinational cycle ...
# import: 17 unit(s) from project 'test'
# import: 15 unit(s) skipped
```

## Export Options

`export` uses Yosys and its ABC mapper to turn synthesizable Verilog into a
Digital-Logic-Sim **2.1.6** project. Install Yosys with ABC first; it is an
external dependency of this command only. On Debian/Ubuntu: `sudo apt install
yosys`. Supply every source file needed by the top module:

```sh
jsonrtl export top.v child.v --top top --profile dls --out build/top
jsonrtl export examples/verilog/counter.v --top counter --out build/counter --clock clk
```

| Option | Description |
|--------|-------------|
| `SOURCE...` | One or more UTF-8 Verilog source files; relative includes resolve from each source's directory. |
| `--top MODULE` | Required top-module name; ordinary Verilog identifiers only. |
| `--profile ID` | Output format; currently `dls` only, also the default. |
| `--out DIR` | Required new project directory. There is no overwrite option. |
| `--clock PORT` | Add a wrapper connecting the named single-bit input to DLS's built-in CLOCK. |
| `--system-verilog` | Enable the SystemVerilog subset supported by the installed Yosys. |
| `--yosys PATH` | Synthesis executable; defaults to `yosys` on PATH. |
| `--timeout-seconds N` | Synthesis deadline, 1–3,600 seconds; default 60. |

The result contains `ProjectDescription.json` and `Chips/*.json`. Hierarchy is
flattened, Boolean/arithmetic logic is mapped to NAND, and registers/latches use
reusable NAND helper chips. Enables and synchronous resets become input muxes;
both edge polarities and asynchronous reset polarities/values are supported.
The default export retains clock inputs. `--clock clk` additionally creates
a `<top>_CLOCK` wrapper, removes its external `clk` pin, and enables a native
clock with a conservative default of 250 simulation steps per half-cycle.
The success message names the actual chip to open, including any collision-safe
rename. Ports wider than 8 bits are split into labeled 8/4/1-bit chunks such as
`data[7:0]` and `data[11:8]`; original offsets and ascending ranges are preserved.

### Opening the generated project

Close DLS, then copy the **whole output directory** into its `Projects` save
folder. Restart DLS, select the generated project, and open the chip reported by
the exporter. For the upstream desktop build, the usual save folders are:

| OS | Projects directory |
|----|--------------------|
| Linux | `~/.config/unity3d/SebastianLague/Digital-Logic-Sim/Projects/` |
| Windows | `%USERPROFILE%\AppData\LocalLow\SebastianLague\Digital-Logic-Sim\Projects\` |
| macOS | `~/Library/Application Support/SebastianLague/Digital-Logic-Sim/Projects/` |

Linux uses `$XDG_CONFIG_HOME` instead of `~/.config` when configured. Find the
directory of an existing DLS project if using a custom build or save location.
The source format and company/product names come from upstream
[SavePaths.cs](https://github.com/SebLague/Digital-Logic-Sim/blob/7aeb66ddff44f916ff7fafb043ab4476ef4409fe/Assets/Scripts/SaveSystem/SavePaths.cs)
and [ProjectSettings.asset](https://github.com/SebLague/Digital-Logic-Sim/blob/7aeb66ddff44f916ff7fafb043ab4476ef4409fe/ProjectSettings/ProjectSettings.asset).

For the counter example, open `counter_CLOCK`, pulse `rst` high then low, set
`en` high, and observe `q`. DLS simulates physical gate propagation: give data
and enables time to settle before an edge, and keep clock phases long enough
for feedback and the combinational path to settle. Larger designs may need
a slower clock in DLS preferences.

### Supported semantics and limits

Explicit initialized register state, X/Z values, tri-state/inout ports,
unresolved modules, unsupported remaining Yosys cells, and combinational cycles
are rejected. Memories work only when Yosys completely lowers them into the
supported cells. This does not add sequential logic to the canonical kernel or
the import command. HDL requiring simulation-only constructs is outside this
synthesis path; Yosys determines its synthesizable subset.

An elaborated source snapshot is taken before technology mapping and checked
for X/Z before publishing. Include a defined `default` in case statements when
Yosys otherwise introduces an unknown default, even if the listed cases cover
every binary selector value. Uninitialized memories whose elaborated cells
contain unknown values are outside this supported subset.

Direct source reads are capped at 8 MiB per file, 32 MiB in total, and 256 files; mapped JSON at
32 MiB; cells at 10,000; each port at 4,096 bits; each generated chip at 8 MiB;
and the generated project at 64 MiB. Included files are read by Yosys and are
governed by the synthesis deadline rather than the direct-source byte caps.
Source directory aliases use temporary directory links; Windows must permit directory symlinks
(for example, through Developer Mode). Export also uses the [shared process
limits](#tool-execution-limits), including bounded output capture and Unix
process-group cleanup for Yosys and ABC.

Validation finishes before publication. Existing files, directories, and
symlinks are refused; failures leave no partial project at `--out`. JSON
diagnostics include `topChip`, `outputDir`, and generated chip names on success.
The [native verification suite](../scripts/dls-verification/README.md) compares
real CLI exports with Icarus using upstream DLS's scheduler and JSON serializer.
It exercises headless simulation; it does not automate the Unity GUI.

## HDL Tool Options

Install Yosys with ABC and Icarus Verilog with VVP. On Debian/Ubuntu:
`sudo apt install yosys iverilog`. Run `jsonrtl tools` to discover every tool's
version and availability. A missing or unrecognized tool makes `tools` exit
`3` while still reporting the other tools. Its `--timeout-seconds N` applies
per probe and defaults to 5 seconds; JSON output goes to stdout.

`check`, `synth`, and `simulate` share these options:

| Option | Description |
|--------|-------------|
| `SOURCE...` | One or more UTF-8 source files; supply all required design/test-bench modules. |
| `--top MODULE` | Required ordinary Verilog identifier; choose the test-bench top for simulation. |
| `--system-verilog` | Enable the installed compiler's SystemVerilog subset (`-g2012` for Icarus). |
| `--timeout-seconds N` | Deadline shared across all tool stages, 1–3,600 seconds; default 60. |

Each command supports its relevant executable overrides: `--yosys PATH`,
`--iverilog PATH`, and/or `--vvp PATH`. `tools` accepts all three. A bare name
uses PATH; a path with directory components resolves from the invoking
directory, before changing to the private work directory.

### Check

```sh
jsonrtl check top.v child.v --top top
jsonrtl check top.v tb.v --top tb --tool iverilog
```

`--tool both|yosys|iverilog` defaults to `both`. Icarus checks compilation and
elaboration without creating a simulation image. Yosys elaborates the selected
hierarchy, lowers processes, and checks the flattened graph for problems such
as conflicting or undriven signals. Use Icarus alone for simulation-only code.
Neither check proves functional equivalence; use an asserting test bench.

### Synthesize

```sh
jsonrtl synth top.v child.v --top top --out build/top-synth
```

`synth` runs Yosys generic synthesis with flattening and an asserted graph check.
It writes `netlist.v`, `netlist.json`, `statistics.json`, and `report.log` to a
new `--out` directory. The Verilog is a self-contained gate netlist suitable
for Icarus. Unlike DLS export, generic synthesis accepts the initialized state
and cell kinds supported by the installed Yosys. `report.log` holds tool
warnings/output; detailed cell counts are in `statistics.json`.

### Simulate

```sh
jsonrtl simulate examples/verilog/counter.v examples/verilog/counter_tb.v \
  --top counter_tb --out build/counter-sim
jsonrtl simulate top.v tb.v --top tb --out build/test --plusarg cycles=100
```

Icarus compiles the sources, then VVP runs non-interactively. The test bench
must finish within the deadline. `$fatal` and `$stop` fail the command. Ordinary
`$display` text does not constitute an assertion: use `$fatal` to signal failed
checks. `--plusarg VALUE` passes a test-bench `+VALUE` argument and may be repeated
up to 256 times, at most 4,096 bytes each without control characters.

The new result directory contains `simulation.vvp`, `compile.log`, `stdout.txt`,
`stderr.txt`, and regular files generated by the test bench in its work directory.
VCD output requires `$dumpfile`/`$dumpvars` in the bench; the counter example
creates `counter.vcd`. Those four result names are reserved, and modifying the
compiled program or creating a reserved log filename fails publication. The
compiler and runtime share one deadline and one output budget. Failed runs
report a bounded diagnostic tail and leave no result directory.

### Tool execution limits

These limits also cover DLS `export`:

| Resource | Limit |
|----------|-------|
| Direct sources | 256 files; 8 MiB each; 32 MiB total; regular UTF-8 files without NUL |
| Captured stdout/stderr | 1 MiB combined across all stages; at most 64 KiB in failure messages |
| Generated files | 32 MiB per file; regular files only |
| Private work directory | 128 MiB total; 4,096 entries; depth 32; directory links are not traversed |
| Unix process limits | 2 GiB virtual memory per process; 32 MiB file size; no core dumps; CPU budget tied to deadline |

Sources are copied to controlled filenames before tool invocation. Source
directory aliases and Verilog line directives preserve each file's relative
includes, including nested and parent paths. Icarus checks the including file's
directory first, then the fallback include directories in source order.
Compiler diagnostics identify `sourceN.v`, where N is the zero-based index in
the supplied source list. Included files remain compiler-managed and are covered by process
and deadline limits, rather than the direct-source byte caps. Windows requires
permission to create directory symlinks for source directory aliases.

Tools run directly without a shell. Unix process groups are cleaned up on
success, failure, resource exhaustion, and timeout. Output readers remain under
the deadline even when a descendant inherits a pipe. Generated symlinks/devices
are rejected. Complete artifacts are staged before publication; existing
destinations are preserved, including Linux publication races. `--out` must be
a new UTF-8 directory path without control characters.

HDL and executable overrides are trusted local code. The private work directory
and resource limits are not an OS security sandbox; test benches can access
external files and system functions. These commands are CLI-only. Linux process,
resource, and publication behavior was tested; non-Unix cleanup terminates only
the direct child, and Windows/macOS behavior was not exercised. Tool usage
details: [Icarus compiler flags](https://steveicarus.github.io/iverilog/usage/command_line_flags.html)
and [VVP runtime flags](https://steveicarus.github.io/iverilog/usage/vvp_flags.html).

## Listing Profiles

Each import profile reports its source tool, expected input layout, supported
subset, and maturity. Three equivalent forms print it:

```sh
jsonrtl profiles                      # subcommand
jsonrtl --list-profiles               # global flag
jsonrtl import --list-profiles        # while reaching for import
jsonrtl --diagnostics json profiles   # machine-readable
```

`--profile` errors also name the ids that do exist, so a typo is
self-correcting.

A profile marked `experimental` has been implemented from the file format but
not yet calibrated against real exports from the tool.

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
