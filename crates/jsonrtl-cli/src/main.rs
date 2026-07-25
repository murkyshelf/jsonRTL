use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::atomic::{AtomicU64, Ordering},
};

use clap::{Args, CommandFactory, Parser, Subcommand, ValueEnum};
use jsonrtl::{
    CIRCUIT_V1_SCHEMA, CircuitDocument, CompileOptions, Diagnostic, DiagnosticCode,
    DiagnosticSeverity, Kernel, ParseError, SUPPORTED_SCHEMA_VERSION, ValidationReport,
};
use jsonrtl_profiles::{
    NamedCircuit, Profile, ProfileError, detect_profile, is_safe_unit_name, profile_by_id, registry,
};
use serde::Serialize;
use serde_json::{Value, json};

const EXIT_INVALID: u8 = 2;
const EXIT_IO: u8 = 3;
const EXIT_INTERNAL: u8 = 4;
const COMPILER_VERSION: &str = env!("CARGO_PKG_VERSION");
static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Debug, Parser)]
#[command(
    name = "jsonrtl",
    version,
    about = "Validate canonical digital circuits and compile deterministic Verilog-2001"
)]
struct Cli {
    /// Diagnostic rendering. Diagnostics are always written to stderr.
    #[arg(
        long,
        value_enum,
        default_value_t = DiagnosticFormat::Human,
        global = true
    )]
    diagnostics: DiagnosticFormat,

    /// List the import profiles available in this build and exit.
    #[arg(long, global = true)]
    list_profiles: bool,

    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum DiagnosticFormat {
    Human,
    Json,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Validate a canonical circuit document.
    Validate(InputArgs),
    /// Compile a canonical circuit document to deterministic Verilog-2001.
    Compile(CompileArgs),
    /// Import a foreign project (e.g. DLS) and compile each unit to Verilog.
    Import(ImportArgs),
    /// List the import profiles available in this build.
    Profiles,
    /// Print the canonical circuit JSON Schema.
    Schema,
}

#[derive(Debug, Args)]
struct InputArgs {
    /// Canonical circuit JSON file to read.
    circuit: PathBuf,
}

#[derive(Debug, Args)]
struct CompileArgs {
    /// Canonical circuit JSON file to read.
    circuit: PathBuf,

    /// Atomically write Verilog to this file.
    #[arg(long, value_name = "FILE", required_unless_present = "stdout")]
    output: Option<PathBuf>,

    /// Write only generated Verilog to stdout.
    #[arg(long, conflicts_with = "output")]
    stdout: bool,

    /// Permit an existing --output file to be atomically replaced.
    #[arg(long, requires = "output")]
    force: bool,
}

#[derive(Debug, Args)]
struct ImportArgs {
    /// Foreign project directory to import.
    #[arg(required_unless_present = "list_profiles")]
    project: Option<PathBuf>,

    /// Import profile id. Auto-detected when omitted.
    #[arg(long, value_name = "ID")]
    profile: Option<String>,

    /// Directory to write one `<ChipName>.v` per compiled unit.
    #[arg(
        long,
        value_name = "DIR",
        required_unless_present_any = ["stdout", "list_profiles"]
    )]
    out: Option<PathBuf>,

    /// Restrict to a single unit by name.
    #[arg(long, value_name = "NAME")]
    chip: Option<String>,

    /// Write a single unit's Verilog to stdout. Requires --chip.
    #[arg(long, requires = "chip", conflicts_with = "out")]
    stdout: bool,

    /// Also write the intermediate canonical JSON per unit to this directory.
    #[arg(long, value_name = "DIR")]
    emit_canonical: Option<PathBuf>,

    /// Permit existing output files to be atomically replaced.
    #[arg(long)]
    force: bool,

    /// Emit every unit that compiles instead of failing on the first that does
    /// not. Each skipped unit is reported with its reason and the run still
    /// exits non-zero, so nothing is ever skipped silently.
    #[arg(long, conflicts_with = "chip")]
    skip_unsupported: bool,
}

#[derive(Debug)]
struct CliFailure {
    stage: &'static str,
    category: &'static str,
    message: String,
    diagnostics: Value,
    exit_code: u8,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DiagnosticEnvelope<'a> {
    success: bool,
    command: &'a str,
    valid: bool,
    schema_version: &'static str,
    compiler_version: &'static str,
    diagnostics: &'a [Diagnostic],
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(code) => ExitCode::from(code),
    }
}

