//! Dataset fingerprints, build and machine identity, and process resource usage evidence.

use sha2::{Digest, Sha256};

pub(crate) fn hash_length_prefixed(hasher: &mut Sha256, bytes: &[u8]) {
    let length = u64::try_from(bytes.len()).expect(
        "INVARIANT VIOLATED: profiler fingerprint input length exceeds u64. This is a bug because one source path or file cannot be that large in the process address space. Fix: inspect corrupt source metadata.",
    );
    hasher.update(length.to_le_bytes());
    hasher.update(bytes);
}

pub(crate) fn stable_fingerprint_hex(bytes: [u8; 16]) -> String {
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect(
            "INVARIANT VIOLATED: writing a byte to an in-memory String failed. This is a bug because String formatting is infallible. Fix: inspect the formatter implementation before emitting profiler evidence.",
        );
    }
    encoded
}

pub(crate) fn dataset_fingerprint_sha256(project_evidence: &[serde_json::Value]) -> String {
    const IDENTITY_FIELDS: [&str; 7] = [
        "root",
        "runtime",
        "detected_ruby_version",
        "runtime_classpath_fingerprint_sha256",
        "project_files",
        "project_source_bytes",
        "project_source_fingerprint_sha256",
    ];

    let mut projects = project_evidence.iter().collect::<Vec<_>>();
    projects.sort_by(|left, right| {
        let left_root = left
            .get("root")
            .and_then(serde_json::Value::as_str)
            .expect(
                "INVARIANT VIOLATED: profiler project evidence has no string root. This is a bug because dataset identity requires one canonical project owner. Fix: construct project evidence with its canonical root before fingerprinting.",
            );
        let right_root = right
            .get("root")
            .and_then(serde_json::Value::as_str)
            .expect(
                "INVARIANT VIOLATED: profiler project evidence has no string root. This is a bug because dataset identity requires one canonical project owner. Fix: construct project evidence with its canonical root before fingerprinting.",
            );
        left_root.cmp(right_root)
    });

    let mut fingerprint = Sha256::new();
    hash_length_prefixed(&mut fingerprint, b"ruby-fast-lsp-profiler-dataset-v1");
    for project in projects {
        for field in IDENTITY_FIELDS {
            let value = project.get(field).unwrap_or_else(|| {
                panic!(
                    "INVARIANT VIOLATED: profiler project evidence is missing dataset identity field `{field}`. This is a bug because comparable runs must hash the same complete input identity. Fix: add the field before computing the dataset fingerprint."
                )
            });
            let encoded = serde_json::to_vec(value).expect(
                "INVARIANT VIOLATED: profiler dataset identity could not serialize to JSON. This is a bug because every evidence field is already JSON-compatible. Fix: keep dataset identity fields serializable.",
            );
            hash_length_prefixed(&mut fingerprint, field.as_bytes());
            hash_length_prefixed(&mut fingerprint, &encoded);
        }
    }
    format!("{:x}", fingerprint.finalize())
}

pub(crate) fn machine_evidence() -> serde_json::Value {
    serde_json::json!({
        "os_release": bounded_command_output("uname", &["-sr"]),
        "cpu_model": bounded_command_output("sysctl", &["-n", "machdep.cpu.brand_string"]),
        "physical_memory_bytes": bounded_command_output("sysctl", &["-n", "hw.memsize"]),
    })
}

pub(crate) fn build_evidence() -> serde_json::Value {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let source_revision = bounded_command_output_in(manifest_dir, "git", &["rev-parse", "HEAD"]);
    let source_worktree_status =
        bounded_command_output_in(manifest_dir, "git", &["status", "--short"]);
    let tracked_diff_sha256 = command_stdout_sha256_in(
        manifest_dir,
        "git",
        &["diff", "--binary", "HEAD", "--", "."],
    );
    let executable = std::env::current_exe().ok();
    let executable_sha256 = executable.as_deref().and_then(sha256_file);
    serde_json::json!({
        "source_revision": source_revision,
        "source_worktree_status": source_worktree_status,
        "tracked_diff_sha256": tracked_diff_sha256,
        "profiler_executable": executable,
        "profiler_executable_sha256": executable_sha256,
    })
}

fn bounded_command_output(program: &str, args: &[&str]) -> Option<String> {
    bounded_command_output_in(".", program, args)
}

