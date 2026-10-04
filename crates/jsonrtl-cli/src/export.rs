//! Verilog synthesis and native DLS project publication.
use crate::{
    CliFailure, DiagnosticFormat, EXIT_INVALID, EXIT_IO, io_failure, profile_failure,
    render_failure,
    toolchain::{self, Job, ScratchDirectory, Sources},
};
use clap::Args;
use jsonrtl_profiles::{dls::export::ExportProject, is_safe_unit_name};
use serde_json::json;
use std::{fs, path::PathBuf};

#[derive(Debug, Args)]
pub(super) struct ExportArgs {
    /// Verilog source files. The selected top and its dependencies must be present.
    #[arg(required = true, num_args = 1..)]
    sources: Vec<PathBuf>,

    /// Name of the Verilog module to export.
    #[arg(long)]
    top: String,

    /// Output format. Currently only Digital-Logic-Sim is supported.
    #[arg(long, default_value = "dls")]
    profile: String,

    /// New project directory to create. Existing paths are never overwritten.
    #[arg(long, value_name = "DIR")]
    out: PathBuf,

    /// Add a ready-to-run wrapper connecting DLS's CLOCK to this single-bit input.
    #[arg(long, value_name = "PORT")]
    clock: Option<String>,

    /// Enable the SystemVerilog subset supported by Yosys.
    #[arg(long)]
    system_verilog: bool,

    /// Yosys executable to use for synthesis.
    #[arg(long, default_value = "yosys", value_name = "PATH")]
    yosys: PathBuf,

    /// Maximum synthesis time in seconds.
    #[arg(long, default_value_t = 60, value_parser = clap::value_parser!(u64).range(1..=3600))]
    timeout_seconds: u64,
}

pub(super) fn run(arguments: ExportArgs, format: DiagnosticFormat) -> Result<(), u8> {
    let result = export_project(&arguments).map_err(|failure| {
        let code = failure.exit_code;
        render_failure(format, &failure);
        code
    })?;
    match format {
        DiagnosticFormat::Human => eprintln!(
            "export: saved Digital-Logic-Sim project to '{}'; open chip '{}' ({} chip files)",
            arguments.out.display(),
            result.top_chip,
            result.chips.len()
        ),
        DiagnosticFormat::Json => eprintln!(
            "{}",
            json!({
                "success": true,
                "command": "export",
                "profile": "dls",
                "topChip": result.top_chip,
                "outputDir": arguments.out,
                "chips": result.chips.keys().collect::<Vec<_>>(),
            })
        ),
    }
    Ok(())
}

fn failure(category: &'static str, message: impl Into<String>, code: u8) -> CliFailure {
    toolchain::failure("export", category, message, code)
}

fn export_project(arguments: &ExportArgs) -> Result<ExportProject, CliFailure> {
    if arguments.profile != "dls" {
        return Err(failure(
            "unsupported_export_profile",
            format!(
                "export profile '{}' is unavailable; supported export profiles: dls",
                arguments.profile
            ),
            EXIT_INVALID,
        ));
    }
    toolchain::validate_top(&arguments.top, "export")?;
    toolchain::new_destination(&arguments.out, "export")?;
    let project_name = arguments
        .out
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| is_safe_unit_name(n))
        .ok_or_else(|| {
            failure(
                "invalid_output",
                "--out must end in an ordinary UTF-8 project directory name.",
                EXIT_INVALID,
            )
        })?;
    let workspace = ScratchDirectory::temporary("export")?;
    let job = Job::new("export", &workspace.path, arguments.timeout_seconds);
    let sources = Sources::prepare(
        &arguments.sources,
        &workspace.path,
        arguments.system_verilog,
        "export",
    )?;
    let script = sources.yosys_script(&format!(
        "hierarchy -top {top}\nproc\nopt_clean\nwrite_json source.json\nsynth -top {top} -flatten -noabc\ndffunmap\nabc -g NAND\nclean\ncheck -assert\nwrite_json design.json\n", top=arguments.top
    ));
    toolchain::write(&workspace.path.join("script.ys"), script, "export")?;
    job.run(
        "Yosys",
        &arguments.yosys,
        &[
            "-Q".into(),
            "-T".into(),
            "-q".into(),
            "-s".into(),
            "script.ys".into(),
        ],
        &workspace.path,
        &sources.allowed_links,
    )?;
    let source = toolchain::read_regular(
        &workspace.path.join("source.json"),
        toolchain::MAX_FILE_BYTES,
        "export",
        true,
    )?;
    let source = std::str::from_utf8(&source).map_err(|_| {
        failure(
            "synthesis_output",
            "Yosys returned elaborated source that is not UTF-8 text.",
            EXIT_INVALID,
        )
    })?;
    jsonrtl_profiles::dls::export::validate_source_states(source, &arguments.top)
        .map_err(export_profile_failure)?;
    let netlist = toolchain::read_regular(
        &workspace.path.join("design.json"),
        toolchain::MAX_FILE_BYTES,
        "export",
        true,
    )?;
    let netlist = std::str::from_utf8(&netlist).map_err(|_| {
        failure(
            "synthesis_output",
            "Yosys returned a netlist that is not UTF-8 text.",
            EXIT_INVALID,
        )
    })?;
    let project = jsonrtl_profiles::dls::export::from_yosys_json(
        netlist,
        &arguments.top,
        project_name,
        arguments.clock.as_deref(),
    )
    .map_err(export_profile_failure)?;
    toolchain::publish(&arguments.out, "export", |directory| {
        fs::create_dir(directory.join("Chips")).map_err(|e| io_failure("export", e))?;
        toolchain::write(
            &directory.join("ProjectDescription.json"),
            &project.project_description,
            "export",
        )?;
        for (name, contents) in &project.chips {
            if !is_safe_unit_name(name) {
                return Err(failure(
                    "invalid_artifact",
                    "exporter returned an unsafe chip name",
                    EXIT_IO,
                ));
            }
            toolchain::write(
                &directory.join("Chips").join(format!("{name}.json")),
                contents,
                "export",
            )?;
        }
        Ok(())
    })?;
    Ok(project)
}

fn export_profile_failure(error: jsonrtl_profiles::ProfileError) -> CliFailure {
    let mut failure = profile_failure(error);
    failure.stage = "export";
    failure
}
