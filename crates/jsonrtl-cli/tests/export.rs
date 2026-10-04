use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

use serde_json::Value;

static NEXT: AtomicU64 = AtomicU64::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "jsonrtl-export-cli-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn source(&self, name: &str, verilog: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, verilog).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_jsonrtl"))
        .args(args)
        .output()
        .unwrap()
}

fn export(source: &Path, destination: &Path, extra: &[&str]) -> Output {
    let mut args = vec![
        "export",
        source.to_str().unwrap(),
        "--top",
        "top",
        "--out",
        destination.to_str().unwrap(),
    ];
    args.extend(extra);
    run(&args)
}

fn stderr(result: &Output) -> String {
    String::from_utf8_lossy(&result.stderr).into_owned()
}

#[test]
fn exports_a_native_project_that_round_trips_through_import() {
    let scratch = Scratch::new();
    let source = scratch.source(
        "top.v",
        "module top(input a, b, output y); assign y = a ^ b; endmodule\n",
    );
    let destination = scratch.0.join("project");
    let result = export(&source, &destination, &[]);
    assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
    assert!(result.stdout.is_empty(), "diagnostics belong on stderr");
    let metadata: Value =
        serde_json::from_slice(&fs::read(destination.join("ProjectDescription.json")).unwrap())
            .unwrap();
    assert_eq!(metadata["ProjectName"], "project");
    let names = metadata["AllCustomChipNames"].as_array().unwrap();
    assert!(names.iter().any(|name| name == "top"));
    for name in names {
        assert!(
            destination
                .join("Chips")
                .join(format!("{}.json", name.as_str().unwrap()))
                .is_file()
        );
    }
    let imported = run(&[
        "import",
        destination.to_str().unwrap(),
        "--chip",
        "top",
        "--stdout",
    ]);
    assert_eq!(imported.status.code(), Some(0), "{}", stderr(&imported));
    assert!(String::from_utf8_lossy(&imported.stdout).contains("module top"));
}

#[test]
fn syntax_errors_leave_no_project_directory() {
    let scratch = Scratch::new();
    let source = scratch.source("top.v", "module top(input a; BROKEN endmodule");
    let destination = scratch.0.join("project");
    let result = export(&source, &destination, &[]);
    assert_eq!(result.status.code(), Some(2), "{}", stderr(&result));
    assert!(stderr(&result).contains("synthesis"), "{}", stderr(&result));
    assert!(!destination.exists());
}

#[test]
fn existing_destination_is_preserved() {
    let scratch = Scratch::new();
    let source = scratch.source("top.v", "module top(output y); assign y = 1'b1; endmodule");
    let destination = scratch.0.join("project");
    fs::create_dir(&destination).unwrap();
    fs::write(destination.join("keep.txt"), "sentinel").unwrap();
    let result = export(&source, &destination, &[]);
    assert_eq!(result.status.code(), Some(3), "{}", stderr(&result));
    assert_eq!(
        fs::read_to_string(destination.join("keep.txt")).unwrap(),
        "sentinel"
    );
    assert_eq!(fs::read_dir(destination).unwrap().count(), 1);
}

#[test]
fn unavailable_export_profile_names_supported_format() {
    let scratch = Scratch::new();
    let source = scratch.source("top.v", "module top(); endmodule");
    let destination = scratch.0.join("project");
    let result = export(&source, &destination, &["--profile", "logisim"]);
    assert_eq!(result.status.code(), Some(2));
    assert!(stderr(&result).contains("dls"), "{}", stderr(&result));
    assert!(!destination.exists());
}

#[test]
fn missing_yosys_is_a_structured_error_without_output() {
    let scratch = Scratch::new();
    let source = scratch.source(
        "top.v",
        "module top(input a, output y); assign y=a; endmodule",
    );
    let destination = scratch.0.join("project");
    let missing = scratch.0.join("missing-yosys");
    let result = export(
        &source,
        &destination,
        &[
            "--yosys",
            missing.to_str().unwrap(),
            "--diagnostics",
            "json",
        ],
    );
    assert_eq!(result.status.code(), Some(3));
    let envelope: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(envelope["stage"], "export");
    assert_eq!(envelope["error"]["category"], "synthesis_tool");
    assert!(!destination.exists());
}

