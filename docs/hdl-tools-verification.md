# HDL tools verification — 2026-10-04

The CLI now supports `tools`, `check`, `synth`, and `simulate`. Yosys provides
checking and flattened gate synthesis; Icarus/VVP compile and execute user test
benches. Existing DLS export shares the hardened process, input, and publication
helpers. The seven earlier audit fixes are retained, and the core/HTTP API do
not execute external tools.

## Results

| Check | Result |
|-------|--------|
| `cargo test --workspace --locked` | 320 passed; no failures or ignored tests |
| `cargo +1.85.0 test --workspace --locked` | 320 passed; no failures or ignored tests |
| New HDL CLI integration tests | 30 passed |
| Existing DLS export CLI integration tests | 16 passed |
| `cargo +1.85.0 check --workspace --all-targets --locked` | Passed |
| `cargo fmt --all --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo test --doc --workspace --locked` | Passed; zero executable doctests |
| `cargo build --workspace --release --locked` | Passed |
| `git diff --check` | Passed |
| Native DLS reference comparison after include fixes | 20 fixtures; 50,180 assertions passed |
| Upstream native simulator baseline | 3,306 assertions passed |
| Release counter check and synthesis | Passed with both installed compilers |
| Release original and synthesized counter simulation | Both assertion benches passed; both VCDs saved |

Installed tools tested: Yosys 0.33, Icarus Verilog 12.0, VVP 12.0. The native
DLS harness uses the pinned upstream commit and .NET version documented in
[the export verification record](verilog-dls-export-verification.md).

## Covered behavior

Real tools verify discovery, compiler checks, gate netlist completeness,
registers with initialized state, test-bench assertions, generated VCD,
SystemVerilog plus arguments, and simulation of synthesized netlists. The
counter bench checks reset, counting, enable hold, and asynchronous reset.

Negative tests verify syntax errors, missing executables, invalid top names,
source count/size limits, non-UTF-8 output paths, preserved destinations,
publication races, `$fatal`, `$stop`, infinite simulation, and a shared
compiler/runtime deadline. Local executable fixtures exercise output floods,
descendants holding pipes, background-process cleanup, FIFO input rejection,
generated symlinks, oversized sparse artifacts, reserved log collisions, and
modified simulation images. Quoted paths, shell punctuation, explicit relative
executables, relative PATH entries, and directories containing `:` are covered.

Tests first reproduced command-registration failures, the old uncapped exporter
output, a reserved-log overwrite, and the output-path serialization panic before
the corresponding fixes. The independent review found a genuine compiler mismatch:
global include directories made Icarus choose the first `defs.vh` for two sources
with different same-named headers. Source directory aliases, `line` directives,
and Icarus relative-include mode fix it while retaining bounded source snapshots.
The regression covers same-named, nested, and parent-directory headers. Review
reruns also passed a header named `source0.v` and cross-file macros. Relative
PATH lookup and the `:` compatibility edge have reproducing regressions.

## Artifacts and reproduction

The release counter results are in `target/hdl-counter-fbp0j51o/`: `synth/`
contains Verilog/JSON/statistics; `sim/` and `netlist-sim/` contain successful
assertion output and `counter.vcd`. `verification.json` records the commands,
tool versions, and release binary SHA-256.

The native comparison artifacts are in `/tmp/jsonrtl-hdl-final-native/`, including
source HDL, Icarus benches, exported DLS projects, scenarios, traces, and the
comparison manifest. Reproduce with:

```sh
cargo test --workspace --locked
cargo +1.85.0 test --workspace --locked
cargo build --workspace --release --locked
python3 scripts/dls-verification/verify_export.py --cli target/release/jsonrtl
```

Final Rust logs are `/tmp/jsonrtl-hdl-final-{stable-tests,msrv-tests}.log` and
`/tmp/jsonrtl-hdl-{clippy,msrv-check,doc-tests,release}.log`. See the [CLI
guide](cli.md#hdl-tool-options) for directly runnable counter commands. CI runs
the real compiler tests, release counter checks/simulation, and native DLS suite.

## Scope and limits

Verification covers Linux with the installed tool versions. Unity GUI interaction
and Windows/macOS execution were not exercised. HDL and executable overrides
are trusted local code: resource limits and private work directories are not an
OS security sandbox. The runner bounds capture, file growth, work-directory size,
and deadlines; Unix cleanup covers the process group, while non-Unix cleanup
terminates the direct child. Includes remain compiler-managed. Windows source
directory aliases require directory-symlink permission.

Compiler messages use controlled `sourceN.v` names, with N matching the supplied
source index. Relative PATH entries are normally made absolute; when a Unix
directory contains `:`, the selected executable is resolved separately and its
subprocesses inherit the original PATH. Use absolute PATH directories or explicit
helper paths when relying on subprocess lookup in that unusual configuration.