fn run(cli: Cli) -> Result<(), u8> {
    // `--list-profiles` is global, so it answers the question wherever it is
    // asked: bare, or alongside the `import` command it informs.
    if cli.list_profiles {
        print_profiles(cli.diagnostics);
        return Ok(());
    }

    let Some(command) = cli.command else {
        let mut help = Cli::command();
        let _ = help.print_help();
        println!();
        return Err(EXIT_INVALID);
    };

    match command {
        Command::Validate(arguments) => validate_command(arguments, cli.diagnostics),
        Command::Compile(arguments) => compile_command(arguments, cli.diagnostics),
        Command::Import(arguments) => import_command(arguments, cli.diagnostics),
        Command::Profiles => {
            print_profiles(cli.diagnostics);
            Ok(())
        }
        Command::Schema => print_schema().map_err(|error| {
            render_failure(cli.diagnostics, &io_failure("schema", error));
            EXIT_IO
        }),
    }
}

fn validate_command(arguments: InputArgs, format: DiagnosticFormat) -> Result<(), u8> {
    let document = load_document(&arguments.circuit).map_err(|failure| {
        let exit_code = failure.exit_code;
        render_failure(format, &failure);
        exit_code
    })?;
    let report = Kernel::default().validate(&document);
    let valid = !report.has_errors();
    render_report(format, "validate", valid, &report);
    if valid { Ok(()) } else { Err(EXIT_INVALID) }
}

fn compile_command(arguments: CompileArgs, format: DiagnosticFormat) -> Result<(), u8> {
    if let Some(output) = &arguments.output {
        if output.exists() && !arguments.force {
            let failure = CliFailure {
                stage: "output",
                category: "output_exists",
                message: format!(
                    "refusing to overwrite '{}'; pass --force to replace it",
                    output.display()
                ),
                diagnostics: Value::Array(Vec::new()),
                exit_code: EXIT_IO,
            };
            render_failure(format, &failure);
            return Err(failure.exit_code);
        }
    }

    let document = load_document(&arguments.circuit).map_err(|failure| {
        let exit_code = failure.exit_code;
        render_failure(format, &failure);
        exit_code
    })?;
    let result = Kernel::default().compile_verilog(&document, &CompileOptions::default());
    if !result.has_output() {
        let internal = result
            .diagnostics
            .diagnostics()
            .iter()
            .any(|diagnostic| diagnostic.code == DiagnosticCode::InternalInvariant);
        render_report(format, "compile", false, &result.diagnostics);
        return Err(if internal {
            EXIT_INTERNAL
        } else {
            EXIT_INVALID
        });
    }

    let verilog = result.verilog.as_deref().expect("has_output checked above");
    if arguments.stdout {
        let mut stdout = io::stdout().lock();
        stdout.write_all(verilog.as_bytes()).map_err(|error| {
            render_failure(format, &io_failure("stdout", error));
            EXIT_IO
        })?;
        stdout.flush().map_err(|error| {
            render_failure(format, &io_failure("stdout", error));
            EXIT_IO
        })?;
    } else if let Some(output) = arguments.output {
        write_atomic(&output, verilog.as_bytes(), arguments.force).map_err(|error| {
            render_failure(format, &io_failure("output", error));
            EXIT_IO
        })?;
    }
    render_report(format, "compile", true, &result.diagnostics);
    Ok(())
}