#[test]
fn multiple_sources_and_relative_includes_work_with_quoted_paths() {
    let scratch = Scratch::new();
    let folder = scratch.0.join("space and quote\";dir");
    fs::create_dir(&folder).unwrap();
    fs::write(folder.join("logic.vh"), "`define OP (a ^ b)\n").unwrap();
    let child = folder.join("child.v");
    fs::write(
        &child,
        "`include \"logic.vh\"\nmodule child(input a,b, output y); assign y=`OP; endmodule\n",
    )
    .unwrap();
    let source = scratch.source(
        "top; $ literal.v",
        "module top(input a,b, output y); child c(a,b,y); endmodule\n",
    );
    let destination = scratch.0.join("project");
    let result = run(&[
        "export",
        source.to_str().unwrap(),
        child.to_str().unwrap(),
        "--top",
        "top",
        "--out",
        destination.to_str().unwrap(),
    ]);
    assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
    assert!(destination.join("Chips/top.json").is_file());
}

#[test]
fn wrong_top_and_script_punctuation_are_rejected_without_output() {
    let scratch = Scratch::new();
    let source = scratch.source(
        "top.v",
        "module top(input a,output y); assign y=a; endmodule\n",
    );
    for top in ["missing", "top; write_json injected.json", "top\nexit"] {
        let destination = scratch.0.join("project");
        let result = run(&[
            "export",
            source.to_str().unwrap(),
            "--top",
            top,
            "--out",
            destination.to_str().unwrap(),
        ]);
        assert_eq!(result.status.code(), Some(2), "{}", stderr(&result));
        assert!(!destination.exists());
        assert!(!scratch.0.join("injected.json").exists());
    }
}

#[test]
fn sequential_export_and_clock_wrapper_create_a_complete_project() {
    let scratch = Scratch::new();
    let source = scratch.source("top.v", "module top(input clk,reset,en,output reg [7:0] q); always @(posedge clk or posedge reset) if(reset) q<=0; else if(en) q<=q+1; endmodule\n");
    let destination = scratch.0.join("project");
    let result = export(
        &source,
        &destination,
        &["--clock", "clk", "--diagnostics", "json"],
    );
    assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
    let envelope: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(envelope["success"], true);
    let top = envelope["topChip"].as_str().unwrap();
    let chip: Value = serde_json::from_slice(
        &fs::read(destination.join("Chips").join(format!("{top}.json"))).unwrap(),
    )
    .unwrap();
    assert!(
        chip["SubChips"]
            .as_array()
            .unwrap()
            .iter()
            .any(|sub| sub["Name"] == "CLOCK")
    );
    assert!(
        !chip["InputPins"]
            .as_array()
            .unwrap()
            .iter()
            .any(|pin| pin["Name"] == "clk")
    );
    let description: Value =
        serde_json::from_slice(&fs::read(destination.join("ProjectDescription.json")).unwrap())
            .unwrap();
    assert_eq!(description["Prefs_SimStepsPerClockTick"], 250);
    for name in description["AllCustomChipNames"].as_array().unwrap() {
        assert!(
            destination
                .join("Chips")
                .join(format!("{}.json", name.as_str().unwrap()))
                .is_file()
        );
    }
}

#[test]
fn explicit_initial_state_and_high_impedance_are_not_silently_changed() {
    let scratch = Scratch::new();
    for source_text in [
        "module top(input clk,d, output reg q=1'b1); always @(posedge clk) q<=d; endmodule\n",
        "module top(output y); assign y=1'bz; endmodule\n",
        "module top(input sel, output y); assign y=sel?1'bx:1'b0; endmodule\n",
        "module top(input sel,d, output y); assign y=sel?d:1'bz; endmodule\n",
        "module top(input clk,rst,d, output reg q); always @(posedge clk or posedge rst) if(rst) q<=1'bx; else q<=d; endmodule\n",
        "module top(input [1:0] s, input a,b,c,d, output reg y); always @* case(s) 2'd0:y=a;2'd1:y=b;2'd2:y=c;default:y=1'bx;endcase endmodule\n",
    ] {
        let source = scratch.source("top.v", source_text);
        let destination = scratch.0.join("project");
        let result = export(&source, &destination, &[]);
        assert_eq!(result.status.code(), Some(2), "{}", stderr(&result));
        assert!(!destination.exists());
    }
}

#[test]
fn source_limit_is_checked_before_synthesis() {
    let scratch = Scratch::new();
    let source = scratch.0.join("huge.v");
    fs::File::create(&source)
        .unwrap()
        .set_len(8 * 1024 * 1024 + 1)
        .unwrap();
    let destination = scratch.0.join("project");
    let result = export(&source, &destination, &["--diagnostics", "json"]);
    assert_eq!(result.status.code(), Some(2));
    let envelope: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(envelope["error"]["category"], "resource_limit");
    assert!(!destination.exists());
}

