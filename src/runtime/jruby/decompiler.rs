use super::{
    classpath::SourceOrigin, java_catalog::JavaClassDeclaration,
    source_navigation::ResolvedJavaSource,
};
use ruby_fast_lsp_jvm_metadata::{
    locate_java_source_declarations, JavaSourceClassLocation, JavaSourceLimits,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use walkdir::WalkDir;

pub const CFR_VERSION: &str = "0.152";
pub const CFR_SHA256: &str = "f686e8f3ded377d7bc87d216a90e9e9512df4156e75b06c655a16648ae8765b2";
const CFR_OPTIONS_ID: &str = "cfr-0.152|silent=true|comments=false|heap=128m|direct=32m|metaspace=96m|code-cache=32m|compressed-class=16m|resident=256m|locale=C|v2";
const MIB: u64 = 1024 * 1024;
const DEFAULT_MAX_PROCESS_RESIDENT_BYTES: u64 = 256 * MIB;
static ACTIVE_DECOMPILERS: AtomicUsize = AtomicUsize::new(0);
static DECOMPILATION_RUN_ID: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JavaDecompilerAsset {
    pub path: PathBuf,
    pub version: String,
    pub fingerprint_sha256: String,
}

#[derive(Debug, Clone, Copy)]
pub struct JavaDecompilerLimits {
    pub max_artifact_bytes: u64,
    pub max_archive_entries: usize,
    pub max_class_files: usize,
    pub max_class_bytes: u64,
    pub max_total_class_bytes: u64,
    pub max_generated_files: usize,
    pub max_generated_bytes: u64,
    pub max_parallel_processes: usize,
    pub max_process_resident_bytes: u64,
    pub timeout: Duration,
}

impl Default for JavaDecompilerLimits {
    fn default() -> Self {
        Self {
            max_artifact_bytes: 512 * 1024 * 1024,
            max_archive_entries: 1_000_000,
            max_class_files: 256,
            max_class_bytes: 16 * 1024 * 1024,
            max_total_class_bytes: 64 * 1024 * 1024,
            max_generated_files: 256,
            max_generated_bytes: 32 * 1024 * 1024,
            max_parallel_processes: 2,
            max_process_resident_bytes: DEFAULT_MAX_PROCESS_RESIDENT_BYTES,
            timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JavaDecompilerError {
    MissingJava(PathBuf),
    MissingAsset(PathBuf),
    AssetFingerprintMismatch(PathBuf),
    ArtifactFingerprintMismatch(PathBuf),
    Read {
        path: PathBuf,
        message: String,
    },
    InvalidArchive {
        path: PathBuf,
        message: String,
    },
    LimitExceeded(&'static str),
    ProcessLimit,
    ProcessMemoryInspection(String),
    ProcessMemoryLimitExceeded {
        limit_bytes: u64,
        observed_bytes: u64,
    },
    Spawn(String),
    Timeout,
    AbnormalExit(Option<i32>),
    InvalidOutput(String),
    AmbiguousOutput(String),
}

#[derive(Debug, Clone)]
pub struct JavaDecompiler {
    java_executable: PathBuf,
    asset: JavaDecompilerAsset,
    cache_root: PathBuf,
    limits: JavaDecompilerLimits,
}

pub fn discover_bundled_cfr_asset() -> Result<JavaDecompilerAsset, JavaDecompilerError> {
    let mut candidates = Vec::new();
    if let Some(path) = std::env::var_os("RUBY_FAST_LSP_CFR_JAR") {
        candidates.push(PathBuf::from(path));
    }
    if let Ok(executable) = std::env::current_exe() {
        let mut parent = executable.parent();
        for _ in 0..3 {
            if let Some(directory) = parent {
                candidates.push(directory.join("jruby-decompiler/cfr-0.152.jar"));
                parent = directory.parent();
            }
        }
    }
    if cfg!(any(debug_assertions, test)) {
        candidates.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("support/jruby/decompiler/cfr-0.152.jar"),
        );
    }
    let path = candidates
        .iter()
        .find(|candidate| candidate.is_file())
        .cloned()
        .ok_or_else(|| {
            JavaDecompilerError::MissingAsset(
                candidates
                    .into_iter()
                    .next()
                    .unwrap_or_else(|| PathBuf::from("jruby-decompiler/cfr-0.152.jar")),
            )
        })?;
    Ok(JavaDecompilerAsset {
        path,
        version: CFR_VERSION.to_string(),
        fingerprint_sha256: CFR_SHA256.to_string(),
    })
}

impl JavaDecompiler {
    pub fn new(
        java_executable: PathBuf,
        asset: JavaDecompilerAsset,
        cache_root: PathBuf,
        limits: JavaDecompilerLimits,
    ) -> Result<Self, JavaDecompilerError> {
        if !java_executable.is_file() {
            return Err(JavaDecompilerError::MissingJava(java_executable));
        }
        if !asset.path.is_file() {
            return Err(JavaDecompilerError::MissingAsset(asset.path));
        }
        assert!(
            limits.max_parallel_processes > 0,
            "INVARIANT VIOLATED: Java decompiler process limit is zero. \
             This is a configuration bug because no request could ever acquire a bounded permit. \
             Fix: configure at least one bounded decompiler process."
        );
        assert!(
            !limits.timeout.is_zero(),
            "INVARIANT VIOLATED: Java decompiler timeout is zero. \
             This is a configuration bug because every valid process would time out before launch. \
             Fix: configure a positive bounded wall-clock timeout."
        );
        assert!(
            limits.max_process_resident_bytes > 0,
            "INVARIANT VIOLATED: Java decompiler resident-memory limit is zero. \
             This is a configuration bug because no valid JVM child could remain below the limit. \
             Fix: configure a positive measured resident-memory bound."
        );
        Ok(Self {
            java_executable,
            asset,
            cache_root,
            limits,
        })
    }

    pub fn decompile(
        &self,
        declaration: &JavaClassDeclaration,
    ) -> Result<Option<ResolvedJavaSource>, JavaDecompilerError> {
        let asset_bytes =
            fs::read(&self.asset.path).map_err(|error| JavaDecompilerError::Read {
                path: self.asset.path.clone(),
                message: error.to_string(),
            })?;
        if format!("{:x}", Sha256::digest(&asset_bytes)) != self.asset.fingerprint_sha256 {
            return Err(JavaDecompilerError::AssetFingerprintMismatch(
                self.asset.path.clone(),
            ));
        }
        if self.asset.version != CFR_VERSION {
            return Err(JavaDecompilerError::InvalidOutput(format!(
                "unsupported CFR asset version `{}`; expected `{CFR_VERSION}`",
                self.asset.version
            )));
        }
        let java_bytes =
            fs::read(&self.java_executable).map_err(|error| JavaDecompilerError::Read {
                path: self.java_executable.clone(),
                message: error.to_string(),
            })?;
        let java_fingerprint = format!("{:x}", Sha256::digest(&java_bytes));
        let artifact_bytes = read_bounded_file(
            &declaration.artifact_path,
            self.limits.max_artifact_bytes,
            "Java classpath artifact bytes",
        )?;
        if format!("{:x}", Sha256::digest(&artifact_bytes))
            != declaration.artifact_fingerprint_sha256
        {
            return Err(JavaDecompilerError::ArtifactFingerprintMismatch(
                declaration.artifact_path.clone(),
            ));
        }
        let class_relative = safe_internal_class_path(&declaration.class.name)?;
        let options_fingerprint = format!("{:x}", Sha256::digest(CFR_OPTIONS_ID.as_bytes()));
        let cache_key = self
            .cache_root
            .join("decompiled")
            .join(&self.asset.fingerprint_sha256)
            .join(java_fingerprint)
            .join(&declaration.artifact_fingerprint_sha256)
            .join(options_fingerprint)
            .join(&class_relative);
        let implementation_root = cache_key.join("implementation");
        if let Some((path, content, location)) =
            find_verified_output(&implementation_root, declaration, self.limits)?
        {
            return Ok(Some(ResolvedJavaSource {
                path,
                content,
                origin: SourceOrigin::Decompiled,
                location,
            }));
        }

        let _permit = ProcessPermit::acquire(self.limits.max_parallel_processes)?;
        let run_id = DECOMPILATION_RUN_ID.fetch_add(1, Ordering::Relaxed);
        let work_root = cache_key.join(format!(".work-{}-{run_id}", std::process::id()));
        let input_root = work_root.join("input");
        let output_root = work_root.join("output");
        fs::create_dir_all(&input_root).map_err(|error| JavaDecompilerError::Read {
            path: input_root.clone(),
            message: error.to_string(),
        })?;
        fs::create_dir_all(&output_root).map_err(|error| JavaDecompilerError::Read {
            path: output_root.clone(),
            message: error.to_string(),
        })?;
        let selected_input =
            extract_class_family(declaration, &artifact_bytes, &input_root, self.limits)?;
        run_cfr(
            &self.java_executable,
            &self.asset.path,
            &selected_input,
            &output_root,
            &work_root,
            self.limits.timeout,
            self.limits.max_process_resident_bytes,
        )?;
        let Some((_generated_path, content, location)) =
            find_verified_output(&output_root, declaration, self.limits)?
        else {
            return Err(JavaDecompilerError::InvalidOutput(format!(
                "CFR produced no source matching `{}`",
                declaration.class.name
            )));
        };
        let source_relative = safe_source_path(declaration)?;
        let final_path =
            materialize_decompiled_source(&implementation_root, &source_relative, &content)?;
        let _ = fs::remove_dir_all(&work_root);
        Ok(Some(ResolvedJavaSource {
            path: final_path,
            content,
            origin: SourceOrigin::Decompiled,
            location,
        }))
    }
}

struct ProcessPermit;

impl ProcessPermit {
    fn acquire(limit: usize) -> Result<Self, JavaDecompilerError> {
        let mut active = ACTIVE_DECOMPILERS.load(Ordering::Acquire);
        loop {
            if active >= limit {
                return Err(JavaDecompilerError::ProcessLimit);
            }
            match ACTIVE_DECOMPILERS.compare_exchange_weak(
                active,
                active + 1,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => return Ok(Self),
                Err(current) => active = current,
            }
        }
    }
}

impl Drop for ProcessPermit {
    fn drop(&mut self) {
        let previous = ACTIVE_DECOMPILERS.fetch_sub(1, Ordering::AcqRel);
        assert!(
            previous > 0,
            "INVARIANT VIOLATED: Java decompiler process permit underflowed. \
             This is a bug because every permit increments the global count exactly once. \
             Fix: keep permit acquisition and RAII release paired."
        );
    }
}

fn read_bounded_file(
    path: &Path,
    max_bytes: u64,
    limit_name: &'static str,
) -> Result<Vec<u8>, JavaDecompilerError> {
    let metadata = fs::metadata(path).map_err(|error| JavaDecompilerError::Read {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    if metadata.len() > max_bytes {
        return Err(JavaDecompilerError::LimitExceeded(limit_name));
    }
    fs::read(path).map_err(|error| JavaDecompilerError::Read {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn safe_internal_class_path(internal_name: &str) -> Result<PathBuf, JavaDecompilerError> {
    let mut path = PathBuf::new();
    for component in internal_name.split('/') {
        if component.is_empty() || component == "." || component == ".." || component.contains('\\')
        {
            return Err(JavaDecompilerError::InvalidOutput(format!(
                "unsafe JVM internal class name `{internal_name}`"
            )));
        }
        path.push(component);
    }
    Ok(path)
}

fn safe_source_path(declaration: &JavaClassDeclaration) -> Result<PathBuf, JavaDecompilerError> {
    let internal = safe_internal_class_path(&declaration.class.name)?;
    let parent = internal.parent().unwrap_or_else(|| Path::new(""));
    let outer = internal
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(|name| name.split('$').next())
        .ok_or_else(|| {
            JavaDecompilerError::InvalidOutput(format!(
                "class `{}` has no safe outer name",
                declaration.class.name
            ))
        })?;
    let source_file = declaration
        .class
        .source_file
        .clone()
        .unwrap_or_else(|| format!("{outer}.java"));
    if source_file.is_empty()
        || source_file == "."
        || source_file == ".."
        || source_file.contains('/')
        || source_file.contains('\\')
        || !source_file.ends_with(".java")
    {
        return Err(JavaDecompilerError::InvalidOutput(format!(
            "class `{}` has unsafe SourceFile `{source_file}`",
            declaration.class.name
        )));
    }
    Ok(parent.join(source_file))
}

fn extract_class_family(
    declaration: &JavaClassDeclaration,
    artifact_bytes: &[u8],
    input_root: &Path,
    limits: JavaDecompilerLimits,
) -> Result<PathBuf, JavaDecompilerError> {
    let mut archive = zip::ZipArchive::new(Cursor::new(artifact_bytes)).map_err(|error| {
        JavaDecompilerError::InvalidArchive {
            path: declaration.artifact_path.clone(),
            message: error.to_string(),
        }
    })?;
    if archive.len() > limits.max_archive_entries {
        return Err(JavaDecompilerError::LimitExceeded(
            "Java artifact archive entries",
        ));
    }
    let selected_entry = Path::new(&declaration.entry_name);
    let selected_parent = selected_entry.parent().unwrap_or_else(|| Path::new(""));
    let selected_name = selected_entry
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| {
            JavaDecompilerError::InvalidOutput(format!(
                "selected class archive entry `{}` is not a safe UTF-8 path",
                declaration.entry_name
            ))
        })?;
    let outer_stem = selected_name
        .strip_suffix(".class")
        .and_then(|name| name.split('$').next())
        .ok_or_else(|| {
            JavaDecompilerError::InvalidOutput(format!(
                "selected archive entry `{}` is not a class",
                declaration.entry_name
            ))
        })?;
    let outer_class_name = format!("{outer_stem}.class");
    let nested_prefix = format!("{outer_stem}$");
    let mut extracted = Vec::<(String, Vec<u8>)>::new();
    let mut total_bytes = 0u64;
    for index in 0..archive.len() {
        let mut entry =
            archive
                .by_index(index)
                .map_err(|error| JavaDecompilerError::InvalidArchive {
                    path: declaration.artifact_path.clone(),
                    message: error.to_string(),
                })?;
        let Some(path) = entry.enclosed_name() else {
            return Err(JavaDecompilerError::InvalidArchive {
                path: declaration.artifact_path.clone(),
                message: format!("archive entry `{}` escapes its root", entry.name()),
            });
        };
        if path.parent().unwrap_or_else(|| Path::new("")) != selected_parent {
            continue;
        }
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        let belongs_to_family = name == outer_class_name
            || (name.starts_with(&nested_prefix) && name.ends_with(".class"));
        if !belongs_to_family {
            continue;
        }
        if extracted.len() >= limits.max_class_files {
            return Err(JavaDecompilerError::LimitExceeded(
                "Java class family files",
            ));
        }
        if entry.size() > limits.max_class_bytes {
            return Err(JavaDecompilerError::LimitExceeded(
                "individual Java class bytes",
            ));
        }
        total_bytes =
            total_bytes
                .checked_add(entry.size())
                .ok_or(JavaDecompilerError::LimitExceeded(
                    "Java class family bytes",
                ))?;
        if total_bytes > limits.max_total_class_bytes {
            return Err(JavaDecompilerError::LimitExceeded(
                "Java class family bytes",
            ));
        }
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry
            .read_to_end(&mut bytes)
            .map_err(|error| JavaDecompilerError::InvalidArchive {
                path: declaration.artifact_path.clone(),
                message: error.to_string(),
            })?;
        extracted.push((name.to_string(), bytes));
    }
    if !extracted.iter().any(|(name, _)| name == selected_name) {
        return Err(JavaDecompilerError::InvalidArchive {
            path: declaration.artifact_path.clone(),
            message: format!(
                "selected class entry `{}` disappeared from verified artifact",
                declaration.entry_name
            ),
        });
    }
    let package_relative = safe_internal_class_path(&declaration.class.name)?;
    let package = package_relative.parent().unwrap_or_else(|| Path::new(""));
    let package_root = input_root.join(package);
    fs::create_dir_all(&package_root).map_err(|error| JavaDecompilerError::Read {
        path: package_root.clone(),
        message: error.to_string(),
    })?;
    for (name, bytes) in &extracted {
        let path = package_root.join(name);
        fs::write(&path, bytes).map_err(|error| JavaDecompilerError::Read {
            path,
            message: error.to_string(),
        })?;
    }
    let outer_input = package_root.join(outer_class_name);
    if outer_input.is_file() {
        Ok(outer_input)
    } else {
        Ok(package_root.join(selected_name))
    }
}

fn run_cfr(
    java_executable: &Path,
    asset: &Path,
    selected_input: &Path,
    output_root: &Path,
    work_root: &Path,
    timeout: Duration,
    max_process_resident_bytes: u64,
) -> Result<(), JavaDecompilerError> {
    let mut child = Command::new(java_executable)
        .arg("-Xmx128m")
        .arg("-XX:MaxDirectMemorySize=32m")
        .arg("-XX:MaxMetaspaceSize=96m")
        .arg("-XX:ReservedCodeCacheSize=32m")
        .arg("-XX:CompressedClassSpaceSize=16m")
        .arg("-Duser.language=en")
        .arg("-Duser.country=US")
        .arg("-Dfile.encoding=UTF-8")
        .arg("-Duser.timezone=UTC")
        .arg("-jar")
        .arg(asset)
        .arg(selected_input)
        .arg("--outputdir")
        .arg(output_root)
        .arg("--silent")
        .arg("true")
        .arg("--comments")
        .arg("false")
        .current_dir(work_root)
        .env_clear()
        .env("LANG", "C")
        .env("LC_ALL", "C")
        .env("TZ", "UTC")
        .stdin(Stdio::null())
        // CFR is deliberately silent and both process streams are discarded,
        // giving them a strict zero-byte server memory and cache bound.
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|error| JavaDecompilerError::Spawn(error.to_string()))?;
    wait_for_bounded_child(&mut child, timeout, max_process_resident_bytes)
}

fn wait_for_bounded_child(
    child: &mut std::process::Child,
    timeout: Duration,
    max_process_resident_bytes: u64,
) -> Result<(), JavaDecompilerError> {
    assert!(
        max_process_resident_bytes > 0,
        "INVARIANT VIOLATED: bounded child wait received a zero resident-memory limit. \
         This is a bug because every live process would violate the limit. \
         Fix: validate a positive process limit before spawning the JVM."
    );
    let started = Instant::now();
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| JavaDecompilerError::Spawn(error.to_string()))?
        {
            return if status.success() {
                Ok(())
            } else {
                Err(JavaDecompilerError::AbnormalExit(status.code()))
            };
        }
        let resident_bytes = match resident_memory_bytes(child.id()) {
            Ok(Some(resident_bytes)) => resident_bytes,
            Ok(None) => {
                thread::sleep(Duration::from_millis(1));
                continue;
            }
            Err(message) => {
                terminate_and_reap(child);
                return Err(JavaDecompilerError::ProcessMemoryInspection(message));
            }
        };
        if resident_bytes > max_process_resident_bytes {
            terminate_and_reap(child);
            return Err(JavaDecompilerError::ProcessMemoryLimitExceeded {
                limit_bytes: max_process_resident_bytes,
                observed_bytes: resident_bytes,
            });
        }
        if started.elapsed() >= timeout {
            terminate_and_reap(child);
            return Err(JavaDecompilerError::Timeout);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn terminate_and_reap(child: &mut std::process::Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(target_os = "macos")]
fn resident_memory_bytes(pid: u32) -> Result<Option<u64>, String> {
    let pid = i32::try_from(pid)
        .map_err(|_| format!("child process ID `{pid}` does not fit macOS pid_t"))?;
    let mut task_info = std::mem::MaybeUninit::<libc::proc_taskinfo>::zeroed();
    let expected_size = std::mem::size_of::<libc::proc_taskinfo>();
    let buffer_size = i32::try_from(expected_size)
        .map_err(|_| "macOS proc_taskinfo size does not fit c_int".to_string())?;
    let returned = unsafe {
        libc::proc_pidinfo(
            pid,
            libc::PROC_PIDTASKINFO,
            0,
            task_info.as_mut_ptr().cast(),
            buffer_size,
        )
    };
    if returned == 0 {
        let error = std::io::Error::last_os_error();
        return if error.raw_os_error() == Some(libc::ESRCH) {
            Ok(None)
        } else {
            Err(format!("proc_pidinfo failed for child {pid}: {error}"))
        };
    }
    if returned != buffer_size {
        return Err(format!(
            "proc_pidinfo returned {returned} bytes for child {pid}; expected {buffer_size}"
        ));
    }
    let task_info = unsafe { task_info.assume_init() };
    Ok(Some(task_info.pti_resident_size))
}

#[cfg(target_os = "linux")]
fn resident_memory_bytes(pid: u32) -> Result<Option<u64>, String> {
    let statm_path = PathBuf::from(format!("/proc/{pid}/statm"));
    let statm = match fs::read_to_string(&statm_path) {
        Ok(statm) => statm,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("failed to read {}: {error}", statm_path.display())),
    };
    let resident_pages = statm
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| format!("{} has no resident-page field", statm_path.display()))?
        .parse::<u64>()
        .map_err(|error| {
            format!(
                "{} has an invalid resident-page field: {error}",
                statm_path.display()
            )
        })?;
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    if page_size <= 0 {
        return Err(format!(
            "sysconf(_SC_PAGESIZE) failed: {}",
            std::io::Error::last_os_error()
        ));
    }
    let page_size = u64::try_from(page_size)
        .map_err(|_| format!("negative page size `{page_size}` escaped validation"))?;
    resident_pages
        .checked_mul(page_size)
        .map(Some)
        .ok_or_else(|| format!("resident bytes overflowed for child {pid}"))
}

#[cfg(target_os = "windows")]
fn resident_memory_bytes(pid: u32) -> Result<Option<u64>, String> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ,
    };

    let handle = unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid) };
    if handle.is_null() {
        let error = std::io::Error::last_os_error();
        return if error.raw_os_error() == Some(87) {
            Ok(None)
        } else {
            Err(format!("OpenProcess failed for child {pid}: {error}"))
        };
    }
    let mut counters = std::mem::MaybeUninit::<PROCESS_MEMORY_COUNTERS>::zeroed();
    let size = u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>())
        .map_err(|_| "PROCESS_MEMORY_COUNTERS size does not fit u32".to_string())?;
    let succeeded = unsafe { GetProcessMemoryInfo(handle, counters.as_mut_ptr(), size) };
    unsafe {
        CloseHandle(handle);
    }
    if succeeded == 0 {
        return Err(format!(
            "GetProcessMemoryInfo failed for child {pid}: {}",
            std::io::Error::last_os_error()
        ));
    }
    let counters = unsafe { counters.assume_init() };
    u64::try_from(counters.WorkingSetSize)
        .map(Some)
        .map_err(|_| format!("working-set size for child {pid} does not fit u64"))
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn resident_memory_bytes(pid: u32) -> Result<Option<u64>, String> {
    Err(format!(
        "resident-memory enforcement is unavailable for child {pid} on this operating system"
    ))
}

fn find_verified_output(
    root: &Path,
    declaration: &JavaClassDeclaration,
    limits: JavaDecompilerLimits,
) -> Result<Option<(PathBuf, String, JavaSourceClassLocation)>, JavaDecompilerError> {
    if !root.is_dir() {
        return Ok(None);
    }
    let mut files = 0usize;
    let mut total_bytes = 0u64;
    let mut matches = Vec::new();
    for entry in WalkDir::new(root).follow_links(false) {
        let entry = entry.map_err(|error| JavaDecompilerError::Read {
            path: root.to_path_buf(),
            message: error.to_string(),
        })?;
        if entry.file_type().is_symlink() || !entry.file_type().is_file() {
            continue;
        }
        files = files
            .checked_add(1)
            .ok_or(JavaDecompilerError::LimitExceeded(
                "decompiler generated files",
            ))?;
        if files > limits.max_generated_files {
            return Err(JavaDecompilerError::LimitExceeded(
                "decompiler generated files",
            ));
        }
        let metadata = entry
            .metadata()
            .map_err(|error| JavaDecompilerError::Read {
                path: entry.path().to_path_buf(),
                message: error.to_string(),
            })?;
        total_bytes =
            total_bytes
                .checked_add(metadata.len())
                .ok_or(JavaDecompilerError::LimitExceeded(
                    "decompiler generated bytes",
                ))?;
        if total_bytes > limits.max_generated_bytes {
            return Err(JavaDecompilerError::LimitExceeded(
                "decompiler generated bytes",
            ));
        }
        if entry
            .path()
            .extension()
            .and_then(|extension| extension.to_str())
            != Some("java")
        {
            continue;
        }
        let content =
            fs::read_to_string(entry.path()).map_err(|error| JavaDecompilerError::Read {
                path: entry.path().to_path_buf(),
                message: error.to_string(),
            })?;
        match locate_java_source_declarations(
            &declaration.class,
            &content,
            JavaSourceLimits {
                max_source_bytes: usize::try_from(limits.max_generated_bytes).unwrap_or(usize::MAX),
                ..JavaSourceLimits::default()
            },
        ) {
            Ok(Some(location)) => matches.push((entry.path().to_path_buf(), content, location)),
            Ok(None) => {}
            Err(error) => {
                return Err(JavaDecompilerError::InvalidOutput(format!(
                    "generated Java source `{}` failed validation: {error:?}",
                    entry.path().display()
                )))
            }
        }
    }
    if matches.len() > 1 {
        return Err(JavaDecompilerError::AmbiguousOutput(
            declaration.class.name.clone(),
        ));
    }
    Ok(matches.pop())
}

fn materialize_decompiled_source(
    root: &Path,
    relative: &Path,
    content: &str,
) -> Result<PathBuf, JavaDecompilerError> {
    let path = root.join(relative);
    let parent = path.parent().expect(
        "INVARIANT VIOLATED: decompiled implementation path has no parent. \
         This is a bug because the cache root and safe Java source path are non-empty. \
         Fix: preserve both components during materialization.",
    );
    fs::create_dir_all(parent).map_err(|error| JavaDecompilerError::Read {
        path: parent.to_path_buf(),
        message: error.to_string(),
    })?;
    if fs::read_to_string(&path).is_ok_and(|existing| existing == content) {
        return Ok(path);
    }
    let temporary = path.with_extension(format!(
        "java.tmp-{}-{}",
        std::process::id(),
        DECOMPILATION_RUN_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let mut file = fs::File::create(&temporary).map_err(|error| JavaDecompilerError::Read {
        path: temporary.clone(),
        message: error.to_string(),
    })?;
    file.write_all(content.as_bytes())
        .map_err(|error| JavaDecompilerError::Read {
            path: temporary.clone(),
            message: error.to_string(),
        })?;
    file.sync_all().map_err(|error| JavaDecompilerError::Read {
        path: temporary.clone(),
        message: error.to_string(),
    })?;
    fs::rename(&temporary, &path).map_err(|error| JavaDecompilerError::Read {
        path: path.clone(),
        message: error.to_string(),
    })?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::jruby::classpath::ProjectClasspath;
    use crate::runtime::jruby::classpath::{ArtifactKind, ArtifactOrigin, ClasspathArtifact};
    use crate::runtime::jruby::java_catalog::build_project_java_catalog;
    use ruby_fast_lsp_jvm_metadata::ArchiveLimits;
    use sha2::{Digest, Sha256};
    use std::fs;
    use std::io::{Cursor, Write};
    use zip::write::SimpleFileOptions;

    fn decode_hex(source: &str) -> Vec<u8> {
        let digits = source
            .bytes()
            .filter(|byte| !byte.is_ascii_whitespace())
            .collect::<Vec<_>>();
        digits
            .chunks_exact(2)
            .map(|pair| {
                u8::from_str_radix(
                    std::str::from_utf8(pair).expect("fixture hex must be ASCII"),
                    16,
                )
                .expect("fixture byte must be valid hex")
            })
            .collect()
    }

    fn fixture_jar(class: &[u8]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        writer
            .start_file("fixtures/RichFixture.class", SimpleFileOptions::default())
            .unwrap();
        writer.write_all(class).unwrap();
        writer.finish().unwrap().into_inner()
    }

    fn fixture_declaration(root: &std::path::Path) -> JavaClassDeclaration {
        let class = decode_hex(include_str!(
            "../../../crates/jvm-metadata/fixtures/rich_fixture.class.hex"
        ));
        let jar = fixture_jar(&class);
        let path = root.join("rich.jar");
        fs::write(&path, &jar).unwrap();
        let file_identity = crate::runtime::jruby::classpath::SourceFileIdentity {
            byte_length: jar.len() as u64,
            modified: fs::metadata(&path).unwrap().modified().unwrap(),
        };
        let classpath = ProjectClasspath {
            project_root: root.to_path_buf(),
            artifacts: vec![ClasspathArtifact {
                path,
                origin: ArtifactOrigin::Explicit,
                kind: ArtifactKind::Jar,
                fingerprint_sha256: format!("{:x}", Sha256::digest(&jar)),
                byte_length: jar.len() as u64,
                file_identity,
            }],
            sources: Vec::new(),
            unresolved: Vec::new(),
            fingerprint_sha256: "fixture-classpath".to_string(),
        };
        build_project_java_catalog(&classpath, 17, ArchiveLimits::default())
            .unwrap()
            .classes
            .remove("fixtures/RichFixture")
            .unwrap()
    }

    fn java_executable() -> PathBuf {
        for candidate in [
            std::env::var_os("JAVA_HOME")
                .map(PathBuf::from)
                .map(|home| home.join("bin/java")),
            Some(PathBuf::from("/opt/homebrew/opt/openjdk/bin/java")),
            Some(PathBuf::from("/usr/local/opt/openjdk/bin/java")),
        ]
        .into_iter()
        .flatten()
        {
            if candidate.is_file() {
                return candidate;
            }
        }
        panic!("a real JDK java executable is required for the decompiler acceptance test");
    }

    fn bundled_asset() -> JavaDecompilerAsset {
        JavaDecompilerAsset {
            path: PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("support/jruby/decompiler/cfr-0.152.jar"),
            version: CFR_VERSION.to_string(),
            fingerprint_sha256: CFR_SHA256.to_string(),
        }
    }

    #[test]
    fn decompiles_only_the_selected_class_and_returns_verified_implementation_ranges() {
        let fixture = tempfile::tempdir().unwrap();
        let declaration = fixture_declaration(fixture.path());
        let cache = fixture.path().join("cache");
        let decompiler = JavaDecompiler::new(
            java_executable(),
            bundled_asset(),
            cache.clone(),
            JavaDecompilerLimits::default(),
        )
        .unwrap();

        let resolved = decompiler
            .decompile(&declaration)
            .expect("decompilation must succeed")
            .expect("decompiled source must match class metadata");

        assert_eq!(resolved.origin, SourceOrigin::Decompiled);
        assert!(resolved.path.starts_with(cache));
        assert!(resolved
            .content
            .contains("return List.of(prefix + values.length);"));
        let combine = resolved
            .location
            .methods
            .iter()
            .find(|method| method.name == "combine")
            .expect("decompiled method must have a verified metadata identity");
        assert_eq!(
            &resolved.content[combine.name_range.start as usize..combine.name_range.end as usize],
            "combine"
        );
        assert_eq!(
            decompiler.decompile(&declaration).unwrap().unwrap(),
            resolved,
            "a repeated request must reuse the deterministic verified cache"
        );
    }

    #[test]
    fn rejects_a_bundled_decompiler_checksum_mismatch_before_execution() {
        let fixture = tempfile::tempdir().unwrap();
        let declaration = fixture_declaration(fixture.path());
        let mut asset = bundled_asset();
        asset.fingerprint_sha256 = "0".repeat(64);
        let decompiler = JavaDecompiler::new(
            java_executable(),
            asset.clone(),
            fixture.path().join("cache"),
            JavaDecompilerLimits::default(),
        )
        .unwrap();

        assert_eq!(
            decompiler.decompile(&declaration),
            Err(JavaDecompilerError::AssetFingerprintMismatch(asset.path))
        );
    }

    #[test]
    fn default_limits_bound_total_decompiler_resident_memory() {
        assert_eq!(
            JavaDecompilerLimits::default().max_process_resident_bytes,
            256 * 1024 * 1024,
            "the JVM child must fit the same conservative 256 MiB resource claim used by JRuby indexing and interactive materialization"
        );
    }

    #[cfg(unix)]
    #[test]
    fn child_exceeding_resident_memory_limit_is_killed_and_reaped() {
        let mut child = Command::new("sleep")
            .arg("5")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("the platform sleep command must launch");

        let result = wait_for_bounded_child(&mut child, Duration::from_secs(2), 1);

        assert!(
            matches!(
                result,
                Err(JavaDecompilerError::ProcessMemoryLimitExceeded {
                    limit_bytes: 1,
                    observed_bytes
                }) if observed_bytes > 1
            ),
            "the resident-memory monitor must report the measured overage: {result:?}"
        );
        assert!(
            child.try_wait().unwrap().is_some(),
            "a memory-limited child must be reaped before the error is returned"
        );
    }
}
