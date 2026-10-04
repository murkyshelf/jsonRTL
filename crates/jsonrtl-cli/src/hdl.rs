//! User-facing synthesis, compiler checks and test-bench simulation.
use crate::{
    CliFailure, DiagnosticFormat, EXIT_IO, io_failure, render_failure,
    toolchain::{self, Job, ScratchDirectory, Sources},
};
use clap::{Args, ValueEnum};
use serde_json::{Value, json};
use std::{collections::BTreeSet, ffi::OsString, fs, path::PathBuf};

#[derive(Debug, Args)]
pub struct Input {
    /// Verilog files containing the selected module and its dependencies.
    #[arg(required=true,num_args=1..)]
    sources: Vec<PathBuf>,
    /// Design module to check/synthesize, or test-bench module to simulate.
    #[arg(long)]
    top: String,
    /// Enable the installed compiler's SystemVerilog subset.
    #[arg(long)]
    system_verilog: bool,
    /// Shared deadline for all compiler/runtime stages, in seconds.
    #[arg(long,default_value_t=60,value_parser=clap::value_parser!(u64).range(1..=3600))]
    timeout_seconds: u64,
}
#[derive(Debug, Clone, Copy, ValueEnum)]
enum CheckTool {
    Both,
    Yosys,
    Iverilog,
}
#[derive(Debug, Args)]
pub struct CheckArgs {
    #[command(flatten)]
    input: Input,
    /// Compiler(s) used to check the selected design.
    #[arg(long,value_enum,default_value_t=CheckTool::Both)]
    tool: CheckTool,
    #[arg(long, default_value = "yosys", value_name = "PATH")]
    yosys: PathBuf,
    #[arg(long, default_value = "iverilog", value_name = "PATH")]
    iverilog: PathBuf,
}
#[derive(Debug, Args)]
pub struct SynthArgs {
    #[command(flatten)]
    input: Input,
    /// New directory for gate netlist Verilog/JSON and synthesis statistics.
    #[arg(long, value_name = "DIR")]
    out: PathBuf,
    #[arg(long, default_value = "yosys", value_name = "PATH")]
    yosys: PathBuf,
}
#[derive(Debug, Args)]
pub struct SimulateArgs {
    #[command(flatten)]
    input: Input,
    /// New directory for the compiled test bench, logs and generated waveforms.
    #[arg(long, value_name = "DIR")]
    out: PathBuf,
    #[arg(long, default_value = "iverilog", value_name = "PATH")]
    iverilog: PathBuf,
    #[arg(long, default_value = "vvp", value_name = "PATH")]
    vvp: PathBuf,
    /// Test-bench plus argument, e.g. --plusarg cycles=100. Repeat as needed.
    #[arg(long, value_name = "VALUE")]
    plusarg: Vec<String>,
}
#[derive(Debug, Args)]
pub struct ToolsArgs {
    #[arg(long, default_value = "yosys", value_name = "PATH")]
    yosys: PathBuf,
    #[arg(long, default_value = "iverilog", value_name = "PATH")]
    iverilog: PathBuf,
    #[arg(long, default_value = "vvp", value_name = "PATH")]
    vvp: PathBuf,
    /// Maximum version-probe time per tool, in seconds.
    #[arg(long,default_value_t=5,value_parser=clap::value_parser!(u64).range(1..=3600))]
    timeout_seconds: u64,
}

fn render(result: Result<Value, CliFailure>, format: DiagnosticFormat) -> Result<(), u8> {
    let value = result.map_err(|e| {
        render_failure(format, &e);
        e.exit_code
    })?;
    match format {
        DiagnosticFormat::Json => eprintln!("{value}"),
        DiagnosticFormat::Human => {
            let command = value["command"].as_str().unwrap_or("hdl");
            if let Some(path) = value["outputDir"].as_str() {
                eprintln!("{command}: saved results to '{path}'");
            } else {
                eprintln!(
                    "{command}: top '{}' passed {}",
                    value["top"].as_str().unwrap_or(""),
                    value["checkedWith"]
                );
            }
        }
    }
    Ok(())
}
fn prepare(
    input: &Input,
    root: &std::path::Path,
    stage: &'static str,
) -> Result<Sources, CliFailure> {
    toolchain::validate_top(&input.top, stage)?;
    Sources::prepare(&input.sources, root, input.system_verilog, stage)
}
fn yosys(
    job: &Job<'_>,
    sources: &Sources,
    root: &std::path::Path,
    executable: &std::path::Path,
    body: &str,
) -> Result<toolchain::ToolOutput, CliFailure> {
    toolchain::write(&root.join("script.ys"), sources.yosys_script(body), "yosys")?;
    job.run(
        "Yosys",
        executable,
        &[
            "-Q".into(),
            "-T".into(),
            "-q".into(),
            "-s".into(),
            "script.ys".into(),
        ],
        root,
        &sources.allowed_links,
    )
}

