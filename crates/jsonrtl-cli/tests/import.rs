use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

#[test]
fn import_writes_one_verilog_file_per_chip() {
    let directory = TempDirectory::new();
    let output = run([
        "import",
        dls_project("test").to_str().unwrap(),
        "--out",
        directory.path().to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));

    for chip in ["AND", "OR", "NOT", "XOR", "1-bit adder"] {
        let path = directory.path().join(format!("{chip}.v"));
        assert!(path.is_file(), "missing {}", path.display());
    }
    // AND is byte-exact against the committed golden.
    assert_eq!(
        fs::read(directory.path().join("AND.v")).unwrap(),
        fs::read(dls_golden("AND.v")).unwrap()
    );
    assert!(stderr(&output).contains("import: 5 unit(s)"));
}

#[test]
fn import_single_chip_to_stdout() {
    let output = run([
        "import",
        dls_project("test").to_str().unwrap(),
        "--chip",
        "AND",
        "--stdout",
    ]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("module AND"), "stdout:\n{text}");
}

#[test]
fn import_auto_detects_profile_and_emits_canonical() {
    let directory = TempDirectory::new();
    let output = run([
        "import",
        dls_project("test").to_str().unwrap(),
        "--chip",
        "AND",
        "--out",
        directory.path().to_str().unwrap(),
        "--emit-canonical",
        directory.path().to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let canonical = fs::read_to_string(directory.path().join("AND.json")).unwrap();
    assert!(canonical.contains("\"schemaVersion\": \"1.0\""));
    assert!(canonical.contains("\"NAND\""));
}

#[test]
fn import_unsupported_project_fails_with_clear_diagnostic() {
    let directory = TempDirectory::new();
    let output = run([
        "import",
        dls_project("unsupported").to_str().unwrap(),
        "--out",
        directory.path().to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        stderr(&output).contains("CLOCK"),
        "stderr: {}",
        stderr(&output)
    );
}

#[test]
fn import_unknown_profile_is_a_usage_error() {
    let directory = TempDirectory::new();
    let output = run([
        "import",
        dls_project("test").to_str().unwrap(),
        "--profile",
        "nope",
        "--out",
        directory.path().to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(stderr(&output).contains("nope"));
}

#[test]
fn import_json_diagnostics_list_units() {
    let directory = TempDirectory::new();
    let output = run([
        "--diagnostics",
        "json",
        "import",
        dls_project("test").to_str().unwrap(),
        "--out",
        directory.path().to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(0), "stderr: {}", stderr(&output));
    let envelope: serde_json::Value = serde_json::from_slice(&output.stderr).unwrap();
    assert_eq!(envelope["success"], true);
    assert_eq!(envelope["command"], "import");
    assert_eq!(envelope["project"], "test");
    let units = envelope["units"].as_array().unwrap();
    assert!(units.iter().any(|unit| unit == "AND"));
}

fn binary() -> &'static str {
    env!("CARGO_BIN_EXE_jsonrtl")
}

fn run<const N: usize>(arguments: [&str; N]) -> Output {
    Command::new(binary()).args(arguments).output().unwrap()
}

fn dls_project(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../jsonrtl-profiles/tests/fixtures/dls")
        .join(name)
}

fn dls_golden(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../jsonrtl-profiles/tests/golden")
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
            "jsonrtl-import-cli-{}-{counter}",
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
        let _ = fs::remove_dir_all(&self.0);
    }
}