fn import_command(arguments: ImportArgs, format: DiagnosticFormat) -> Result<(), u8> {
    // `--list-profiles` short-circuits in `run`, so a project is present here.
    let project = arguments
        .project
        .clone()
        .expect("clap requires PROJECT unless --list-profiles");
    let profile = select_profile(&arguments, &project).map_err(|failure| {
        let code = failure.exit_code;
        render_failure(format, &failure);
        code
    })?;

    // In skip mode each unit is converted and compiled independently so one
    // failure cannot hide the units that do work. Every skip is reported.
    let mut skipped: Vec<SkippedUnit> = Vec::new();
    let (project_name, selected) = if arguments.skip_unsupported {
        let units = profile.units(&project).map_err(|error| {
            let failure = profile_failure(error);
            let code = failure.exit_code;
            render_failure(format, &failure);
            code
        })?;
        let mut kept = Vec::new();
        for unit in &units.unit_names {
            match profile.convert_unit(&project, unit) {
                Ok(conversion) => kept.extend(conversion.circuits),
                Err(error) => skipped.push(SkippedUnit {
                    unit: unit.clone(),
                    reason: error.to_string(),
                }),
            }
        }
        (units.project_name, kept)
    } else {
        // Select before converting. Converting the whole project first would
        // let an unsupported chip fail a `--chip` run for an unrelated unit.
        let conversion = match &arguments.chip {
            Some(name) => profile.convert_unit(&project, name),
            None => profile.convert(&project),
        }
        .map_err(|error| {
            let failure = profile_failure(error);
            let code = failure.exit_code;
            render_failure(format, &failure);
            code
        })?;
        (conversion.project_name, conversion.circuits)
    };

    // Compile each selected unit.
    let mut outputs: Vec<(&str, String)> = Vec::with_capacity(selected.len());
    for named in &selected {
        let result = Kernel::default().compile_verilog(&named.document, &CompileOptions::default());
        if !result.has_output() {
            if arguments.skip_unsupported {
                skipped.push(SkippedUnit {
                    unit: named.name.clone(),
                    reason: first_error_message(&result.diagnostics),
                });
                continue;
            }
            let internal = result
                .diagnostics
                .diagnostics()
                .iter()
                .any(|diagnostic| diagnostic.code == DiagnosticCode::InternalInvariant);
            eprintln!("import: unit '{}' failed to compile", named.name);
            render_report(format, "import", false, &result.diagnostics);
            return Err(if internal {
                EXIT_INTERNAL
            } else {
                EXIT_INVALID
            });
        }
        let verilog = result.verilog.expect("has_output checked above");
        outputs.push((named.name.as_str(), verilog));
    }
    let selected: Vec<&NamedCircuit> = selected
        .iter()
        .filter(|named| outputs.iter().any(|(name, _)| *name == named.name))
        .collect();

    if arguments.skip_unsupported && outputs.is_empty() {
        let failure = CliFailure {
            stage: "import",
            category: "no_supported_units",
            message: format!(
                "no unit in project '{project_name}' could be compiled; {} skipped",
                skipped.len()
            ),
            diagnostics: skipped_json(&skipped),
            exit_code: EXIT_INVALID,
        };
        render_failure(format, &failure);
        return Err(EXIT_INVALID);
    }

    // Unit names become file names. A profile is responsible for its own input,
    // but this boundary owns what gets written, so names are checked here too
    // before any path is built from them.
    for named in &selected {
        if !is_safe_unit_name(&named.name) {
            let failure = CliFailure {
                stage: "output",
                category: "unsafe_unit_name",
                message: format!(
                    "unit name '{}' is not a valid file name; refusing to write outside the target directory",
                    named.name
                ),
                diagnostics: Value::Array(Vec::new()),
                exit_code: EXIT_INVALID,
            };
            render_failure(format, &failure);
            return Err(EXIT_INVALID);
        }
    }

    // Refuse the whole run if any destination already exists, so a partial set
    // of files is never left behind by a mid-loop failure.
    if !arguments.force {
        let mut destinations: Vec<PathBuf> = Vec::new();
        if let Some(dir) = &arguments.emit_canonical {
            destinations.extend(
                selected
                    .iter()
                    .map(|named| dir.join(format!("{}.json", named.name))),
            );
        }
        if let Some(out) = &arguments.out {
            destinations.extend(
                selected
                    .iter()
                    .map(|named| out.join(format!("{}.v", named.name))),
            );
        }
        if let Some(existing) = destinations.iter().find(|path| path.exists()) {
            let failure = CliFailure {
                stage: "output",
                category: "output_exists",
                message: format!(
                    "refusing to overwrite '{}'; pass --force to replace it",
                    existing.display()
                ),
                diagnostics: Value::Array(Vec::new()),
                exit_code: EXIT_IO,
            };
            render_failure(format, &failure);
            return Err(EXIT_IO);
        }
    }

    // Optionally emit the intermediate canonical JSON.
    if let Some(dir) = &arguments.emit_canonical {
        fs::create_dir_all(dir).map_err(|error| {
            render_failure(format, &io_failure("emit-canonical", error));
            EXIT_IO
        })?;
        for named in &selected {
            let json = serde_json::to_string_pretty(&named.document).map_err(|_| {
                let failure = CliFailure {
                    stage: "emit-canonical",
                    category: "internal",
                    message: "canonical document could not be serialized".into(),
                    diagnostics: Value::Array(Vec::new()),
                    exit_code: EXIT_INTERNAL,
                };
                render_failure(format, &failure);
                EXIT_INTERNAL
            })?;
            let path = dir.join(format!("{}.json", named.name));
            write_atomic(&path, json.as_bytes(), arguments.force).map_err(|error| {
                render_failure(format, &io_failure("emit-canonical", error));
                EXIT_IO
            })?;
        }
    }

    // Emit Verilog to stdout or one file per unit.
    if arguments.stdout {
        let (_, verilog) = &outputs[0];
        let mut handle = io::stdout().lock();
        handle.write_all(verilog.as_bytes()).map_err(|error| {
            render_failure(format, &io_failure("stdout", error));
            EXIT_IO
        })?;
        handle.flush().map_err(|error| {
            render_failure(format, &io_failure("stdout", error));
            EXIT_IO
        })?;
    } else {
        let out = arguments
            .out
            .as_ref()
            .expect("clap requires --out unless --stdout");
        fs::create_dir_all(out).map_err(|error| {
            render_failure(format, &io_failure("output", error));
            EXIT_IO
        })?;
        for (name, verilog) in &outputs {
            let path = out.join(format!("{name}.v"));
            write_atomic(&path, verilog.as_bytes(), arguments.force).map_err(|error| {
                render_failure(format, &io_failure("output", error));
                EXIT_IO
            })?;
        }
    }

    render_import_success(format, &project_name, &outputs, &skipped);
    // Skipping is opt-in but never silent: the run reports every skipped unit
    // and still fails, so a script cannot mistake a partial import for a full
    // one.
    if skipped.is_empty() {
        Ok(())
    } else {
        Err(EXIT_INVALID)
    }
}

