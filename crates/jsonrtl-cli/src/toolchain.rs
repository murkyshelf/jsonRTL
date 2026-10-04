//! Local HDL tool boundary. The reusable kernel never executes processes.
use crate::{CliFailure, EXIT_INVALID, EXIT_IO, io_failure};
use serde_json::json;
use std::{
    collections::BTreeSet,
    ffi::OsString,
    fs::{self, OpenOptions},
    io::{self, Read},
    path::{Component, Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

pub const MAX_FILE_BYTES: u64 = 32 * 1024 * 1024;
pub const MAX_CAPTURE_BYTES: u64 = 1024 * 1024;
const MAX_SOURCE_BYTES: u64 = 8 * 1024 * 1024;
const MAX_SOURCE_TOTAL: u64 = 32 * 1024 * 1024;
const MAX_SOURCES: usize = 256;
const MAX_WORKSPACE_BYTES: u64 = 128 * 1024 * 1024;
const MAX_ENTRIES: usize = 4096;
const MAX_DEPTH: usize = 32;
const MAX_TAIL: usize = 64 * 1024;
static NEXT: AtomicU64 = AtomicU64::new(0);

pub fn failure(
    stage: &'static str,
    category: &'static str,
    message: impl Into<String>,
    exit_code: u8,
) -> CliFailure {
    CliFailure {
        stage,
        category,
        message: message.into(),
        diagnostics: json!([]),
        exit_code,
    }
}
pub fn is_identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    matches!(bytes.next(), Some(b'a'..=b'z' | b'A'..=b'Z' | b'_'))
        && bytes.all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'$'))
}
pub fn validate_top(top: &str, stage: &'static str) -> Result<(), CliFailure> {
    if is_identifier(top) {
        Ok(())
    } else {
        Err(failure(
            stage,
            "invalid_top",
            "--top must be an ordinary Verilog identifier (letters, digits, '_' and '$').",
            EXIT_INVALID,
        ))
    }
}
pub fn exists(path: &Path) -> io::Result<bool> {
    match fs::symlink_metadata(path) {
        Ok(_) => Ok(true),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}
pub fn new_destination(path: &Path, stage: &'static str) -> Result<(), CliFailure> {
    if path.file_name().is_none()
        || path
            .to_str()
            .is_none_or(|n| n.chars().any(char::is_control))
    {
        return Err(failure(
            stage,
            "invalid_output",
            "--out must be a UTF-8 directory path without control characters.",
            EXIT_INVALID,
        ));
    }
    if exists(path).map_err(|e| io_failure(stage, e))? {
        return Err(failure(
            stage,
            "output_exists",
            format!(
                "'{}' already exists; choose a new output directory",
                path.display()
            ),
            EXIT_IO,
        ));
    }
    Ok(())
}

pub struct ScratchDirectory {
    pub path: PathBuf,
}
impl ScratchDirectory {
    pub fn new_in(parent: &Path, purpose: &str) -> io::Result<Self> {
        for _ in 0..100 {
            let path = parent.join(format!(
                ".jsonrtl-{purpose}-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            match builder.create(&path) {
                Ok(()) => {
                    return Ok(Self {
                        path: path.canonicalize()?,
                    });
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "could not allocate a private tool workspace",
        ))
    }
    pub fn temporary(stage: &'static str) -> Result<Self, CliFailure> {
        Self::new_in(&std::env::temp_dir(), "tools").map_err(|e| io_failure(stage, e))
    }
}
impl Drop for ScratchDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// O_NONBLOCK keeps a raced-in FIFO from blocking before its type is checked.
pub fn read_regular(
    path: &Path,
    maximum: u64,
    stage: &'static str,
    generated: bool,
) -> Result<Vec<u8>, CliFailure> {
    let info = fs::symlink_metadata(path).map_err(|e| io_failure(stage, e))?;
    if (generated && info.file_type().is_symlink())
        || (!info.is_file() && !info.file_type().is_symlink())
    {
        return Err(failure(
            stage,
            "invalid_file",
            format!("'{}' must be a regular file", path.display()),
            EXIT_INVALID,
        ));
    }
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let flags = libc::O_NONBLOCK | if generated { libc::O_NOFOLLOW } else { 0 };
        options.custom_flags(flags);
    }
    let file = options.open(path).map_err(|e| io_failure(stage, e))?;
    let info = file.metadata().map_err(|e| io_failure(stage, e))?;
    if !info.is_file() {
        return Err(failure(
            stage,
            "invalid_file",
            format!("'{}' must be a regular file", path.display()),
            EXIT_INVALID,
        ));
    }
    let over = || {
        failure(
            stage,
            "resource_limit",
            format!("'{}' exceeds the {maximum} byte file limit", path.display()),
            EXIT_INVALID,
        )
    };
    if info.len() > maximum {
        return Err(over());
    }
    let mut bytes = Vec::new();
    file.take(maximum + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_failure(stage, e))?;
    if bytes.len() as u64 > maximum {
        return Err(over());
    }
    Ok(bytes)
}
pub fn write(path: &Path, bytes: impl AsRef<[u8]>, stage: &'static str) -> Result<(), CliFailure> {
    fs::write(path, bytes).map_err(|e| io_failure(stage, e))
}

pub struct Sources {
    pub paths: Vec<OsString>,
    pub includes: Vec<OsString>,
    pub allowed_links: BTreeSet<PathBuf>,
    reads: String,
}
impl Sources {
    pub fn prepare(
        files: &[PathBuf],
        root: &Path,
        sv: bool,
        stage: &'static str,
    ) -> Result<Self, CliFailure> {
        if files.is_empty() || files.len() > MAX_SOURCES {
            return Err(failure(
                stage,
                "resource_limit",
                format!("provide 1–{MAX_SOURCES} source files"),
                EXIT_INVALID,
            ));
        }
        let mut result = Self {
            paths: Vec::new(),
            includes: Vec::new(),
            allowed_links: BTreeSet::new(),
            reads: String::new(),
        };
        let mut total = 0;
        for (index, source) in files.iter().enumerate() {
            let bytes = read_regular(
                source,
                MAX_SOURCE_BYTES.min(MAX_SOURCE_TOTAL - total),
                stage,
                false,
            )?;
            total += bytes.len() as u64;
            if std::str::from_utf8(&bytes).is_err() || bytes.contains(&0) {
                return Err(failure(
                    stage,
                    "malformed_input",
                    format!(
                        "source '{}' must be UTF-8 text without NUL bytes",
                        source.display()
                    ),
                    EXIT_INVALID,
                ));
            }
            let name = format!("source{index}.v");
            let canonical = source.canonicalize().map_err(|e| io_failure(stage, e))?;
            let include = format!("include{index}");
            let alias = root.join(&include);
            #[cfg(unix)]
            std::os::unix::fs::symlink(canonical.parent().expect("file has parent"), &alias)
                .map_err(|e| io_failure(stage, e))?;
            #[cfg(windows)]
            std::os::windows::fs::symlink_dir(canonical.parent().expect("file has parent"), &alias)
                .map_err(|e| io_failure(stage, e))?;
            result.allowed_links.insert(alias);
            result.includes.push(format!("-I{include}").into());
            // `line supplies the original directory through a safe alias.
            // With -grelative-include, Icarus selects each file's own headers,
            // including nested and ../ paths, while compiling bounded snapshots.
            // No sibling enumeration or copies of included files are needed.
            let mut snapshot = format!("`line 1 \"{include}/{name}\" 0\n").into_bytes();
            snapshot.extend_from_slice(&bytes);
            write(&root.join(&name), snapshot, stage)?;
            result.paths.push(name.clone().into());
            result.reads.push_str(if sv {
                "read_verilog -defer -sv "
            } else {
                "read_verilog -defer "
            });
            result.reads.push_str(&format!("-I{include} "));
            result.reads.push_str(&name);
            result.reads.push('\n');
        }
        Ok(result)
    }
    pub fn yosys_script(&self, body: &str) -> String {
        format!("{}{body}", self.reads)
    }
    pub fn icarus_args(&self, top: &str, sv: bool, output: Option<&str>) -> Vec<OsString> {
        let mut args: Vec<OsString> = vec![
            if sv { "-g2012" } else { "-g2005" }.into(),
            "-grelative-include".into(),
            "-s".into(),
            top.into(),
        ];
        if let Some(path) = output {
            args.extend([OsString::from("-o"), path.into()]);
        } else {
            args.push("-tnull".into());
        }
        args.extend(self.includes.iter().cloned());
        args.extend(self.paths.iter().cloned());
        args
    }
}

pub struct Job<'a> {
    stage: &'static str,
    root: &'a Path,
    deadline: Instant,
    seconds: u64,
    captured: Arc<AtomicU64>,
}
pub struct ToolOutput {
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
}
impl ToolOutput {
    pub fn text(&self) -> String {
        format!("{}{}", clean_text(&self.stdout), clean_text(&self.stderr))
    }
}
struct Capture {
    bytes: Mutex<Vec<u8>>,
    done: AtomicBool,
    error: Mutex<Option<String>>,
}
impl Capture {
    fn start(mut pipe: impl Read + Send + 'static, total: Arc<AtomicU64>) -> io::Result<Arc<Self>> {
        let capture = Arc::new(Self {
            bytes: Mutex::new(Vec::new()),
            done: AtomicBool::new(false),
            error: Mutex::new(None),
        });
        let target = capture.clone();
        thread::Builder::new()
            .name("hdl-output".into())
            .spawn(move || {
                let mut chunk = [0u8; 4096];
                loop {
                    match pipe.read(&mut chunk) {
                        Ok(0) => break,
                        Ok(n) => {
                            let previous = total.fetch_add(n as u64, Ordering::Relaxed);
                            let take = n.min(MAX_CAPTURE_BYTES.saturating_sub(previous) as usize);
                            target
                                .bytes
                                .lock()
                                .expect("capture lock")
                                .extend_from_slice(&chunk[..take]);
                            if previous + n as u64 > MAX_CAPTURE_BYTES {
                                break;
                            }
                        }
                        Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
                        Err(e) => {
                            *target.error.lock().expect("capture lock") = Some(e.to_string());
                            break;
                        }
                    }
                }
                target.done.store(true, Ordering::Release);
            })?;
        Ok(capture)
    }
    fn bytes(&self) -> Vec<u8> {
        self.bytes.lock().expect("capture lock").clone()
    }
}
struct RunningChild {
    child: Child,
    stopped: bool,
}
impl RunningChild {
    fn stop(&mut self) {
        if self.stopped {
            return;
        }
        self.stopped = true;
        #[cfg(unix)]
        unsafe {
            libc::kill(-(self.child.id() as libc::pid_t), libc::SIGKILL);
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Drop for RunningChild {
    fn drop(&mut self) {
        self.stop();
    }
}

impl<'a> Job<'a> {
    pub fn new(stage: &'static str, root: &'a Path, seconds: u64) -> Self {
        Self {
            stage,
            root,
            deadline: Instant::now() + Duration::from_secs(seconds),
            seconds,
            captured: Arc::new(AtomicU64::new(0)),
        }
    }
    fn category(&self, ordinary: &'static str, export: &'static str) -> &'static str {
        if self.stage == "export" {
            export
        } else {
            ordinary
        }
    }
    fn timeout(&self, name: &str) -> CliFailure {
        failure(
            self.stage,
            self.category("tool_timeout", "synthesis_timeout"),
            format!(
                "{name} exceeded the shared {} second deadline",
                self.seconds
            ),
            EXIT_INVALID,
        )
    }
    fn check_capture_limit(&self, name: &str) -> Result<(), CliFailure> {
        if self.captured.load(Ordering::Relaxed) > MAX_CAPTURE_BYTES {
            return Err(failure(
                self.stage,
                "resource_limit",
                format!("{name} output exceeds the {MAX_CAPTURE_BYTES} byte combined log limit"),
                EXIT_INVALID,
            ));
        }
        Ok(())
    }
    pub fn run(
        &self,
        name: &str,
        executable: &Path,
        args: &[OsString],
        cwd: &Path,
        links: &BTreeSet<PathBuf>,
    ) -> Result<ToolOutput, CliFailure> {
        if Instant::now() >= self.deadline {
            return Err(self.timeout(name));
        }
        let invoking_directory = std::env::current_dir().map_err(|e| io_failure(self.stage, e))?;
        let program = if executable.is_relative() && executable.components().count() > 1 {
            invoking_directory.join(executable)
        } else {
            executable.to_path_buf()
        };
        let (program, normalized_path) = if let Some(path) = std::env::var_os("PATH") {
            let absolute: Vec<_> = std::env::split_paths(&path)
                .map(|entry| {
                    if entry.is_absolute() {
                        entry
                    } else {
                        invoking_directory.join(entry)
                    }
                })
                .collect();
            match std::env::join_paths(&absolute) {
                Ok(normalized) => (program, Some(normalized)),
                Err(_) => {
                    // A legal Unix cwd may contain ':', which PATH cannot
                    // represent absolutely. Resolve the selected tool here
                    // and retain the original PATH for its subprocesses.
                    #[cfg(unix)]
                    let program = if executable.components().count() == 1 {
                        use std::os::unix::fs::PermissionsExt;
                        absolute
                            .iter()
                            .map(|p| p.join(executable))
                            .find(|p| {
                                fs::metadata(p).is_ok_and(|m| {
                                    m.is_file() && m.permissions().mode() & 0o111 != 0
                                })
                            })
                            .unwrap_or(program)
                    } else {
                        program
                    };
                    (program, Some(path))
                }
            }
        } else {
            (program, None)
        };
        let temp = self.root.join(".tool-tmp");
        fs::create_dir_all(&temp).map_err(|e| io_failure(self.stage, e))?;
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .env("TMPDIR", &temp)
            .env("TMP", &temp)
            .env("TEMP", &temp);
        if let Some(path) = normalized_path {
            command.env("PATH", path);
        }
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            let cpu = self
                .deadline
                .saturating_duration_since(Instant::now())
                .as_secs()
                + 2;
            unsafe {
                command.pre_exec(move || {
                    for (resource, maximum) in [
                        (libc::RLIMIT_AS, 2_u64 * 1024 * 1024 * 1024),
                        (libc::RLIMIT_FSIZE, MAX_FILE_BYTES),
                        (libc::RLIMIT_CORE, 0),
                        (libc::RLIMIT_CPU, cpu),
                    ] {
                        let mut old: libc::rlimit = std::mem::zeroed();
                        if libc::getrlimit(resource, &mut old) != 0 {
                            return Err(io::Error::last_os_error());
                        }
                        let cap = (maximum as libc::rlim_t).min(old.rlim_max);
                        let new = libc::rlimit {
                            rlim_cur: cap.min(old.rlim_cur),
                            rlim_max: cap,
                        };
                        if libc::setrlimit(resource, &new) != 0 {
                            return Err(io::Error::last_os_error());
                        }
                    }
                    Ok(())
                });
            }
        }
        let child=command.spawn().map_err(|e|failure(self.stage,self.category("tool_unavailable","synthesis_tool"),format!("could not run {name} '{}': {e}. Install the tool or select its executable path.",executable.display()),EXIT_IO))?;
        let mut running = RunningChild {
            child,
            stopped: false,
        };
        let stdout = Capture::start(
            running.child.stdout.take().expect("piped stdout"),
            self.captured.clone(),
        )
        .map_err(|e| io_failure(self.stage, e))?;
        let stderr = Capture::start(
            running.child.stderr.take().expect("piped stderr"),
            self.captured.clone(),
        )
        .map_err(|e| io_failure(self.stage, e))?;
        let mut status: Option<ExitStatus> = None;
        let mut next_scan = Instant::now();
        loop {
            self.check_capture_limit(name)?;
            if Instant::now() >= self.deadline {
                return Err(self.timeout(name));
            }
            if Instant::now() >= next_scan {
                scan_tree(self.root, links, self.stage)?;
                next_scan = Instant::now() + Duration::from_millis(50);
            }
            if status.is_none() {
                status = running
                    .child
                    .try_wait()
                    .map_err(|e| io_failure(self.stage, e))?;
            }
            if status.is_some()
                && stdout.done.load(Ordering::Acquire)
                && stderr.done.load(Ordering::Acquire)
            {
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }
        // Readers can finish between the limit check and the EOF checks.
        // Their Acquire completion flags make the final byte count visible.
        self.check_capture_limit(name)?;
        for capture in [&stdout, &stderr] {
            if let Some(message) = capture.error.lock().expect("capture lock").as_ref() {
                return Err(failure(
                    self.stage,
                    "tool_output",
                    format!("could not capture {name} output: {message}"),
                    EXIT_IO,
                ));
            }
        }
        running.stop();
        scan_tree(self.root, links, self.stage)?;
        let output = ToolOutput {
            stdout: stdout.bytes(),
            stderr: stderr.bytes(),
        };
        let status = status.expect("process completed");
        #[cfg(unix)]
        {
            use std::os::unix::process::ExitStatusExt;
            if matches!(status.signal(), Some(libc::SIGXFSZ | libc::SIGXCPU)) {
                return Err(failure(
                    self.stage,
                    "resource_limit",
                    format!("{name} exceeded its process resource limits"),
                    EXIT_INVALID,
                ));
            }
        }
        if !status.success() {
            let mut combined = output.stdout.clone();
            combined.extend_from_slice(&output.stderr);
            let start = combined.len().saturating_sub(MAX_TAIL);
            return Err(failure(
                self.stage,
                self.category("tool_failed", "synthesis_failed"),
                format!(
                    "{name} failed ({status}): {}",
                    clean_text(&combined[start..]).trim()
                ),
                EXIT_INVALID,
            ));
        }
        Ok(output)
    }
}
pub fn clean_text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes)
        .chars()
        .filter(|c| !c.is_control() || matches!(c, '\n' | '\t'))
        .collect()
}

/// Scan bounded private trees without following source include aliases.
pub fn scan_tree(
    root: &Path,
    allowed_links: &BTreeSet<PathBuf>,
    stage: &'static str,
) -> Result<Vec<PathBuf>, CliFailure> {
    let mut stack = vec![(root.to_path_buf(), 0)];
    let mut entries = 0;
    let mut total = 0u64;
    let mut files = Vec::new();
    while let Some((dir, depth)) = stack.pop() {
        if depth > MAX_DEPTH {
            return Err(failure(
                stage,
                "resource_limit",
                "tool workspace exceeds maximum directory depth",
                EXIT_INVALID,
            ));
        }
        let list = match fs::read_dir(&dir) {
            Ok(v) => v,
            Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
            Err(e) => return Err(io_failure(stage, e)),
        };
        for entry in list {
            let entry = entry.map_err(|e| io_failure(stage, e))?;
            entries += 1;
            if entries > MAX_ENTRIES {
                return Err(failure(
                    stage,
                    "resource_limit",
                    format!("tool workspace exceeds {MAX_ENTRIES} entries"),
                    EXIT_INVALID,
                ));
            }
            let path = entry.path();
            let info = match fs::symlink_metadata(&path) {
                Ok(v) => v,
                Err(e) if e.kind() == io::ErrorKind::NotFound => continue,
                Err(e) => return Err(io_failure(stage, e)),
            };
            if info.file_type().is_symlink() {
                if allowed_links.contains(&path) {
                    continue;
                }
                return Err(failure(
                    stage,
                    "invalid_artifact",
                    format!("tool produced symlink '{}'", path.display()),
                    EXIT_INVALID,
                ));
            }
            if info.is_dir() {
                stack.push((path, depth + 1));
            } else if info.is_file() {
                total = total.saturating_add(info.len());
                if info.len() > MAX_FILE_BYTES || total > MAX_WORKSPACE_BYTES {
                    return Err(failure(
                        stage,
                        "resource_limit",
                        "tool workspace exceeds file or total byte limits",
                        EXIT_INVALID,
                    ));
                }
                files.push(path);
            } else {
                return Err(failure(
                    stage,
                    "invalid_artifact",
                    format!("tool produced non-regular artifact '{}'", path.display()),
                    EXIT_INVALID,
                ));
            }
        }
    }
    files.sort();
    Ok(files)
}

pub fn publish(
    destination: &Path,
    stage: &'static str,
    populate: impl FnOnce(&Path) -> Result<(), CliFailure>,
) -> Result<(), CliFailure> {
    new_destination(destination, stage)?;
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| io_failure(stage, e))?;
    let directory =
        ScratchDirectory::new_in(parent, "publish").map_err(|e| io_failure(stage, e))?;
    populate(&directory.path)?;
    rename_new(&directory.path, destination).map_err(|e| io_failure(stage, e))
}
pub fn copy_artifacts(
    source: &Path,
    destination: &Path,
    stage: &'static str,
) -> Result<Vec<String>, CliFailure> {
    let files = scan_tree(source, &BTreeSet::new(), stage)?;
    let mut names = Vec::new();
    publish(destination, stage, |target| {
        for path in &files {
            let relative = path.strip_prefix(source).expect("scanned descendant");
            if relative
                .components()
                .any(|c| !matches!(c, Component::Normal(_)))
            {
                return Err(failure(
                    stage,
                    "invalid_artifact",
                    "unsafe generated artifact path",
                    EXIT_INVALID,
                ));
            }
            let output = target.join(relative);
            fs::create_dir_all(output.parent().expect("file parent"))
                .map_err(|e| io_failure(stage, e))?;
            write(
                &output,
                read_regular(path, MAX_FILE_BYTES, stage, true)?,
                stage,
            )?;
            names.push(relative.to_string_lossy().into_owned());
        }
        Ok(())
    })?;
    Ok(names)
}
#[cfg(target_os = "linux")]
fn rename_new(source: &Path, destination: &Path) -> io::Result<()> {
    use std::{ffi::CString, os::unix::ffi::OsStrExt};
    let source = CString::new(source.as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let destination = CString::new(destination.as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            source.as_ptr(),
            libc::AT_FDCWD,
            destination.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if result == 0 {
        Ok(())
    } else {
        Err(io::Error::last_os_error())
    }
}
#[cfg(not(target_os = "linux"))]
fn rename_new(source: &Path, destination: &Path) -> io::Result<()> {
    fs::create_dir(destination)?;
    #[cfg(windows)]
    fs::remove_dir(destination)?;
    let result = fs::rename(source, destination);
    if result.is_err() {
        let _ = fs::remove_dir(destination);
    }
    result
}
