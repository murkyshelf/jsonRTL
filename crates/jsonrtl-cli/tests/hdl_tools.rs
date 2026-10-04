use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "jsonrtl-hdl-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, name: &str, body: &str) -> PathBuf {
        let p = self.0.join(name);
        fs::write(&p, body).unwrap();
        p
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jsonrtl"))
        .args(args)
        .output()
        .unwrap()
}
fn error(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
fn success(output: &Output) {
    assert_eq!(output.status.code(), Some(0), "{}", error(output));
}
fn hdl(s: &Scratch) -> PathBuf {
    s.write(
        "top.v",
        "module top(input a,b,output y); assign y=a^b; endmodule\n",
    )
}

#[cfg(unix)]
#[test]
fn non_utf8_output_parent_is_rejected_without_panic_or_publication() {
    use std::os::unix::ffi::OsStringExt;
    let s = Scratch::new();
    let source = hdl(&s);
    let parent =
        s.0.join(std::ffi::OsString::from_vec(b"parent-\xff".to_vec()));
    fs::create_dir(&parent).unwrap();
    let out = parent.join("result");
    let output = Command::new(env!("CARGO_BIN_EXE_jsonrtl"))
        .arg("synth")
        .arg(source)
        .args(["--top", "top", "--out"])
        .arg(&out)
        .args(["--diagnostics", "json"])
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2), "{}", error(&output));
    let report: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(report["error"]["category"], "invalid_output");
    assert!(!out.exists());
}

#[test]
fn discovers_all_real_tools_with_versions() {
    let output = cli(&["tools", "--diagnostics", "json"]);
    success(&output);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    for name in ["yosys", "iverilog", "vvp"] {
        assert_eq!(report["tools"][name]["available"], true);
        assert!(
            !report["tools"][name]["version"]
                .as_str()
                .unwrap()
                .is_empty()
        );
    }
}

#[test]
fn both_compilers_check_a_real_design() {
    let s = Scratch::new();
    let source = hdl(&s);
    let output = cli(&[
        "check",
        source.to_str().unwrap(),
        "--top",
        "top",
        "--diagnostics",
        "json",
    ]);
    success(&output);
    let report: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(
        report["checkedWith"],
        serde_json::json!(["iverilog", "yosys"])
    );
}

#[test]
fn standalone_yosys_synthesis_creates_a_self_contained_netlist() {
    let s = Scratch::new();
    let source = hdl(&s);
    let out = s.0.join("synth");
    let output = cli(&[
        "synth",
        source.to_str().unwrap(),
        "--top",
        "top",
        "--out",
        out.to_str().unwrap(),
    ]);
    success(&output);
    for file in ["netlist.v", "netlist.json", "statistics.json", "report.log"] {
        assert!(out.join(file).is_file(), "missing {file}");
    }
    let check = cli(&[
        "check",
        out.join("netlist.v").to_str().unwrap(),
        "--top",
        "top",
        "--tool",
        "iverilog",
    ]);
    success(&check);
}

const TB: &str = "module tb; reg a=0,b=0; wire y; top dut(a,b,y); initial begin $dumpfile(\"wave.vcd\"); $dumpvars(0,tb); #1; if(y!==0) $fatal(1,\"bad zero\"); a=1; #1; if(y!==1) $fatal(1,\"bad xor\"); $display(\"PASS\"); $finish; end endmodule\n";

