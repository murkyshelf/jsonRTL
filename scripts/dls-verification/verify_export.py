#!/usr/bin/env python3
"""Compare original Verilog in Icarus against CLI exports in native DLS C#."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import random
import re
import shutil
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
REPO = HERE.parent.parent


def run(command, **kwargs):
    result = subprocess.run(command, text=True, capture_output=True, timeout=180, **kwargs)
    if result.returncode:
        raise RuntimeError(f"Command failed: {' '.join(map(str, command))}\n{result.stdout[-3000:]}\n{result.stderr[-6000:]}")
    return result.stdout


def port_range(spec):
    return (spec - 1, 0) if isinstance(spec, int) else spec


def width(spec):
    left, right = port_range(spec)
    return abs(left - right) + 1


def declaration(spec):
    left, right = port_range(spec)
    return "" if isinstance(spec, int) and spec == 1 else f"[{left}:{right}] "


def reference(folder, name, source, inputs, outputs, phases):
    src = folder / f"{name}.v"
    src.write_text(source)
    lines = ["module tb;"]
    lines += [f"reg {declaration(spec)}{pin} = 0;" for pin, spec in inputs.items()]
    lines += [f"wire {declaration(spec)}{pin};" for pin, spec in outputs.items()]
    lines += [f"{name} dut({', '.join('.' + pin + '(' + pin + ')' for pin in [*inputs, *outputs])});", "initial begin", "#20;"]
    for index, phase in enumerate(phases):
        for pin, value in phase["inputs"].items():
            lines.append(f"{pin} = {width(inputs[pin])}'d{value};")
        lines.append("#20;")
        fmt = " ".join(["%0d"] * (len(outputs) + 1))
        lines.append(f'$display("{fmt}", {index}, {", ".join(outputs)});')
    lines += ["$finish;", "end", "endmodule"]
    tb = folder / f"{name}_tb.v"
    tb.write_text("\n".join(lines))
    sim = folder / f"{name}.vvp"
    run(["iverilog", "-g2012", "-s", "tb", "-o", str(sim), str(src), str(tb)])
    rows = []
    for line in run(["vvp", str(sim)]).splitlines():
        fields = line.split()
        if len(fields) != len(outputs) + 1 or not fields[0].isdigit():
            continue
        index = int(fields[0])
        values = {}
        if phases[index].get("sample", True):
            for pin, value in zip(outputs, fields[1:]):
                if not value.isdecimal():
                    raise RuntimeError(f"Icarus has undefined sampled output {name}, phase {index}, {pin}: {value}")
                values[pin] = int(value)
        rows.append(values)
    if len(rows) != len(phases):
        raise RuntimeError(f"Missing reference samples for {name}")
    return src, rows


def pin_values(pins, logical, specs):
    result = {}
    for pin in pins:
        match = re.fullmatch(r"(.+?)(?:\[(-?\d+)(?::(-?\d+))?\])?", pin["Name"])
        base, first, last = match.groups()
        if base not in logical:
            continue
        shift = 0 if first is None else abs(int(last or first) - port_range(specs[base])[1])
        result[pin["Name"]] = (logical[base] >> shift) & ((1 << pin["BitCount"]) - 1)
    return result


def compare(folder, cli, dotnet, name, source, inputs, outputs, phases,
            repetitions=10, automatic=False, clock_steps=32, settle_ticks=32,
            project=None, chip_name=None):
    src, expected = reference(folder, name, source, inputs, outputs, phases)
    if project is None:
        project = folder / f"{name}_project"
        command = [str(cli), "export", str(src), "--top", name, "--out", str(project)]
        if automatic:
            command += ["--clock", "clk"]
        run(command)
    chip_name = chip_name or (name + "_CLOCK" if automatic else name)
    desc = json.loads((project / "Chips" / f"{chip_name}.json").read_text())
    scenario = {"chip": chip_name, "settleTicks": settle_ticks,
                "clockSteps": clock_steps if automatic else 0,
                "repetitions": repetitions, "steps": []}
    for phase, values in zip(phases, expected):
        step = {"inputs": pin_values(desc["InputPins"], phase["inputs"], inputs),
                "expect": pin_values(desc["OutputPins"], values, outputs)}
        if automatic:
            step["ticks"] = 1
        scenario["steps"].append(step)
    path = folder / f"{name}_scenario.json"
    path.write_text(json.dumps(scenario))
    result = json.loads(run([str(dotnet), str(HERE / "bin/Debug/net8.0/DlsHarness.dll"),
                             "--project", str(project), "--scenario", str(path)]))
    (folder / f"{name}_native_trace.json").write_text(json.dumps(result))
    helpers = sorted(p.stem for p in (project / "Chips").glob("JSONRTL_D*.json"))
    print(json.dumps({"fixture": name, "assertions": result["assertions"], "phases": len(phases),
                      "repetitions": repetitions, "helpers": helpers, "result": result["result"]}), flush=True)
    return result["assertions"]


def phase(**inputs):
    return {"inputs": inputs}


def ff_phases(positive, reset=None):
    active, inactive = (1, 0) if positive else (0, 1)
    initial = phase(c=inactive, d=0, r=0 if reset is None or reset[0] else 1)
    initial["sample"] = False
    phases = [initial, phase(c=active), phase(d=1), phase(c=inactive), phase(c=active),
              phase(d=0), phase(c=inactive), phase(c=active)]
    if reset is not None:
        r_active, value = reset
        r_off = 1 - r_active
        # Assert/release independently in each clock phase. Changing D during a
        # reset, and release without an active clock edge, must retain reset Q.
        for c in (active, inactive):
            phases += [phase(c=c), phase(r=r_active, d=1 - value),
                       phase(d=value), phase(d=1 - value), phase(r=r_off),
                       phase(d=value), phase(d=1 - value), phase(c=inactive), phase(c=active)]
    return phases


def counter_phases(cycles):
    phases = [phase(clk=0, rst=1, en=0), phase(rst=0)]
    rng = random.Random(1827)
    for _ in range(cycles):
        phases += [phase(clk=0, en=rng.randrange(2)), phase(clk=1),
                   phase(en=rng.randrange(2)), phase(clk=0)]
    phases += [phase(rst=1), phase(rst=0, en=1), phase(clk=1), phase(clk=0)]
    return phases


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--cli", type=Path, default=REPO / "target/debug/jsonrtl")
    parser.add_argument("--upstream", type=Path, default=Path("/tmp/jsonrtl-dls-upstream"))
    parser.add_argument("--keep", type=Path, help="Write fixtures and traces to a new folder instead of a temporary folder")
    parser.add_argument("--repetitions", type=int, default=10)
    args = parser.parse_args()
    if args.repetitions < 1:
        parser.error("--repetitions must be positive")
    dotnet = os.environ.get("DOTNET_BINARY") or shutil.which("dotnet") or "/tmp/jsonrtl-dotnet/dotnet"
    for executable in (args.cli, dotnet, shutil.which("iverilog"), shutil.which("vvp"), shutil.which("yosys")):
        if not executable or not Path(executable).is_file():
            parser.error("Requires built jsonrtl CLI, .NET8 SDK, Icarus and Yosys; see README")
    if args.keep:
        args.keep.mkdir(parents=True, exist_ok=False)
        folder = args.keep
        cleanup = None
    else:
        cleanup = tempfile.TemporaryDirectory(prefix="jsonrtl-native-dls-verify-")
        folder = Path(cleanup.name)
    # Keep one concrete exporter version while cargo may rebuild the workspace.
    cli_snapshot = folder / "jsonrtl-under-test"
    shutil.copy2(args.cli, cli_snapshot)
    (folder / "verification_manifest.json").write_text(json.dumps({
        "cli_sha256": hashlib.sha256(cli_snapshot.read_bytes()).hexdigest(),
        "upstream_commit": run(["git", "-C", str(args.upstream), "rev-parse", "HEAD"]).strip(),
        "dotnet": run([str(dotnet), "--version"]).strip(),
        "yosys": run(["yosys", "-V"]).strip(),
        "repetitions": args.repetitions,
    }, indent=2))
    native_baseline = json.loads(run(["bash", str(HERE / "run.sh"), "--upstream", str(args.upstream), "--self-test"]))
    total = 0
    for positive in (True, False):
        polarity = "P" if positive else "N"
        cases = [(None, polarity)] + [((high, value), polarity + ("P" if high else "N") + str(value))
                                     for high in (1, 0) for value in (0, 1)]
        for reset, suffix in cases:
            name = "seq_" + suffix
            sensitivity = "posedge c" if positive else "negedge c"
            body = "q <= d;"
            if reset is not None:
                high, value = reset
                sensitivity += " or " + ("posedge r" if high else "negedge r")
                body = f"if ({'r' if high else '!r'}) q <= 1'b{value}; else q <= d;"
            source = f"module {name}(input c,d,r,output reg q); always @({sensitivity}) {body} endmodule\n"
            total += compare(folder, cli_snapshot, dotnet, name, source, {"c": 1, "d": 1, "r": 1}, {"q": 1},
                             ff_phases(positive, reset), args.repetitions)
        name = "latch_" + polarity
        source = f"module {name}(input c,d,output reg q); always @* if ({'c' if positive else '!c'}) q = d; endmodule\n"
        active = 1 if positive else 0
        initial = phase(c=1-active, d=1)
        initial["sample"] = False
        phases = [initial, phase(c=active, d=0), phase(d=1), phase(c=1-active), phase(d=0),
                  phase(c=active), phase(c=1-active), phase(d=1), phase(c=active)]
        total += compare(folder, cli_snapshot, dotnet, name, source, {"c": 1, "d": 1}, {"q": 1}, phases, args.repetitions)

    for bits, cycles in ((8, 600), (32, 600)):
        name = f"counter{bits}"
        reset_value = 0 if bits == 8 else 0xfffffff0
        source = f"module {name}(input clk,rst,en,output reg [{bits-1}:0] q); always @(posedge clk or posedge rst) if(rst) q <= {bits}'d{reset_value}; else if(en) q <= q+1; endmodule\n"
        total += compare(folder, cli_snapshot, dotnet, name, source, {"clk": 1, "rst": 1, "en": 1}, {"q": bits},
                         counter_phases(cycles), min(args.repetitions, 3))
    name = "mixed"
    source = "module mixed(input clk,rst,en,input[7:0]d,output reg[7:0]q,output reg n); always @(posedge clk) if(rst) q<=0; else if(en) q<=d; always @(negedge clk) n<=d[0]; endmodule\n"
    phases = [phase(clk=0, rst=1, en=0, d=0), phase(clk=1), phase(clk=0), phase(rst=0)]
    phases[0]["sample"] = phases[1]["sample"] = False
    rng = random.Random(314)
    for _ in range(100):
        phases += [phase(d=rng.randrange(256), en=rng.randrange(2)), phase(clk=1),
                   phase(d=rng.randrange(256)), phase(clk=0)]
    total += compare(folder, cli_snapshot, dotnet, name, source, {"clk": 1, "rst": 1, "en": 1, "d": 8}, {"q": 8, "n": 1}, phases, args.repetitions)

    name = "comb"
    source = "module comb(input[11:0]a,b,input sel,output[11:0]y,output eq); assign y=sel?a+b:a^b; assign eq=(a==b); endmodule\n"
    phases = [phase(a=a, b=b, sel=s) for a, b in [(0,0),(4095,1),(2048,2048),(4095,4095),(0xA53,0x12F)] for s in (0,1)]
    phases += [phase(a=rng.randrange(4096), b=rng.randrange(4096), sel=rng.randrange(2)) for _ in range(250)]
    total += compare(folder, cli_snapshot, dotnet, name, source, {"a": 12, "b": 12, "sel": 1}, {"y": 12, "eq": 1}, phases, 1)

    name = "endian"
    source = "module endian(input[8:20]a,input[4:0]b,output[31:19]y,output[0:7]z,output[15:0]k,output one,zero); assign y=a; assign z={b,3'b101}; assign k=16'hA53C; assign one=1'b1; assign zero=1'b0; endmodule\n"
    phases = [phase(a=1 << i, b=i & 31) for i in range(13)] + [phase(a=8191, b=31), phase(a=0, b=0)]
    total += compare(folder, cli_snapshot, dotnet, name, source, {"a": (8,20), "b": 5},
                     {"y": (31,19), "z": (0,7), "k": 16, "one": 1, "zero": 1}, phases, 1)

    name = "arithmetic"
    source = "module arithmetic(input[7:0]a,b,input[2:0]s,input sel,output[7:0]sum,diff,prod,shl,shr,band,bor,bxor,inv,mux,output eq,gt,lt,sgt,truth); assign sum=a+b; assign diff=a-b; assign prod=a*b; assign shl=a<<s; assign shr=a>>s; assign band=a&b; assign bor=a|b; assign bxor=a^b; assign inv=~a; assign mux=sel?a:b; assign eq=a==b; assign gt=a>b; assign lt=a<b; assign sgt=$signed(a)>$signed(b); assign truth=(a&&b)||sel; endmodule\n"
    phases = [phase(a=rng.randrange(256), b=rng.randrange(256), s=rng.randrange(8), sel=rng.randrange(2)) for _ in range(150)]
    total += compare(folder, cli_snapshot, dotnet, name, source, {"a": 8, "b": 8, "s": 3, "sel": 1},
                     {**{n: 8 for n in ("sum","diff","prod","shl","shr","band","bor","bxor","inv","mux")},
                      **{n: 1 for n in ("eq","gt","lt","sgt","truth")}}, phases, 1)

    # Sample the center of each native CLOCK phase, allowing gate propagation.
    for bits in (8, 32):
        name = f"automatic{bits}"
        reset_value = 0 if bits == 8 else 0xfffffff0
        source = f"module {name}(input clk,rst,en,output reg[{bits-1}:0]q); always @(posedge clk or posedge rst) if(rst)q<={bits}'d{reset_value}; else if(en)q<=q+1; endmodule\n"
        phases = []
        for frame in range(1, 32 * 40 + 1):
            p = phase(clk=1 if (frame // 32) % 2 == 0 else 0, rst=1 if frame < 5 else 0, en=1 if frame >= 5 else 0)
            p["sample"] = frame % 32 == 16
            phases.append(p)
        total += compare(folder, cli_snapshot, dotnet, name, source, {"clk": 1, "rst": 1, "en": 1}, {"q": bits},
                         phases, min(args.repetitions, 3), automatic=True)
    summary = {"result": "PASS", "native_self_test_assertions": native_baseline["assertions"],
               "export_reference_assertions": total, "artifacts": str(folder) if args.keep else "temporary"}
    (folder / "verification_summary.json").write_text(json.dumps(summary, indent=2))
    print(json.dumps(summary), flush=True)
    if cleanup:
        cleanup.cleanup()


if __name__ == "__main__":
    main()
