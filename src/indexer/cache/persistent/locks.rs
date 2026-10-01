//! Private on-disk lock files and bounded lock acquisition for cache ownership.

use anyhow::{anyhow, Context, Result};
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::atomic::Ordering;
use std::time::Instant;

use super::{PersistentProductCounters, LOCK_RETRY_INTERVAL, LOCK_WAIT_TIMEOUT};

pub(super) fn open_private_lock_file(path: &Path) -> Result<File> {
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("cache lock path has no parent: {}", path.display()))?;
    std::fs::create_dir_all(parent).with_context(|| {
        format!(
            "creating persistent cache lock directory {}",
            parent.display()
        )
    })?;
    let mut options = OpenOptions::new();
    options.read(true).write(true).create(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options
        .open(path)
        .with_context(|| format!("opening persistent cache lock {}", path.display()))
}

pub(super) fn try_acquire_lock(file: &File, exclusive: bool) -> Result<bool> {
    let result = if exclusive {
        FileExt::try_lock_exclusive(file)
    } else {
        FileExt::try_lock_shared(file)
    };
    match result {
        Ok(()) => Ok(true),
        Err(error) if error.raw_os_error() == fs2::lock_contended_error().raw_os_error() => {
            Ok(false)
        }
        Err(error) => Err(error).context("acquiring persistent cache ownership lock"),
    }
}

pub(super) fn acquire_lock(
    file: &File,
    exclusive: bool,
    counters: &PersistentProductCounters,
) -> Result<()> {
    let started = Instant::now();
    let mut waited = false;
    loop {
        match try_acquire_lock(file, exclusive)? {
            true => {
                if waited {
                    counters.lock_waits.fetch_add(1, Ordering::Relaxed);
                }
                return Ok(());
            }
            false => {
                waited = true;
                if started.elapsed() >= LOCK_WAIT_TIMEOUT {
                    return Err(anyhow!(
                        "timed out after {:?} waiting for persistent cache ownership lock",
                        LOCK_WAIT_TIMEOUT
                    ));
                }
                std::thread::sleep(LOCK_RETRY_INTERVAL);
            }
        }
    }
}