#[test]
fn identically_named_headers_resolve_from_each_source_directory() {
    let s = Scratch::new();
    s.write("shared.vh", "`define SHARED 1\n");
    for (name, value) in [("a", 1), ("b", 0)] {
        fs::create_dir(s.0.join(name)).unwrap();
        fs::create_dir(s.0.join(name).join("nested")).unwrap();
        s.write(&format!("{name}/defs.vh"), "`include \"nested/value.vh\"\n");
        s.write(
            &format!("{name}/nested/value.vh"),
            &format!("`define VALUE {value}\n"),
        );
        s.write(
            &format!("{name}/{name}.v"),
            &format!("`include \"../shared.vh\"\n`include \"defs.vh\"\nmodule {name}(output y); assign y=`VALUE & `SHARED; endmodule\n`undef VALUE\n"),
        );
    }
    let tb = s.write("tb.v", "module tb; wire x,y; a aa(x); b bb(y); initial begin #1; if(x!==1 || y!==0) $fatal(1,\"wrong headers: %b %b\",x,y); $finish; end endmodule\n");
    let output = cli(&[
        "simulate",
        s.0.join("a/a.v").to_str().unwrap(),
        s.0.join("b/b.v").to_str().unwrap(),
        tb.to_str().unwrap(),
        "--top",
        "tb",
        "--out",
        s.0.join("result").to_str().unwrap(),
    ]);
    success(&output);
    let top = s.write(
        "top.v",
        "module top(output x,y); a aa(x); b bb(y); endmodule\n",
    );
    success(&cli(&[
        "check",
        s.0.join("a/a.v").to_str().unwrap(),
        s.0.join("b/b.v").to_str().unwrap(),
        top.to_str().unwrap(),
        "--top",
        "top",
    ]));
}

#[cfg(unix)]
#[test]
fn relative_path_entries_resolve_from_the_invoking_directory() {
    let s = Scratch::new();
    let source = hdl(&s);
    fs::create_dir(s.0.join("bin")).unwrap();
    let tool = fake(&s, "exit 0");
    fs::rename(tool, s.0.join("bin/review-yosys")).unwrap();
    let entries = std::iter::once(PathBuf::from("bin"))
        .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap()))
        .collect::<Vec<_>>();
    let output = Command::new(env!("CARGO_BIN_EXE_jsonrtl"))
        .current_dir(&s.0)
        .env("PATH", std::env::join_paths(entries).unwrap())
        .arg("check")
        .arg(source)
        .args(["--top", "top", "--tool", "yosys", "--yosys", "review-yosys"])
        .output()
        .unwrap();
    success(&output);
}