/// A unit left out of a `--skip-unsupported` run, and why.
#[derive(Debug)]
struct SkippedUnit {
    unit: String,
    reason: String,
}

fn skipped_json(skipped: &[SkippedUnit]) -> Value {
    Value::Array(
        skipped
            .iter()
            .map(|entry| json!({ "unit": entry.unit, "reason": entry.reason }))
            .collect(),
    )
}

/// The first error message in a report, for a one-line skip reason.
fn first_error_message(report: &jsonrtl::ValidationReport) -> String {
    report
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
        .map_or_else(
            || "unit failed to compile".to_string(),
            |diagnostic| format!("{}: {}", diagnostic.code.as_str(), diagnostic.message),
        )
}

/// The ids a caller may pass to `--profile`, for error messages.
fn available_profile_ids() -> String {
    registry()
        .iter()
        .map(|profile| profile.id())
        .collect::<Vec<_>>()
        .join(", ")
}

fn select_profile(arguments: &ImportArgs, project: &Path) -> Result<Box<dyn Profile>, CliFailure> {
    match &arguments.profile {
        Some(id) => profile_by_id(id).ok_or_else(|| CliFailure {
            stage: "import",
            category: "unknown_profile",
            message: format!(
                "no import profile with id '{id}'; available ids are {}. Run 'jsonrtl profiles' for details",
                available_profile_ids()
            ),
            diagnostics: Value::Array(Vec::new()),
            exit_code: EXIT_INVALID,
        }),
        None => detect_profile(project).ok_or_else(|| CliFailure {
            stage: "import",
            category: "profile_detection",
            message: format!(
                "could not detect an import profile for '{}'; pass --profile with one of {}. Run 'jsonrtl profiles' for details",
                project.display(),
                available_profile_ids()
            ),
            diagnostics: Value::Array(Vec::new()),
            exit_code: EXIT_INVALID,
        }),
    }
}

fn profile_failure(error: ProfileError) -> CliFailure {
    let (category, exit_code) = match &error {
        ProfileError::Io { .. } => ("io", EXIT_IO),
        ProfileError::Parse { .. } => ("malformed_input", EXIT_INVALID),
        ProfileError::Unsupported { .. } => ("unsupported", EXIT_INVALID),
        ProfileError::Structure { .. } => ("structure", EXIT_INVALID),
        ProfileError::Limit { .. } => ("resource_limit", EXIT_INVALID),
        ProfileError::UnknownUnit { .. } => ("unknown_unit", EXIT_INVALID),
    };
    CliFailure {
        stage: "import",
        category,
        message: error.to_string(),
        diagnostics: Value::Array(Vec::new()),
        exit_code,
    }
}

