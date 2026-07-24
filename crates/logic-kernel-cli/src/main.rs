use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
    process::ExitCode,
    sync::atomic::{AtomicU64, Ordering},
};

use clap::{Args, Parser, Subcommand, ValueEnum};
use logic_kernel::{
    CIRCUIT_V1_SCHEMA, CircuitDocument, CompileOptions, Diagnostic, DiagnosticCode, Kernel,
    ParseError, SUPPORTED_SCHEMA_VERSION, ValidationReport,
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
    name = "logic-kernel",
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

    #[command(subcommand)]
    command: Command,
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
    /// Print the canonical circuit JSON Schema v1.0.
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
    match cli.command {
        Command::Validate(arguments) => validate_command(arguments, cli.diagnostics),
        Command::Compile(arguments) => compile_command(arguments, cli.diagnostics),
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