pub fn check(args: CheckArgs, format: DiagnosticFormat) -> Result<(), u8> {
    render(check_inner(&args), format)
}
fn check_inner(args: &CheckArgs) -> Result<Value, CliFailure> {
    let workspace = ScratchDirectory::temporary("check")?;
    let job = Job::new("check", &workspace.path, args.input.timeout_seconds);
    let sources = prepare(&args.input, &workspace.path, "check")?;
    let mut checked = Vec::new();
    if matches!(args.tool, CheckTool::Both | CheckTool::Iverilog) {
        job.run(
            "Icarus Verilog",
            &args.iverilog,
            &sources.icarus_args(&args.input.top, args.input.system_verilog, None),
            &workspace.path,
            &sources.allowed_links,
        )?;
        checked.push("iverilog");
    }
    if matches!(args.tool, CheckTool::Both | CheckTool::Yosys) {
        yosys(
            &job,
            &sources,
            &workspace.path,
            &args.yosys,
            &format!(
                "hierarchy -check -top {}\nproc\nflatten\nopt_clean\ncheck -assert\n",
                args.input.top
            ),
        )?;
        checked.push("yosys");
    }
    Ok(json!({"success":true,"command":"check","top":args.input.top,"checkedWith":checked}))
}

pub fn synth(args: SynthArgs, format: DiagnosticFormat) -> Result<(), u8> {
    render(synth_inner(&args), format)
}
fn synth_inner(args: &SynthArgs) -> Result<Value, CliFailure> {
    toolchain::new_destination(&args.out, "synth")?;
    let workspace = ScratchDirectory::temporary("synth")?;
    let job = Job::new("synth", &workspace.path, args.input.timeout_seconds);
    let sources = prepare(&args.input, &workspace.path, "synth")?;
    let artifacts = workspace.path.join("results");
    fs::create_dir(&artifacts).map_err(|e| io_failure("synth", e))?;
    let output = yosys(
        &job,
        &sources,
        &workspace.path,
        &args.yosys,
        &format!(
            "synth -top {} -flatten\ncheck -assert\nwrite_json results/netlist.json\nwrite_verilog -noattr results/netlist.v\ntee -o results/statistics.json stat -json\n",
            args.input.top
        ),
    )?;
    // Required artifacts must be regular, complete and nonempty. Parse the
    // JSON rather than mistaking a successful executable exit for synthesis.
    for name in ["netlist.json", "statistics.json"] {
        let bytes = toolchain::read_regular(
            &artifacts.join(name),
            toolchain::MAX_FILE_BYTES,
            "synth",
            true,
        )?;
        let value: Value = serde_json::from_slice(&bytes).map_err(|e| {
            toolchain::failure(
                "synth",
                "invalid_artifact",
                format!("invalid {name}: {e}"),
                crate::EXIT_INVALID,
            )
        })?;
        if !value["modules"].is_object() {
            return Err(toolchain::failure(
                "synth",
                "invalid_artifact",
                format!("{name} has no modules"),
                crate::EXIT_INVALID,
            ));
        }
        if name == "netlist.json" && !value["modules"][&args.input.top].is_object() {
            return Err(toolchain::failure(
                "synth",
                "invalid_artifact",
                "synthesized netlist is missing the selected top",
                crate::EXIT_INVALID,
            ));
        }
    }
    let verilog = toolchain::read_regular(
        &artifacts.join("netlist.v"),
        toolchain::MAX_FILE_BYTES,
        "synth",
        true,
    )?;
    if verilog.is_empty() || std::str::from_utf8(&verilog).is_err() {
        return Err(toolchain::failure(
            "synth",
            "invalid_artifact",
            "synthesized Verilog must be nonempty UTF-8 text",
            crate::EXIT_INVALID,
        ));
    }
    toolchain::write(&artifacts.join("report.log"), output.text(), "synth")?;
    let files = toolchain::copy_artifacts(&artifacts, &args.out, "synth")?;
    Ok(
        json!({"success":true,"command":"synth","top":args.input.top,"outputDir":args.out,"files":files}),
    )
}