fn render_import_success(
    format: DiagnosticFormat,
    project: &str,
    outputs: &[(&str, String)],
    skipped: &[SkippedUnit],
) {
    match format {
        DiagnosticFormat::Human => {
            for (name, _) in outputs {
                eprintln!("compiled unit '{name}'");
            }
            for entry in skipped {
                eprintln!("skipped unit '{}': {}", entry.unit, entry.reason);
            }
            eprintln!("import: {} unit(s) from project '{project}'", outputs.len());
            if !skipped.is_empty() {
                eprintln!("import: {} unit(s) skipped", skipped.len());
            }
        }
        DiagnosticFormat::Json => {
            let units: Vec<&str> = outputs.iter().map(|(name, _)| *name).collect();
            let envelope = json!({
                "success": skipped.is_empty(),
                "command": "import",
                "project": project,
                "schemaVersion": SUPPORTED_SCHEMA_VERSION,
                "compilerVersion": COMPILER_VERSION,
                "units": units,
                "skipped": skipped_json(skipped),
            });
            match serde_json::to_string(&envelope) {
                Ok(encoded) => eprintln!("{encoded}"),
                Err(_) => eprintln!("{{\"success\":false}}"),
            }
        }
    }
}

/// Lists the import profiles compiled into this build.
fn print_profiles(format: DiagnosticFormat) {
    let profiles = registry();
    match format {
        DiagnosticFormat::Human => {
            println!("Import profiles available in this build:");
            println!();
            for profile in &profiles {
                println!("  {} ({})", profile.id(), profile.status().as_str());
                println!("      source:   {}", profile.source());
                println!("      input:    {}", profile.input_hint());
                println!("      supports: {}", profile.supports());
                println!();
            }
            println!("Use with: jsonrtl import <PROJECT> --profile <ID>");
            println!("The profile is auto-detected when --profile is omitted.");
        }
        DiagnosticFormat::Json => {
            let entries: Vec<Value> = profiles
                .iter()
                .map(|profile| {
                    json!({
                        "id": profile.id(),
                        "status": profile.status().as_str(),
                        "source": profile.source(),
                        "input": profile.input_hint(),
                        "supports": profile.supports(),
                    })
                })
                .collect();
            let envelope = json!({
                "success": true,
                "command": "profiles",
                "compilerVersion": COMPILER_VERSION,
                "profiles": entries,
            });
            match serde_json::to_string(&envelope) {
                Ok(encoded) => println!("{encoded}"),
                Err(_) => println!("{{\"success\":false}}"),
            }
        }
    }
}

fn print_schema() -> io::Result<()> {
    let mut stdout = io::stdout().lock();
    stdout.write_all(CIRCUIT_V1_SCHEMA.as_bytes())?;
    if !CIRCUIT_V1_SCHEMA.ends_with('\n') {
        stdout.write_all(b"\n")?;
    }
    stdout.flush()
}

fn load_document(path: &Path) -> Result<CircuitDocument, CliFailure> {
    let input = fs::read_to_string(path).map_err(|error| CliFailure {
        stage: "input",
        category: "io",
        message: format!("could not read '{}': {error}", path.display()),
        diagnostics: Value::Array(Vec::new()),
        exit_code: EXIT_IO,
    })?;
    CircuitDocument::from_json(&input).map_err(parse_failure)
}

fn parse_failure(error: ParseError) -> CliFailure {
    match error {
        ParseError::DocumentTooLarge { actual, maximum } => CliFailure {
            stage: "parse",
            category: "resource_limit",
            message: "document exceeds the configured byte limit".into(),
            diagnostics: json!([{ "code": "LIMIT_DOCUMENT_BYTES", "actual": actual, "maximum": maximum }]),
            exit_code: EXIT_INVALID,
        },
        ParseError::MalformedJson {
            message,
            line,
            column,
        } => CliFailure {
            stage: "parse",
            category: "malformed_json",
            message: "input is not valid JSON".into(),
            diagnostics: json!([{ "code": "MALFORMED_JSON", "message": message, "line": line, "column": column }]),
            exit_code: EXIT_INVALID,
        },
        ParseError::UnsupportedSchemaVersion { found, supported } => CliFailure {
            stage: "schema",
            category: "unsupported_schema_version",
            message: format!("schema version '{found}' is unsupported"),
            diagnostics: json!([{ "code": "UNSUPPORTED_SCHEMA_VERSION", "found": found, "supported": supported }]),
            exit_code: EXIT_INVALID,
        },
        ParseError::Schema { diagnostics } => CliFailure {
            stage: "schema",
            category: "schema",
            message: "document does not satisfy canonical schema v1.0".into(),
            diagnostics: serde_json::to_value(diagnostics).unwrap_or_else(|_| Value::Array(vec![])),
            exit_code: EXIT_INVALID,
        },
        ParseError::ResourceLimits { diagnostics } => CliFailure {
            stage: "schema",
            category: "resource_limit",
            message: "document exceeds configured kernel limits".into(),
            diagnostics: serde_json::to_value(diagnostics).unwrap_or_else(|_| Value::Array(vec![])),
            exit_code: EXIT_INVALID,
        },
        ParseError::InvalidEmbeddedSchema { .. } | ParseError::Deserialization { .. } => {
            CliFailure {
                stage: "internal",
                category: "internal",
                message: "the kernel could not process the canonical schema".into(),
                diagnostics: Value::Array(Vec::new()),
                exit_code: EXIT_INTERNAL,
            }
        }
    }
}

