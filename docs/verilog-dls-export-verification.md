# Verilog export verification — 2026-10-04

The seven audit fixes are retained. The new `export` command creates native
Digital-Logic-Sim 2.1.6 projects from Verilog, including registers, resets,
latches, buses, and an optional native CLOCK wrapper. The canonical kernel and
the import path keep their combinational contract.

## Results

| Check | Result |
|-------|--------|
| `cargo test --workspace --locked` | 290 passed; no failures or ignored tests |
| `cargo +1.85.0 test --workspace --locked` | 290 passed; no failures or ignored tests |
| `cargo +1.85.0 check --workspace --all-targets --locked` | Passed |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo test --doc --workspace --locked` | Passed; zero executable doctests |
| `cargo build --workspace --release --locked` | Passed |
| `git diff --check` | Passed |
| Real CLI/Yosys exports vs Icarus in upstream DLS | 20 fixtures; 50,180 assertions passed |
| Upstream DLS simulator baseline | 3,306 assertions passed |
| Additional repeated sequential-helper stress | 12 kinds × 100 runs; 23,000 assertions passed |
| Bundled counter example | 90 assertions passed over 10 fresh graphs |

The native verification uses unmodified upstream simulation and JSON serializer
code at commit `7aeb66ddff44f916ff7fafb043ab4476ef4409fe`, compiled with .NET
8.0.425. Yosys 0.33 and Icarus provide synthesis and reference execution. Unity
geometry, keyboard, GUI input holders, and audio sinks are stubbed; logical chip
graphs, feedback scheduling, split/merge converters, CLOCK, and serialization
come from upstream code. The tests also reject malformed wire addresses and
disconnected expected outputs.

## Coverage

The workspace suite includes the kernel, API, CLI, DLS import, Logisim import,
and the seven audit regressions. New export tests cover project completeness,
combinational importer round trips, deterministic bytes, SystemVerilog
`always_ff`, multiple files and includes, filenames with spaces/quotes/parser
punctuation, relative custom executables, missing tools, syntax/top/profile
errors, deadlines, source bounds, preserved destinations, initialized state,
and X/Z branches and reset values.

Native comparisons cover all 12 sequential cell kinds, positive/negative clock
edges, latch transparency and hold, enable and synchronous reset, every
supported asynchronous reset polarity/value in both clock phases, mixed edges,
8/32-bit feedback counters, full-width carry overflow, native CLOCK wrappers,
mux/add/XOR/equality, multiplication, signed comparisons, shifts, constants, and
ascending/nonzero-offset bus ranges. Data/enables get setup time before edges,
and clock phases allow physical NAND feedback to settle.

## Review and limits

Independent read-only review found no critical or important exporter defect.
It checked graph validation, bus ordering, helpers, pin addresses, name
collisions, cleanup, and publication, and independently ran the targeted Rust
tests. A later negative test exposed Yosys optimizing X/Z branches away; its
regression failed before the source-snapshot check and passes with it. The full
native comparison and both workspace suites were then rerun successfully.

The source check deliberately rejects unknown values in the elaborated graph.
Some exhaustive case lists still produce an internal unknown default in Yosys;
give those cases a defined `default`. This conservative choice can reject valid
binary HDL until a default is added; it prevents arbitrary replacement of an
unknown branch. Explicit initialization and uninitialized memory cells with
unknown state are also outside this first version.

The initially deferred synthesis-log cap was resolved by the subsequent
[HDL tools hardening](cli.md#tool-execution-limits). Export now shares bounded
in-memory capture (1 MiB combined), a 64 KiB diagnostic tail, workspace accounting,
and Unix resource/process limits with the new checking/simulation commands.

Unity GUI/project selection and visual layout were not exercised. Windows and
macOS publication/process handling were not executed. These results establish
native JSON compatibility and headless simulation behavior on Linux, rather
than GUI or cross-platform verification.

## Reproduce and open

See the [harness instructions](../scripts/dls-verification/README.md) for
prerequisites and the runnable comparison suite. The final run used:

```sh
python3 scripts/dls-verification/verify_export.py --keep /tmp/jsonrtl-native-source-states-verified
```

That directory retains original HDL, Icarus test benches, generated DLS
projects, scenarios, native traces, and a version/binary-hash manifest. Test
logs are `/tmp/jsonrtl-final-{stable-tests,msrv-tests,msrv-check,clippy,doc-tests,release}.log`.

The bundled `examples/verilog/counter.v` has already been exported to
`target/dls-counter`. Copy that entire directory into DLS's `Projects` save
folder, restart DLS, select `dls-counter`, and open `counter_CLOCK`. Pulse `rst`
high then low and set `en` high. Save locations and the complete command
reference are in [the CLI guide](cli.md#export-options).