pub fn simulate(args: SimulateArgs, format: DiagnosticFormat) -> Result<(), u8> {
    render(simulate_inner(&args), format)
}
fn simulate_inner(args: &SimulateArgs) -> Result<Value, CliFailure> {
    toolchain::new_destination(&args.out, "simulate")?;
    if args.plusarg.len() > 256
        || args
            .plusarg
            .iter()
            .any(|a| a.len() > 4096 || a.chars().any(char::is_control))
    {
        return Err(toolchain::failure(
            "simulate",
            "resource_limit",
            "at most 256 plus arguments of 4096 bytes without control characters are supported",
            crate::EXIT_INVALID,
        ));
    }
    let workspace = ScratchDirectory::temporary("simulate")?;
    let job = Job::new("simulate", &workspace.path, args.input.timeout_seconds);
    let sources = prepare(&args.input, &workspace.path, "simulate")?;
    let artifacts = workspace.path.join("results");
    fs::create_dir(&artifacts).map_err(|e| io_failure("simulate", e))?;
    let compiler = job.run(
        "Icarus Verilog",
        &args.iverilog,
        &sources.icarus_args(
            &args.input.top,
            args.input.system_verilog,
            Some("results/simulation.vvp"),
        ),
        &workspace.path,
        &sources.allowed_links,
    )?;
    let image = toolchain::read_regular(
        &artifacts.join("simulation.vvp"),
        toolchain::MAX_FILE_BYTES,
        "simulate",
        true,
    )?;
    if image.is_empty() {
        return Err(toolchain::failure(
            "simulate",
            "invalid_artifact",
            "Icarus produced an empty simulation program",
            crate::EXIT_INVALID,
        ));
    }
    let mut runtime_args: Vec<OsString> = vec!["-N".into(), "-i".into(), "simulation.vvp".into()];
    runtime_args.extend(
        args.plusarg
            .iter()
            .map(|a| OsString::from(format!("+{}", a.trim_start_matches('+')))),
    );
    let runtime = job.run(
        "VVP",
        &args.vvp,
        &runtime_args,
        &artifacts,
        &sources.allowed_links,
    )?;
    if toolchain::read_regular(
        &artifacts.join("simulation.vvp"),
        toolchain::MAX_FILE_BYTES,
        "simulate",
        true,
    )? != image
    {
        return Err(toolchain::failure(
            "simulate",
            "invalid_artifact",
            "test bench modified its compiled simulation program",
            crate::EXIT_INVALID,
        ));
    }
    for name in ["compile.log", "stdout.txt", "stderr.txt"] {
        if toolchain::exists(&artifacts.join(name)).map_err(|e| io_failure("simulate", e))? {
            return Err(toolchain::failure(
                "simulate",
                "invalid_artifact",
                format!(
                    "test bench created reserved artifact '{name}'; choose a different filename"
                ),
                crate::EXIT_INVALID,
            ));
        }
    }
    toolchain::write(&artifacts.join("compile.log"), compiler.text(), "simulate")?;
    toolchain::write(&artifacts.join("stdout.txt"), &runtime.stdout, "simulate")?;
    toolchain::write(&artifacts.join("stderr.txt"), &runtime.stderr, "simulate")?;
    let files = toolchain::copy_artifacts(&artifacts, &args.out, "simulate")?;
    Ok(
        json!({"success":true,"command":"simulate","top":args.input.top,"outputDir":args.out,"files":files,"runtime":"vvp"}),
    )
}

pub fn tools(args: ToolsArgs, format: DiagnosticFormat) -> Result<(), u8> {
    let workspace = ScratchDirectory::temporary("tools").map_err(|e| {
        render_failure(format, &e);
        e.exit_code
    })?;
    let mut entries = serde_json::Map::new();
    let mut all = true;
    for (name, path, prefix) in [
        ("yosys", &args.yosys, "Yosys "),
        ("iverilog", &args.iverilog, "Icarus Verilog version "),
        ("vvp", &args.vvp, "Icarus Verilog runtime version "),
    ] {
        let job = Job::new("tools", &workspace.path, args.timeout_seconds);
        let result = job.run(
            name,
            path,
            &["-V".into()],
            &workspace.path,
            &BTreeSet::new(),
        );
        let entry = match result {
            Ok(output) => {
                let text = output.text();
                if let Some(version) = text.lines().find(|l| l.starts_with(prefix)) {
                    json!({"available":true,"executable":path.to_string_lossy(),"version":version})
                } else {
                    all = false;
                    json!({"available":false,"executable":path.to_string_lossy(),"error":"tool did not return a recognized version"})
                }
            }
            Err(error) => {
                all = false;
                json!({"available":false,"executable":path.to_string_lossy(),"category":error.category,"error":error.message})
            }
        };
        entries.insert(name.into(), entry);
    }
    let report = json!({"success":all,"command":"tools","tools":entries});
    match format {
        DiagnosticFormat::Json => println!("{report}"),
        DiagnosticFormat::Human => {
            for (name, entry) in entries {
                if entry["available"] == true {
                    println!("{name}: {}", entry["version"].as_str().unwrap_or(""));
                } else {
                    println!(
                        "{name}: unavailable — {}",
                        entry["error"].as_str().unwrap_or("")
                    );
                }
            }
        }
    }
    if all { Ok(()) } else { Err(EXIT_IO) }
}