#[test]
fn repeated_export_produces_identical_chip_and_metadata_bytes() {
    let scratch = Scratch::new();
    let source = scratch.source(
        "top.v",
        "module top(input [7:0] a,b,output [7:0] y); assign y=a^b; endmodule\n",
    );
    let first = scratch.0.join("first/project");
    let second = scratch.0.join("second/project");
    for destination in [&first, &second] {
        let result = export(&source, destination, &[]);
        assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
    }
    assert_eq!(
        fs::read(first.join("ProjectDescription.json")).unwrap(),
        fs::read(second.join("ProjectDescription.json")).unwrap()
    );
    for entry in fs::read_dir(first.join("Chips")).unwrap() {
        let entry = entry.unwrap();
        assert_eq!(
            fs::read(entry.path()).unwrap(),
            fs::read(second.join("Chips").join(entry.file_name())).unwrap()
        );
    }
}

#[cfg(unix)]
fn fake_yosys(scratch: &Scratch, script: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let path = scratch.source("fake-yosys", &format!("#!/bin/sh\n{script}\n"));
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

#[test]
fn system_verilog_frontend_exports_always_ff_registers() {
    let scratch = Scratch::new();
    let source = scratch.source(
        "top.sv",
        "module top(input logic clk, rst, d, output logic q);\n\
         always_ff @(posedge clk or posedge rst)\n\
             if (rst) q <= 1'b0; else q <= d;\n\
         endmodule\n",
    );
    let destination = scratch.0.join("project");
    let result = export(&source, &destination, &["--system-verilog"]);
    assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
    let top: Value =
        serde_json::from_slice(&fs::read(destination.join("Chips/top.json")).unwrap()).unwrap();
    assert!(top["SubChips"].as_array().unwrap().iter().any(|chip| {
        chip["Name"]
            .as_str()
            .is_some_and(|name| name.starts_with("JSONRTL_DFF"))
    }));
}

#[test]
fn case_mux_with_defined_default_remains_supported() {
    let scratch = Scratch::new();
    let source = scratch.source(
        "top.v",
        "module top(input [1:0] s, input a,b,c,d, output reg y);\n\
         always @* case(s)\n\
           2'd0:y=a; 2'd1:y=b; 2'd2:y=c; default:y=d;\n\
         endcase\n\
         endmodule\n",
    );
    let destination = scratch.0.join("project");
    let result = export(&source, &destination, &[]);
    assert_eq!(result.status.code(), Some(0), "{}", stderr(&result));
    assert!(destination.join("Chips/top.json").is_file());
}

#[cfg(unix)]
#[test]
fn relative_synthesis_executable_is_resolved_from_invoking_directory() {
    let scratch = Scratch::new();
    let source = scratch.source("top.v", "module top(); endmodule");
    fake_yosys(&scratch, "printf 'relative tool executed\\n' >&2\nexit 1");
    let destination = scratch.0.join("project");
    let result = Command::new(env!("CARGO_BIN_EXE_jsonrtl"))
        .current_dir(&scratch.0)
        .args([
            "export",
            source.to_str().unwrap(),
            "--top",
            "top",
            "--out",
            destination.to_str().unwrap(),
            "--yosys",
            "./fake-yosys",
            "--diagnostics",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(result.status.code(), Some(2), "{}", stderr(&result));
    let envelope: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(envelope["error"]["category"], "synthesis_failed");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("relative tool executed")
    );
    assert!(!destination.exists());
}

#[cfg(unix)]
#[test]
fn synthesis_timeout_is_reported_and_output_is_absent() {
    let scratch = Scratch::new();
    let source = scratch.source("top.v", "module top(); endmodule");
    let tool = fake_yosys(&scratch, "sleep 60");
    let destination = scratch.0.join("project");
    let started = std::time::Instant::now();
    let result = export(
        &source,
        &destination,
        &[
            "--yosys",
            tool.to_str().unwrap(),
            "--timeout-seconds",
            "1",
            "--diagnostics",
            "json",
        ],
    );
    assert_eq!(result.status.code(), Some(2), "{}", stderr(&result));
    let envelope: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(envelope["error"]["category"], "synthesis_timeout");
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert!(!destination.exists());
}

#[cfg(unix)]
#[test]
fn synthesis_failure_diagnostics_are_bounded_and_valid_json() {
    let scratch = Scratch::new();
    let source = scratch.source("top.v", "module top(); endmodule");
    let tool = fake_yosys(&scratch, "printf 'tool failure\\n' >&2\nexit 1");
    let destination = scratch.0.join("project");
    let result = export(
        &source,
        &destination,
        &["--yosys", tool.to_str().unwrap(), "--diagnostics", "json"],
    );
    assert_eq!(result.status.code(), Some(2));
    let envelope: Value = serde_json::from_slice(&result.stderr).unwrap();
    assert_eq!(envelope["error"]["category"], "synthesis_failed");
    assert!(
        envelope["error"]["message"]
            .as_str()
            .unwrap()
            .contains("tool failure")
    );
    assert!(!destination.exists());
}