fn io_failure(stage: &'static str, error: io::Error) -> CliFailure {
    CliFailure {
        stage,
        category: "io",
        message: error.to_string(),
        diagnostics: Value::Array(Vec::new()),
        exit_code: EXIT_IO,
    }
}

fn render_report(
    format: DiagnosticFormat,
    command: &'static str,
    valid: bool,
    report: &ValidationReport,
) {
    match format {
        DiagnosticFormat::Human => {
            for diagnostic in report.diagnostics() {
                eprintln!(
                    "{:?}[{}] {} ({})",
                    diagnostic.severity,
                    diagnostic.code,
                    diagnostic.message,
                    diagnostic.ordering_key
                );
            }
            eprintln!("{command}: {}", if valid { "valid" } else { "invalid" });
        }
        DiagnosticFormat::Json => {
            let envelope = DiagnosticEnvelope {
                success: valid,
                command,
                valid,
                schema_version: SUPPORTED_SCHEMA_VERSION,
                compiler_version: COMPILER_VERSION,
                diagnostics: report.diagnostics(),
            };
            match serde_json::to_string(&envelope) {
                Ok(encoded) => eprintln!("{encoded}"),
                Err(_) => eprintln!(
                    "{{\"success\":false,\"error\":{{\"category\":\"internal\",\"message\":\"diagnostics could not be serialized\"}}}}"
                ),
            }
        }
    }
}

fn render_failure(format: DiagnosticFormat, failure: &CliFailure) {
    match format {
        DiagnosticFormat::Human => {
            eprintln!(
                "error[{}] during {}: {}",
                failure.category, failure.stage, failure.message
            );
            if failure.diagnostics != Value::Array(Vec::new()) {
                eprintln!(
                    "{}",
                    serde_json::to_string_pretty(&failure.diagnostics)
                        .unwrap_or_else(|_| "diagnostics unavailable".into())
                );
            }
        }
        DiagnosticFormat::Json => {
            let envelope = json!({
                "success": false,
                "stage": failure.stage,
                "error": {
                    "category": failure.category,
                    "message": failure.message,
                },
                "schemaVersion": SUPPORTED_SCHEMA_VERSION,
                "compilerVersion": COMPILER_VERSION,
                "diagnostics": failure.diagnostics,
            });
            eprintln!(
                "{}",
                serde_json::to_string(&envelope).unwrap_or_else(|_| {
                    "{\"success\":false,\"error\":{\"category\":\"internal\",\"message\":\"failure could not be serialized\"}}".into()
                })
            );
        }
    }
}

fn write_atomic(path: &Path, contents: &[u8], force: bool) -> io::Result<()> {
    if path.exists() && !force {
        return Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!(
                "'{}' already exists; pass --force to replace it",
                path.display()
            ),
        ));
    }

    let parent = path
        .parent()
        .filter(|item| !item.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidInput, "output path has no file name")
    })?;
    let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary = parent.join(format!(
        ".{}.{}.{}.tmp",
        file_name.to_string_lossy(),
        std::process::id(),
        counter
    ));

    let result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)?;
        file.write_all(contents)?;
        file.sync_all()?;
        drop(file);

        if force {
            fs::rename(&temporary, path)
        } else {
            fs::hard_link(&temporary, path)?;
            fs::remove_file(&temporary)
        }
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}
