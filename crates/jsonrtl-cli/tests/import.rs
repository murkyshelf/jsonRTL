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
    assert!(canonical.contains(&format!(
        "\"schemaVersion\": \"{}\"",
        jsonrtl::SUPPORTED_SCHEMA_VERSION
    )));
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

#[test]
fn import_refuses_a_project_whose_chip_name_escapes_the_output_directory() {
    // Regression: a chip named "../escaped" wrote its Verilog one level above
    // --out and still exited 0 reporting success.
    let directory = TempDirectory::new();
    let project = directory.path().join("proj");
    let out = directory.path().join("out");
    fs::create_dir_all(project.join("Chips")).unwrap();
    fs::create_dir_all(&out).unwrap();
    fs::write(
        project.join("ProjectDescription.json"),
        r#"{"ProjectName":"trav","AllCustomChipNames":["../escaped"]}"#,
    )
    .unwrap();

    let output = run([
        "import",
        project.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]);

    assert_ne!(output.status.code(), Some(0), "traversal must not succeed");
    assert!(
        !directory.path().join("escaped.v").exists(),
        "wrote outside the output directory"
    );
    assert_eq!(
        fs::read_dir(&out).unwrap().count(),
        0,
        "out must stay empty"
    );
}

#[test]
fn import_writes_nothing_when_any_destination_already_exists() {
    // Regression: files were written one at a time, so a mid-loop failure left
    // the units compiled before it on disk.
    let directory = TempDirectory::new();
    let blocker = directory.path().join("AND.v");
    fs::write(&blocker, "sentinel\n").unwrap();

    let output = run([
        "import",
        dls_project("test").to_str().unwrap(),
        "--out",
        directory.path().to_str().unwrap(),
    ]);

    assert_eq!(output.status.code(), Some(3));
    assert!(stderr(&output).contains("--force"));
    assert_eq!(fs::read_to_string(&blocker).unwrap(), "sentinel\n");
    // No other unit may have been written before the failure.
    let written: Vec<String> = fs::read_dir(directory.path())
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    assert_eq!(
        written,
        vec!["AND.v".to_string()],
        "partial output: {written:?}"
    );
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

#[test]
fn skip_unsupported_emits_what_compiles_and_names_the_rest() {
    // The `unsupported` fixture mixes convertible chips (inverter, wide) with
    // ones outside the subset (clocky).
    let directory = TempDirectory::new();
    let output = run([
        "import",
        dls_project("unsupported").to_str().unwrap(),
        "--out",
        directory.path().to_str().unwrap(),
        "--skip-unsupported",
    ]);

    // Still fails, so a script cannot mistake a partial import for a full one.
    assert_eq!(output.status.code(), Some(2), "stderr: {}", stderr(&output));

    assert!(directory.path().join("inverter.v").is_file());
    assert!(directory.path().join("wide.v").is_file());
    assert!(!directory.path().join("clocky.v").is_file());

    let text = stderr(&output);
    assert!(text.contains("compiled unit 'inverter'"), "{text}");
    assert!(text.contains("skipped unit 'clocky'"), "{text}");
    assert!(text.contains("CLOCK"), "reason must be given: {text}");
    assert!(text.contains("unit(s) skipped"), "{text}");
}

#[test]
fn without_skip_unsupported_a_bad_chip_still_fails_the_whole_run() {
    let directory = TempDirectory::new();
    let output = run([
        "import",
        dls_project("unsupported").to_str().unwrap(),
        "--out",
        directory.path().to_str().unwrap(),
    ]);
    assert_eq!(output.status.code(), Some(2));
    assert!(
        fs::read_dir(directory.path()).unwrap().next().is_none(),
        "a failed run must leave no files behind"
    );
}

#[test]
fn skip_unsupported_json_lists_every_skipped_unit_with_a_reason() {
    let directory = TempDirectory::new();
    let output = run([
        "--diagnostics",
        "json",
        "import",
        dls_project("unsupported").to_str().unwrap(),
        "--out",
        directory.path().to_str().unwrap(),
        "--skip-unsupported",
    ]);
    let envelope: serde_json::Value = serde_json::from_str(stderr(&output).trim()).unwrap();
    assert_eq!(envelope["success"], false);
    assert_eq!(envelope["command"], "import");
    let skipped = envelope["skipped"].as_array().unwrap();
    assert!(!skipped.is_empty());
    assert!(skipped.iter().all(|entry| {
        !entry["unit"].as_str().unwrap().is_empty() && !entry["reason"].as_str().unwrap().is_empty()
    }));
}

#[test]
fn profiles_lists_every_registered_profile() {
    let output = run(["profiles"]);
    assert_eq!(output.status.code(), Some(0));
    let text = String::from_utf8_lossy(&output.stdout).into_owned();
    assert!(text.contains("dls"), "{text}");
    assert!(text.contains("logisim"), "{text}");
    assert!(text.contains("experimental"), "{text}");
    assert!(text.contains("Digital-Logic-Sim"), "{text}");
}

#[test]
fn profiles_json_is_machine_readable() {
    let output = run(["--diagnostics", "json", "profiles"]);
    assert_eq!(output.status.code(), Some(0));
    let envelope: serde_json::Value =
        serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).unwrap();
    assert_eq!(envelope["command"], "profiles");
    let profiles = envelope["profiles"].as_array().unwrap();
    let ids: Vec<&str> = profiles
        .iter()
        .map(|profile| profile["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["dls", "logisim"]);
    for profile in profiles {
        for field in ["status", "source", "input", "supports"] {
            assert!(
                !profile[field].as_str().unwrap().is_empty(),
                "empty {field}"
            );
        }
    }
}
