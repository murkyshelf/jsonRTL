use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
fn valid_validate_uses_the_shared_kernel() {
    let output = run([
        "validate",
        fixture("valid/minimal-and.json").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("validate: valid"));
}

#[test]
fn invalid_validate_returns_the_document_failure_exit_code() {
    let output = run([
        "validate",
        fixture("semantic/combined-invalid.json").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(output.stdout.is_empty());
    assert!(stderr(&output).contains("validate: invalid"));
}

#[test]
fn compile_stdout_contains_only_byte_exact_verilog() {
    let output = run([
        "compile",
        fixture("valid/minimal-and.json").to_str().unwrap(),
        "--stdout",
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, fs::read(golden("minimal-and.v")).unwrap());
    assert!(stderr(&output).contains("compile: valid"));
    assert!(!String::from_utf8_lossy(&output.stdout).contains("Warning"));
}

#[test]
fn compile_file_is_atomic_and_requires_force_to_replace() {
    let directory = TempDirectory::new();
    let target = directory.path().join("circuit.v");
    fs::write(&target, "sentinel\n").unwrap();

    let refused = run([
        "compile",
        fixture("valid/minimal-and.json").to_str().unwrap(),
        "--output",
        target.to_str().unwrap(),
    ]);
    assert_eq!(refused.status.code(), Some(3));
    assert_eq!(fs::read_to_string(&target).unwrap(), "sentinel\n");
    assert!(stderr(&refused).contains("--force"));

    let replaced = run([
        "compile",
        fixture("valid/minimal-and.json").to_str().unwrap(),
        "--output",
        target.to_str().unwrap(),
        "--force",
    ]);
    assert_eq!(replaced.status.code(), Some(0));
    assert!(replaced.stdout.is_empty());
    assert_eq!(
        fs::read(&target).unwrap(),
        fs::read(golden("minimal-and.v")).unwrap()
    );
    assert!(fs::read_dir(directory.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
}

#[test]
fn json_diagnostics_have_stable_machine_readable_fields() {
    let output = run([
        "--diagnostics",
        "json",
        "validate",
        fixture("semantic/combined-invalid.json").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(envelope["success"], false);
    assert_eq!(envelope["command"], "validate");
    assert_eq!(envelope["valid"], false);
    assert_eq!(envelope["schemaVersion"], "1.0");
    assert_eq!(envelope["compilerVersion"], env!("CARGO_PKG_VERSION"));
    assert!(envelope["diagnostics"].as_array().unwrap().len() > 1);
}

#[test]
fn malformed_and_missing_files_have_distinct_exit_categories() {
    let malformed = run([
        "--diagnostics",
        "json",
        "validate",
        fixture("invalid/malformed.json").to_str().unwrap(),
    ]);
    assert_eq!(malformed.status.code(), Some(2));
    let malformed_json: serde_json::Value = serde_json::from_slice(&malformed.stderr).unwrap();
    assert_eq!(malformed_json["error"]["category"], "malformed_json");

    let missing_path = fixture("does-not-exist.json");
    let missing = run([
        "--diagnostics",
        "json",
        "validate",
        missing_path.to_str().unwrap(),
    ]);
    assert_eq!(missing.status.code(), Some(3));
    let missing_json: serde_json::Value = serde_json::from_slice(&missing.stderr).unwrap();
    assert_eq!(missing_json["error"]["category"], "io");
}

#[test]
fn failed_compilation_never_creates_an_output_file() {
    let directory = TempDirectory::new();
    let target = directory.path().join("invalid.v");
    let output = run([
        "compile",
        fixture("semantic/combined-invalid.json").to_str().unwrap(),
        "--output",
        target.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(!target.exists());
}

#[test]
fn schema_command_prints_the_canonical_contract() {
    let output = run(["schema"]);
    assert_eq!(output.status.code(), Some(0));
    let schema: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        schema["$schema"],
        "https://json-schema.org/draft/2020-12/schema"
    );
    assert!(output.stderr.is_empty());
}

#[test]
fn json_diagnostics_for_valid_input() {
    let output = run([
        "--diagnostics",
        "json",
        "validate",
        fixture("valid/minimal-and.json").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(envelope["success"], true);
    assert_eq!(envelope["command"], "validate");
    assert_eq!(envelope["valid"], true);
    assert_eq!(envelope["schemaVersion"], "1.0");
    assert_eq!(envelope["compilerVersion"], env!("CARGO_PKG_VERSION"));
    let diags = envelope["diagnostics"].as_array().unwrap();
    for diagnostic in diags {
        assert_ne!(diagnostic["severity"], "error");
    }
}

#[test]
fn json_diagnostics_for_schema_invalid_input() {
    let output = run([
        "--diagnostics",
        "json",
        "validate",
        fixture("invalid/missing-required-field.json")
            .to_str()
            .unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(envelope["success"], false);
    assert_eq!(envelope["stage"], "schema");
    assert_eq!(envelope["error"]["category"], "schema");
    assert!(!envelope["diagnostics"].as_array().unwrap().is_empty());
    assert_eq!(envelope["schemaVersion"], "1.0");
}

#[test]
fn json_diagnostics_for_unsupported_version() {
    let output = run([
        "--diagnostics",
        "json",
        "validate",
        fixture("invalid/unsupported-version.json")
            .to_str()
            .unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(envelope["error"]["category"], "unsupported_schema_version");
    assert_eq!(envelope["stage"], "schema");
}

#[test]
fn compile_requires_output_or_stdout() {
    let output = run([
        "compile",
        fixture("valid/minimal-and.json").to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--output <FILE>"));
}

#[test]
fn compile_stdout_conflicts_with_output() {
    let output = run([
        "compile",
        fixture("valid/minimal-and.json").to_str().unwrap(),
        "--stdout",
        "--output",
        "out.v",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--stdout") && stderr(&output).contains("--output"));
}

#[test]
fn compile_force_requires_output() {
    let output = run([
        "compile",
        fixture("valid/minimal-and.json").to_str().unwrap(),
        "--force",
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("--force") && stderr(&output).contains("--output"));
}

#[test]
fn compile_new_output_without_force_succeeds() {
    let directory = TempDirectory::new();
    let target = directory.path().join("minimal-and.v");
    assert!(!target.exists());

    let output = run([
        "compile",
        fixture("valid/minimal-and.json").to_str().unwrap(),
        "--output",
        target.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(0));
    assert!(output.stdout.is_empty());
    assert_eq!(
        fs::read(&target).unwrap(),
        fs::read(golden("minimal-and.v")).unwrap()
    );
    assert!(stderr(&output).contains("compile: valid"));
    assert!(fs::read_dir(directory.path()).unwrap().all(|entry| {
        !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .ends_with(".tmp")
    }));
}

#[test]
fn failed_compile_with_force_preserves_existing_output() {
    let directory = TempDirectory::new();
    let target = directory.path().join("output.v");
    let sentinel = b"sentinel content\n";
    fs::write(&target, sentinel).unwrap();

    // Schema-invalid input — parse fails, exit 2, file unchanged.
    let schema_fail = run([
        "compile",
        fixture("invalid/missing-required-field.json")
            .to_str()
            .unwrap(),
        "--output",
        target.to_str().unwrap(),
        "--force",
    ]);
    assert_eq!(schema_fail.status.code(), Some(2));
    assert_eq!(fs::read(&target).unwrap(), sentinel);
    assert!(stderr(&schema_fail).contains("schema:"));

    // Malformed JSON input — parse fails, exit 2, file unchanged.
    let malformed_fail = run([
        "compile",
        fixture("invalid/malformed.json").to_str().unwrap(),
        "--output",
        target.to_str().unwrap(),
        "--force",
    ]);
    assert_eq!(malformed_fail.status.code(), Some(2));
    assert_eq!(fs::read(&target).unwrap(), sentinel);
    assert!(stderr(&malformed_fail).contains("input is not valid JSON"));
}

#[test]
fn json_diagnostics_for_output_io_failure() {
    use serde::Deserialize;

    let directory = TempDirectory::new();
    let absent_child = directory.path().join("nonexistent");
    let target = absent_child.join("output.v");
    assert!(!absent_child.exists());
    assert!(!target.exists());

    let output = run([
        "--diagnostics",
        "json",
        "compile",
        fixture("valid/minimal-and.json").to_str().unwrap(),
        "--output",
        target.to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(3));
    // Parse every JSON value on stderr — exactly one must be present.
    let mut de = serde_json::Deserializer::from_slice(&output.stderr);
    let envelope: serde_json::Value = serde_json::Value::deserialize(&mut de).unwrap();
    let second: Result<serde_json::Value, _> = serde_json::Value::deserialize(&mut de);
    assert!(
        second.is_err(),
        "stderr must contain exactly one JSON object"
    );
    assert_eq!(envelope["success"], false);
    assert_eq!(envelope["error"]["category"], "io");
    assert!(!target.exists());
}

#[test]
fn json_diagnostics_for_compile_stdout() {
    let output = run([
        "--diagnostics",
        "json",
        "compile",
        fixture("valid/minimal-and.json").to_str().unwrap(),
        "--stdout",
    ]);
    assert_eq!(output.status.code(), Some(0));
    // stdout is clean Verilog matching the golden
    assert_eq!(output.stdout, fs::read(golden("minimal-and.v")).unwrap());
    // stderr is exactly one parseable JSON diagnostic object
    let envelope: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(envelope["success"], true);
    assert_eq!(envelope["command"], "compile");
    assert_eq!(envelope["valid"], true);
    assert_eq!(envelope["schemaVersion"], "1.0");
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_logic-kernel")
}

fn run<const N: usize>(arguments: [&str; N]) -> Output {
    Command::new(binary()).args(arguments).output().unwrap()
}

fn fixture(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/fixtures")
        .join(relative)
}

fn golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../tests/golden")
        .join(name)
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

struct TempDirectory(PathBuf);

impl TempDirectory {
    fn new() -> Self {
        let counter = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "logic-kernel-phase4-cli-{}-{counter}",
            std::process::id()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
