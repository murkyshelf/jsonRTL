# Upstream DLS simulator harness

This optional verification tool compiles the original DLS `Simulator`, `SimChip`,
`SimPin`, `PinState`, description classes, `Serializer`, `BuiltinChipCreator`, and `ChipLibrary`
directly from an upstream checkout. It uses the real `RunSimulationStep` scheduler,
including traversal ordering, latch feedback, random race ordering, and native
clock/split/merge behavior. It does not replace registers with nominal flip-flops.

Only Unity geometry, GUI input holders, keyboard polling, and audio sinks are
stubbed. This is headless simulation verification; it does not exercise the Unity
GUI, project-selection screens, layout drawing, or audio. Chip and project JSON
use the upstream Newtonsoft serializer and its bundled DLL, including the
original vector, color, and date converters. Scenario JSON uses `System.Text.Json`.
Wire directions, IDs, owner addresses,
and widths are checked before the upstream graph builder can silently discard an
invalid wire.

Prerequisites are a .NET 8 SDK and the DLS source checkout. The source used during
development was `SebLague/Digital-Logic-Sim` commit
`7aeb66ddff44f916ff7fafb043ab4476ef4409fe`. Nothing is added to the product's
runtime dependencies. No NuGet packages or live external services are used when
running the harness.

```sh
git clone https://github.com/SebLague/Digital-Logic-Sim.git /tmp/jsonrtl-dls-upstream
git -C /tmp/jsonrtl-dls-upstream checkout 7aeb66ddff44f916ff7fafb043ab4476ef4409fe
bash scripts/dls-verification/run.sh --self-test
```

The runner uses `dotnet` on `PATH`, or `/tmp/jsonrtl-dotnet/dotnet`. Override this
with `DOTNET_BINARY`. Set `DLS_SOURCE` or `--upstream PATH` for a different source
checkout. The native self-test checks exhaustive NAND, 4/8-bit split/merge,
nibble split/merge, native CLOCK, custom NOT, and repeated native D-LATCH and
FLIP-FLOP capture/hold behavior.

To test a generated project:

```sh
bash scripts/dls-verification/run.sh --project /tmp/generated-dls --scenario /tmp/scenario.json
```

The project path contains `Chips/*.json`, or points directly to the chips folder.
A scenario drives the named top-level input pins and samples named output pins:

```json
{
  "chip": "example",
  "settleTicks": 32,
  "clockSteps": 0,
  "repetitions": 20,
  "steps": [
    {"inputs": {"clk": 0, "d": 0}},
    {"inputs": {"clk": 1}, "expect": {"q": 0}},
    {"inputs": {"d": 1}, "expect": {"q": 0}},
    {"inputs": {"clk": 0}, "expect": {"q": 0}},
    {"inputs": {"clk": 1}, "expect": {"q": 1}}
  ]
}
```

Each step holds its inputs for `settleTicks` simulator frames, or its own `ticks`
value. Unspecified inputs retain their previous values and initially start low.
Native built-in CLOCK chips use `clockSteps` frames per half-cycle; `0` holds
their output low. Hold clock phases long enough for NAND feedback and the
combinational path to settle, and give data a separate setup step before an edge.
The output is a JSON trace with values, disconnected-bit masks, simulator frame,
and expanded graph counts. An expected output also requires all its bits to be
connected. Any mismatch produces a nonzero exit code.

The complete local export verification suite generates Verilog fixtures, exports
them with the real CLI and Yosys, runs the original HDL in Icarus, and compares its
outputs with this upstream DLS runtime:

```sh
cargo build -p jsonrtl-cli
python3 scripts/dls-verification/verify_export.py
```

It requires `yosys`, `iverilog`, and `vvp` on `PATH`. `--cli PATH` selects another
CLI binary; the runner snapshots it so concurrent builds cannot change the
exporter during verification. `--keep /tmp/new-results-directory` retains the
source, test benches, exported projects, scenarios, native traces, and a version
manifest. The directory must be new. Missing tools, undefined reference outputs,
export failures, and native mismatches fail the command. Fixtures cover all 12
sequential kinds, all asynchronous reset polarities and values in both clock
phases, latch transparency/hold, mixed clock edges, enable and synchronous reset,
8/32-bit feedback counters, mux/add/XOR/equality, arithmetic and signed comparisons,
shifts, offset/ascending port ranges, constants, and automatic native CLOCK.

Sequential cases repeat with fresh graphs to exercise the upstream scheduler's
random feedback ordering. Undefined startup state is explicitly excluded until
the fixture initializes the register through its actual clock/reset behavior.
Native CLOCK traces sample the middle of each 32-frame phase to allow physical
gate propagation, while manual clock cases hold every phase for 32 frames.