fn bounded_command_output_in(
    current_dir: impl AsRef<std::path::Path>,
    program: &str,
    args: &[&str],
) -> Option<String> {
    const MAX_OUTPUT_BYTES: usize = 4096;
    let output = std::process::Command::new(program)
        .current_dir(current_dir)
        .args(args)
        .output()
        .ok()?;
    if !output.status.success() || output.stdout.len() > MAX_OUTPUT_BYTES {
        return None;
    }
    let value = std::str::from_utf8(&output.stdout).ok()?.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn command_stdout_sha256_in(
    current_dir: impl AsRef<std::path::Path>,
    program: &str,
    args: &[&str],
) -> Option<String> {
    const MAX_OUTPUT_BYTES: usize = 256 * 1024 * 1024;
    use std::io::Read;
    use std::process::Stdio;

    let mut child = std::process::Command::new(program)
        .current_dir(current_dir)
        .args(args)
        .stdout(Stdio::piped())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let mut hasher = Sha256::new();
    let mut total = 0usize;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = stdout.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        total = total.checked_add(read)?;
        if total > MAX_OUTPUT_BYTES {
            let _ = child.kill();
            let _ = child.wait();
            return None;
        }
        hasher.update(&buffer[..read]);
    }
    let status = child.wait().ok()?;
    status.success().then(|| format!("{:x}", hasher.finalize()))
}

fn sha256_file(path: &std::path::Path) -> Option<String> {
    use std::io::Read;

    let mut file = std::fs::File::open(path).ok()?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(format!("{:x}", hasher.finalize()))
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct ProcessResourceUsage {
    pub(crate) user_cpu_us: u64,
    pub(crate) system_cpu_us: u64,
    pub(crate) peak_rss_bytes: u64,
    pub(crate) input_blocks: u64,
    pub(crate) output_blocks: u64,
}

impl ProcessResourceUsage {
    #[cfg(unix)]
    pub(crate) fn capture() -> Option<Self> {
        let mut usage = std::mem::MaybeUninit::<libc::rusage>::zeroed();
        // SAFETY: `usage` points to writable storage for one `rusage`; libc
        // initializes it on a successful `getrusage` call.
        let result = unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) };
        if result != 0 {
            return None;
        }
        // SAFETY: successful `getrusage` initialized every field.
        let usage = unsafe { usage.assume_init() };
        let peak_rss = u64::try_from(usage.ru_maxrss).ok()?;
        #[cfg(any(target_os = "macos", target_os = "ios"))]
        let peak_rss_bytes = peak_rss;
        #[cfg(not(any(target_os = "macos", target_os = "ios")))]
        let peak_rss_bytes = peak_rss.checked_mul(1024)?;
        Some(Self {
            user_cpu_us: timeval_us(usage.ru_utime)?,
            system_cpu_us: timeval_us(usage.ru_stime)?,
            peak_rss_bytes,
            input_blocks: u64::try_from(usage.ru_inblock).ok()?,
            output_blocks: u64::try_from(usage.ru_oublock).ok()?,
        })
    }

    #[cfg(not(unix))]
    pub(crate) fn capture() -> Option<Self> {
        None
    }

    pub(crate) fn delta(start: Option<Self>, end: Option<Self>) -> serde_json::Value {
        let (Some(start), Some(end)) = (start, end) else {
            return serde_json::Value::Null;
        };
        serde_json::json!({
            "user_cpu_ms": (end.user_cpu_us.saturating_sub(start.user_cpu_us)) as f64 / 1000.0,
            "system_cpu_ms": (end.system_cpu_us.saturating_sub(start.system_cpu_us)) as f64 / 1000.0,
            "peak_rss_bytes": end.peak_rss_bytes,
            "input_blocks": end.input_blocks.saturating_sub(start.input_blocks),
            "output_blocks": end.output_blocks.saturating_sub(start.output_blocks),
        })
    }
}

#[cfg(unix)]
fn timeval_us(value: libc::timeval) -> Option<u64> {
    let seconds = u64::try_from(value.tv_sec).ok()?;
    let micros = u64::try_from(value.tv_usec).ok()?;
    seconds.checked_mul(1_000_000)?.checked_add(micros)
}