#[cfg(unix)]
#[test]
fn a_colon_in_the_invoking_directory_does_not_break_tool_lookup() {
    let s = Scratch::new();
    let directory = s.0.join("colon:cwd");
    fs::create_dir(&directory).unwrap();
    let source = hdl(&s);
    let tool = fake(&s, "exit 0");
    fs::rename(tool, directory.join("review-yosys")).unwrap();
    let entries = std::iter::once(PathBuf::from("."))
        .chain(std::env::split_paths(&std::env::var_os("PATH").unwrap()))
        .collect::<Vec<_>>();
    for executable in [
        PathBuf::from("review-yosys"),
        directory.join("review-yosys"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_jsonrtl"))
            .current_dir(&directory)
            .env("PATH", std::env::join_paths(&entries).unwrap())
            .arg("check")
            .arg(&source)
            .args(["--top", "top", "--tool", "yosys", "--yosys"])
            .arg(executable)
            .output()
            .unwrap();
        success(&output);
    }
}

#[test]
fn simulation_runs_assertions_and_publishes_waveforms() {
    let s = Scratch::new();
    let source = hdl(&s);
    let tb = s.write("tb.v", TB);
    let out = s.0.join("sim");
    let output = cli(&[
        "simulate",
        source.to_str().unwrap(),
        tb.to_str().unwrap(),
        "--top",
        "tb",
        "--out",
        out.to_str().unwrap(),
    ]);
    success(&output);
    assert!(
        fs::read_to_string(out.join("stdout.txt"))
            .unwrap()
            .contains("PASS")
    );
    assert!(
        fs::read_to_string(out.join("wave.vcd"))
            .unwrap()
            .contains("$enddefinitions")
    );
    assert!(out.join("simulation.vvp").is_file());
}

#[test]
fn fatal_and_stop_fail_without_publishing() {
    let s = Scratch::new();
    for task in ["$fatal(1,\"intentional\");", "$stop;"] {
        let tb = s.write(
            "tb.v",
            &format!("module tb; initial begin {task} end endmodule\n"),
        );
        let out = s.0.join("sim");
        let output = cli(&[
            "simulate",
            tb.to_str().unwrap(),
            "--top",
            "tb",
            "--out",
            out.to_str().unwrap(),
            "--diagnostics",
            "json",
        ]);
        assert_eq!(output.status.code(), Some(2), "{}", error(&output));
        let report: Value = serde_json::from_slice(&output.stderr).unwrap();
        assert_eq!(report["error"]["category"], "tool_failed");
        assert!(!out.exists());
    }
}

#[test]
fn infinite_simulation_stops_at_its_deadline() {
    let s = Scratch::new();
    let tb = s.write("tb.v", "module tb; reg c=0; always #1 c=~c; endmodule\n");
    let out = s.0.join("sim");
    let started = std::time::Instant::now();
    let output = cli(&[
        "simulate",
        tb.to_str().unwrap(),
        "--top",
        "tb",
        "--out",
        out.to_str().unwrap(),
        "--timeout-seconds",
        "1",
        "--diagnostics",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(!out.exists());
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    let report: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(report["error"]["category"], "tool_timeout");
}

#[cfg(unix)]
fn fake(s: &Scratch, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let p = s.write("fake-tool", &format!("#!/bin/sh\n{body}\n"));
    fs::set_permissions(&p, fs::Permissions::from_mode(0o755)).unwrap();
    p
}

#[cfg(unix)]
#[test]
fn exporter_rejects_a_tool_output_flood() {
    let s = Scratch::new();
    let source = hdl(&s);
    let out = s.0.join("dls");
    let tool = fake(&s, "head -c 2097152 /dev/zero\nexit 1");
    let output = cli(&[
        "export",
        source.to_str().unwrap(),
        "--top",
        "top",
        "--out",
        out.to_str().unwrap(),
        "--yosys",
        tool.to_str().unwrap(),
        "--diagnostics",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(!out.exists());
    let report: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(report["error"]["category"], "resource_limit");
    assert!(output.stderr.len() < 70_000);
}

#[test]
fn syntax_errors_are_tool_failures() {
    let s = Scratch::new();
    let source = s.write("top.v", "module top(input BROKEN;");
    let output = cli(&[
        "check",
        source.to_str().unwrap(),
        "--top",
        "top",
        "--diagnostics",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(2));
    let report: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(report["error"]["category"], "tool_failed");
}

#[test]
fn simulation_preserves_existing_directories() {
    let s = Scratch::new();
    let source = s.write("tb.v", "module tb; initial $finish; endmodule");
    let out = s.0.join("sim");
    fs::create_dir(&out).unwrap();
    fs::write(out.join("keep"), "sentinel").unwrap();
    let output = cli(&[
        "simulate",
        source.to_str().unwrap(),
        "--top",
        "tb",
        "--out",
        out.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(3));
    assert_eq!(fs::read_to_string(out.join("keep")).unwrap(), "sentinel");
}

fn failure_category(output: &Output, category: &str) {
    assert_eq!(output.status.code(), Some(2), "{}", error(output));
    let report: Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(report["error"]["category"], category, "{}", error(output));
}

#[test]
fn testbench_cannot_silently_overwrite_reserved_log_names() {
    let s = Scratch::new();
    let tb=s.write("tb.v","module tb; initial begin $dumpfile(\"stdout.txt\"); $dumpvars(0,tb); #1; $finish; end endmodule\n");
    let out = s.0.join("sim");
    let output = cli(&[
        "simulate",
        tb.to_str().unwrap(),
        "--top",
        "tb",
        "--out",
        out.to_str().unwrap(),
        "--diagnostics",
        "json",
    ]);
    failure_category(&output, "invalid_artifact");
    assert!(!out.exists());
}

#[test]
fn plus_arguments_reach_the_real_testbench() {
    let s = Scratch::new();
    let tb=s.write("tb.sv","module tb; integer cycles; initial begin if(!$value$plusargs(\"cycles=%d\",cycles) || cycles!=37) $fatal(1,\"bad plusarg\"); $display(\"cycles=%d\",cycles); $finish; end endmodule\n");
    let out = s.0.join("sim");
    success(&cli(&[
        "simulate",
        tb.to_str().unwrap(),
        "--top",
        "tb",
        "--system-verilog",
        "--plusarg",
        "cycles=37",
        "--out",
        out.to_str().unwrap(),
    ]));
    assert!(
        fs::read_to_string(out.join("stdout.txt"))
            .unwrap()
            .contains("37")
    );
}

#[cfg(unix)]
#[test]
fn a_descendant_holding_output_pipes_cannot_escape_the_deadline() {
    let s = Scratch::new();
    let source = hdl(&s);
    let marker = s.0.join("escaped");
    let quoted = format!("'{}'", marker.to_str().unwrap().replace('\'', "'\\''"));
    let tool = fake(
        &s,
        &format!("(sleep 2; printf leaked > {quoted}) &\nexit 0"),
    );
    let start = std::time::Instant::now();
    let output = cli(&[
        "check",
        source.to_str().unwrap(),
        "--top",
        "top",
        "--tool",
        "yosys",
        "--yosys",
        tool.to_str().unwrap(),
        "--timeout-seconds",
        "1",
        "--diagnostics",
        "json",
    ]);
    failure_category(&output, "tool_timeout");
    assert!(start.elapsed() < std::time::Duration::from_secs(3));
    std::thread::sleep(std::time::Duration::from_millis(1200));
    assert!(!marker.exists(), "background child survived cleanup");
}

#[cfg(unix)]
#[test]
fn fifo_sources_are_rejected_without_opening_a_blocking_reader() {
    let s = Scratch::new();
    let source = s.0.join("source.v");
    assert!(
        Command::new("mkfifo")
            .arg(&source)
            .status()
            .unwrap()
            .success()
    );
    let started = std::time::Instant::now();
    let output = cli(&[
        "check",
        source.to_str().unwrap(),
        "--top",
        "top",
        "--diagnostics",
        "json",
    ]);
    failure_category(&output, "invalid_file");
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}

#[cfg(unix)]
#[test]
fn generated_symlinks_are_rejected_without_publishing() {
    let s = Scratch::new();
    let source = hdl(&s);
    let out = s.0.join("synth");
    let tool = fake(&s, "ln -s /etc/passwd results/netlist.v\nexit 0");
    let output = cli(&[
        "synth",
        source.to_str().unwrap(),
        "--top",
        "top",
        "--out",
        out.to_str().unwrap(),
        "--yosys",
        tool.to_str().unwrap(),
        "--diagnostics",
        "json",
    ]);
    failure_category(&output, "invalid_artifact");
    assert!(!out.exists());
}

#[cfg(unix)]
#[test]
fn sparse_artifact_flood_is_caught_before_reading_or_publishing() {
    let s = Scratch::new();
    let source = hdl(&s);
    let out = s.0.join("synth");
    let tool = fake(
        &s,
        "for name in a b c d e; do truncate -s 33554432 results/$name; done\nexit 0",
    );
    let output = cli(&[
        "synth",
        source.to_str().unwrap(),
        "--top",
        "top",
        "--out",
        out.to_str().unwrap(),
        "--yosys",
        tool.to_str().unwrap(),
        "--diagnostics",
        "json",
    ]);
    failure_category(&output, "resource_limit");
    assert!(!out.exists());
}

#[cfg(unix)]
#[test]
fn publication_race_preserves_an_empty_destination() {
    let s = Scratch::new();
    let source = hdl(&s);
    let out = s.0.join("synth");
    let quoted = format!("'{}'", out.to_str().unwrap().replace('\'', "'\\''"));
    let tool = fake(
        &s,
        &format!("yosys \"$@\"\nresult=$?\nmkdir {quoted}\nexit \"$result\""),
    );
    let output = cli(&[
        "synth",
        source.to_str().unwrap(),
        "--top",
        "top",
        "--out",
        out.to_str().unwrap(),
        "--yosys",
        tool.to_str().unwrap(),
        "--diagnostics",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(3), "{}", error(&output));
    assert!(out.is_dir());
    assert_eq!(fs::read_dir(&out).unwrap().count(), 0);
}

#[test]
fn checks_and_simulation_handle_quoted_source_and_include_paths() {
    let s = Scratch::new();
    let folder = s.0.join("source quote\"; and space");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("logic.vh"), "`define OP (a ^ b)\n").unwrap();
    let source = folder.join("top $literal.v");
    fs::write(
        &source,
        "`include \"logic.vh\"\nmodule top(input a,b,output y); assign y=`OP; endmodule\n",
    )
    .unwrap();
    success(&cli(&["check", source.to_str().unwrap(), "--top", "top"]));
    let tb = s.write("tb.v", TB);
    let out = s.0.join("simulation");
    success(&cli(&[
        "simulate",
        source.to_str().unwrap(),
        tb.to_str().unwrap(),
        "--top",
        "tb",
        "--out",
        out.to_str().unwrap(),
    ]));
}

#[test]
fn missing_tool_status_is_reported_without_hiding_available_tools() {
    let s = Scratch::new();
    let missing = s.0.join("missing-yosys");
    let output = cli(&[
        "tools",
        "--yosys",
        missing.to_str().unwrap(),
        "--diagnostics",
        "json",
    ]);
    assert_eq!(output.status.code(), Some(3));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["tools"]["yosys"]["available"], false);
    assert_eq!(report["tools"]["iverilog"]["available"], true);
    assert_eq!(report["tools"]["vvp"]["available"], true);
}

#[test]
fn synthesized_register_netlist_matches_the_original_testbench() {
    let s = Scratch::new();
    let repo = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let source = repo.join("examples/verilog/counter.v");
    let tb = repo.join("examples/verilog/counter_tb.v");
    let synth = s.0.join("synth");
    success(&cli(&[
        "synth",
        source.to_str().unwrap(),
        "--top",
        "counter",
        "--out",
        synth.to_str().unwrap(),
    ]));
    for (name, design) in [("original", source), ("mapped", synth.join("netlist.v"))] {
        let out = s.0.join(name);
        success(&cli(&[
            "simulate",
            design.to_str().unwrap(),
            tb.to_str().unwrap(),
            "--top",
            "counter_tb",
            "--out",
            out.to_str().unwrap(),
        ]));
        assert!(
            fs::read_to_string(out.join("stdout.txt"))
                .unwrap()
                .contains("PASS: counter capture")
        );
    }
}

#[test]
fn generic_synthesis_preserves_supported_initial_state() {
    let s = Scratch::new();
    let source = s.write(
        "top.v",
        "module top(input clk,d,output reg q=1'b1); always @(posedge clk) q<=d; endmodule\n",
    );
    let out = s.0.join("synth");
    success(&cli(&[
        "synth",
        source.to_str().unwrap(),
        "--top",
        "top",
        "--out",
        out.to_str().unwrap(),
    ]));
    let tb=s.write("tb.v","module tb; reg clk=0,d=0; wire q; top dut(clk,d,q); initial begin #1; if(q!==1) $fatal(1,\"init lost\"); clk=1; #1; if(q!==0) $fatal(1,\"capture lost\"); $finish; end endmodule\n");
    let sim = s.0.join("sim");
    success(&cli(&[
        "simulate",
        out.join("netlist.v").to_str().unwrap(),
        tb.to_str().unwrap(),
        "--top",
        "tb",
        "--out",
        sim.to_str().unwrap(),
    ]));
}

#[cfg(unix)]
#[test]
fn compilation_and_runtime_share_one_deadline() {
    use std::os::unix::fs::PermissionsExt;
    let s = Scratch::new();
    let tb = s.write("tb.v", "module tb; initial $finish; endmodule");
    let out = s.0.join("sim");
    let compiler = fake(&s, "sleep 1.2\nexec iverilog \"$@\"");
    let runtime = s.write("runtime", "#!/bin/sh\nsleep 1.2\nexec vvp \"$@\"\n");
    fs::set_permissions(&runtime, fs::Permissions::from_mode(0o755)).unwrap();
    let began = std::time::Instant::now();
    let output = cli(&[
        "simulate",
        tb.to_str().unwrap(),
        "--top",
        "tb",
        "--out",
        out.to_str().unwrap(),
        "--iverilog",
        compiler.to_str().unwrap(),
        "--vvp",
        runtime.to_str().unwrap(),
        "--timeout-seconds",
        "2",
        "--diagnostics",
        "json",
    ]);
    failure_category(&output, "tool_timeout");
    assert!(!out.exists());
    assert!(began.elapsed() < std::time::Duration::from_secs(4));
}

#[cfg(unix)]
#[test]
fn quoted_relative_executables_are_not_interpreted_by_a_shell() {
    let s = Scratch::new();
    let source = hdl(&s);
    let tool = fake(&s, "exec yosys \"$@\"");
    let name = "tool $(touch INJECTED) quote\";";
    fs::rename(tool, s.0.join(name)).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_jsonrtl"))
        .current_dir(&s.0)
        .args([
            "check",
            source.to_str().unwrap(),
            "--top",
            "top",
            "--tool",
            "yosys",
            "--yosys",
            &format!("./{name}"),
        ])
        .output()
        .unwrap();
    success(&output);
    assert!(!s.0.join("INJECTED").exists());
}

#[test]
fn module_name_script_injection_is_rejected() {
    let s = Scratch::new();
    let source = hdl(&s);
    let output = cli(&[
        "check",
        source.to_str().unwrap(),
        "--top",
        "top; shell touch INJECTED",
        "--diagnostics",
        "json",
    ]);
    failure_category(&output, "invalid_top");
    assert!(!s.0.join("INJECTED").exists());
}

#[test]
fn excessive_source_count_is_rejected_before_tools_run() {
    let s = Scratch::new();
    let source = hdl(&s);
    let mut args = vec!["check", "--top", "top", "--diagnostics", "json"];
    args.extend(std::iter::repeat_n(source.to_str().unwrap(), 257));
    failure_category(&cli(&args), "resource_limit");
}

#[test]
fn simulation_output_flood_is_a_resource_failure() {
    let s = Scratch::new();
    let tb=s.write("tb.v","module tb; integer i; initial begin for(i=0;i<100000;i=i+1) $display(\"12345678901234567890123456789012345678901234567890\"); $finish; end endmodule\n");
    let out = s.0.join("sim");
    let output = cli(&[
        "simulate",
        tb.to_str().unwrap(),
        "--top",
        "tb",
        "--out",
        out.to_str().unwrap(),
        "--diagnostics",
        "json",
    ]);
    failure_category(&output, "resource_limit");
    assert!(!out.exists());
}

#[test]
fn testbench_cannot_publish_a_modified_simulation_program() {
    let s = Scratch::new();
    let tb=s.write("tb.v","module tb; integer f; initial begin f=$fopen(\"simulation.vvp\",\"w\"); $fdisplay(f,\"modified\"); $fclose(f); $finish; end endmodule\n");
    let out = s.0.join("sim");
    let output = cli(&[
        "simulate",
        tb.to_str().unwrap(),
        "--top",
        "tb",
        "--out",
        out.to_str().unwrap(),
        "--diagnostics",
        "json",
    ]);
    failure_category(&output, "invalid_artifact");
    assert!(!out.exists());
}
