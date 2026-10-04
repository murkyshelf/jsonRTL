# Yosys and Icarus HDL tools

The user requested native Yosys/Icarus support and hardening, and selected HDL
checking plus test-bench simulation. This extends the CLI; the canonical core
and HTTP API continue to operate without external tools.

## Commands

- `tools`: probe Yosys, Icarus, and VVP versions with configurable executable
  paths. Report every tool; fail if one is unavailable.
- `check SOURCE... --top MODULE [--tool both|yosys|iverilog]`: compile with
  Icarus and/or elaborate/check the synthesis graph with Yosys. Default both.
- `synth SOURCE... --top MODULE --out DIR`: Yosys synthesis, flattened gate
  netlist Verilog, JSON, and bounded synthesis report. Generic synthesis does
  not inherit DLS's restricted sequential-cell or initialization policy.
- `simulate SOURCE... --top TESTBENCH --out DIR`: compile with Icarus, execute
  VVP non-interactively, and publish the program, complete bounded stdout/stderr,
  and any regular waveform/data files generated in its working directory.
  `$fatal` and `$stop` fail. Simulation requires user-provided test-bench code.
- Existing `export` continues to produce DLS projects using the same hardened
  execution and input helpers.

Sources accept ordinary Verilog and optional `--system-verilog`. Module names
are ordinary identifiers. Executables can be absolute paths, relative paths
resolved from the invoking directory, or names found on PATH. HDL jobs accept a
1–3,600 second deadline (60 default), shared across their tool stages. `tools`
uses a separate deadline per probe (5 seconds default) so one missing tool does
not prevent the remaining reports.

## Hardening

- No shell invocation or arbitrary command-line/VPI-plugin passthrough.
- Private work directories; bounded snapshots of regular UTF-8 source files:
  8 MiB per file, 32 MiB aggregate, at most 256 files.
- Controlled include aliases and line directives preserve per-source paths,
  nested/parent includes, and filenames with spaces/quotes. Includes are
  compiler-managed and covered by execution resource/deadline limits.
- Capture stdout/stderr concurrently in bounded memory: 1 MiB combined. An
  overflow terminates the job; diagnostics show at most a 64 KiB tail.
- Default Unix limits: 2 GiB virtual memory per process, 32 MiB per generated
  file, zero core dumps, CPU budget tied to the deadline. Poll work directories
  for at most 128 MiB total, 4,096 entries, and depth 32; do not follow links.
- Unix process groups are terminated on timeout, resource failure, and cleanup.
  Monitor output readers as well as the direct process so an inherited pipe
  cannot evade the deadline.
- Reject generated symlinks, devices, and malformed/missing required outputs.
  Publish only complete projects/results to new destinations and preserve
  existing files/directories/symlinks, including publication races.

HDL and custom executables are trusted local code. Resource limits and a private
working directory are not an OS security sandbox: test benches can deliberately
access external files or invoke system functions. External execution stays out
of the network API for that reason. Unix resource/process-tree protections are
verified on Linux; other platforms retain direct-child termination.

## Verification

Use real Yosys, Icarus and VVP for successful checking, synthesis and simulation.
Test source/include and executable paths containing spaces and punctuation,
invalid tops, missing tools, syntax/elaboration failures, `$fatal`, `$stop`,
simulation stalls, generated VCD, and synthesis-netlist simulation. Reproduce
log floods, inherited pipes, non-regular source paths, oversized/symlink outputs,
and destination races with local executable fixtures. Preserve all existing
DLS, API and core regressions, run Rust 1.85 and stable tests, Clippy, formatting,
release build, and the upstream native DLS comparison suite. Obtain an
independent final review.
