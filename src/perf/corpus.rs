//! Corpus harness for perf benchmarks and regression tests.
//!
//! Resolves a named corpus to a directory of Ruby source. Three sources, in
//! priority order:
//!
//! 1. `RUBY_FAST_LSP_CORPUS_DIR=/path/to/dir` env override — skip
//!    extraction entirely, point at an existing checkout. Useful for
//!    iterating locally.
//! 2. Cached extraction at `target/perf-corpus/<name>/` — reused across
//!    runs. An `.corpus-ready` marker signals a completed extraction; if
//!    missing the directory is wiped and re-extracted.
//! 3. Tarball at `tests/perf/corpus/<name>.tar.zst`, extracted on demand.
//!
//! Synthetic corpora (generated at runtime, no tarball) are handled by
//! [`ensure_synthetic`], which delegates to the generator and caches the
//! result under the same `target/perf-corpus/synthetic-<scale>/` root.
//!
//! Race note: extraction writes to a scratch dir and renames atomically,
//! so two concurrent callers converge on the same cache without tearing.

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

const READY_MARKER: &str = ".corpus-ready";

/// Ensure the named corpus exists on disk and return its root directory.
///
/// `name` maps to a tarball at `tests/perf/corpus/<name>.tar.zst` (relative
/// to the crate root). Pass e.g. `"discourse"` or `"mastodon"`.
pub fn ensure_corpus(name: &str) -> Result<PathBuf> {
    assert!(
        !name.is_empty() && !name.contains('/') && !name.contains(".."),
        "INVARIANT VIOLATED: corpus name {:?} is invalid. \
         This is a bug because corpus names are used as path segments. \
         Fix: pass a bare identifier like \"discourse\".",
        name
    );

    if let Ok(override_dir) = std::env::var("RUBY_FAST_LSP_CORPUS_DIR") {
        let p = PathBuf::from(override_dir).join(name);
        if p.is_dir() {
            return Ok(p);
        }
    }

    let cache_dir = cache_root()?.join(name);
    if is_ready(&cache_dir) {
        return Ok(cache_dir);
    }

    let tarball = tarball_path(name);
    if tarball.is_file() {
        extract_tarball(&tarball, &cache_dir)
            .with_context(|| format!("extracting corpus {} from {}", name, tarball.display()))?;
        return Ok(cache_dir);
    }

    Err(anyhow!(
        "corpus {:?} not available. \
         Run `scripts/snapshot_corpus.sh {}` to fetch it into {}, \
         ship a tarball at {}, or set RUBY_FAST_LSP_CORPUS_DIR to an existing checkout.",
        name,
        name,
        cache_dir.display(),
        tarball.display()
    ))
}

/// Ensure a synthetically-generated corpus exists and return its root.
///
/// `scale` is the approximate target file count (e.g. 1_000, 10_000).
/// Generation is deterministic given the scale; the cache key encodes it.
pub fn ensure_synthetic(scale: usize, generate: impl FnOnce(&Path) -> Result<()>) -> Result<PathBuf> {
    assert!(
        scale > 0,
        "INVARIANT VIOLATED: synthetic corpus scale is zero. \
         This is a bug because a zero-file corpus is not useful. \
         Fix: pass a positive scale."
    );

    let name = format!("synthetic-{}", scale);
    let cache_dir = cache_root()?.join(&name);
    if is_ready(&cache_dir) {
        return Ok(cache_dir);
    }

    let scratch = scratch_path(&cache_dir);
    fs::create_dir_all(&scratch)
        .with_context(|| format!("creating scratch dir {}", scratch.display()))?;

    generate(&scratch).context("generating synthetic corpus")?;
    finalize(&scratch, &cache_dir)?;
    Ok(cache_dir)
}

fn cache_root() -> Result<PathBuf> {
    let workspace = workspace_root()?;
    let dir = workspace.join("target").join("perf-corpus");
    fs::create_dir_all(&dir)
        .with_context(|| format!("creating cache root {}", dir.display()))?;
    Ok(dir)
}

fn tarball_path(name: &str) -> PathBuf {
    workspace_root()
        .expect("INVARIANT VIOLATED: CARGO_MANIFEST_DIR missing")
        .join("tests")
        .join("perf")
        .join("corpus")
        .join(format!("{}.tar.zst", name))
}

fn workspace_root() -> Result<PathBuf> {
    // Baked in at compile time by cargo; survives after the binary is
    // detached from the cargo harness (e.g. `./target/release/bench_references`).
    Ok(PathBuf::from(env!("CARGO_MANIFEST_DIR")))
}

fn is_ready(dir: &Path) -> bool {
    dir.join(READY_MARKER).is_file()
}

fn scratch_path(cache_dir: &Path) -> PathBuf {
    let mut s = cache_dir.as_os_str().to_owned();
    s.push(".extracting");
    PathBuf::from(s)
}

fn extract_tarball(tarball: &Path, cache_dir: &Path) -> Result<()> {
    let scratch = scratch_path(cache_dir);
    if scratch.exists() {
        fs::remove_dir_all(&scratch)
            .with_context(|| format!("removing stale scratch {}", scratch.display()))?;
    }
    fs::create_dir_all(&scratch)?;

    let f = fs::File::open(tarball)
        .with_context(|| format!("opening tarball {}", tarball.display()))?;
    let zstd_reader = zstd::Decoder::new(f).context("initializing zstd decoder")?;
    let mut archive = tar::Archive::new(zstd_reader);
    archive
        .unpack(&scratch)
        .with_context(|| format!("unpacking into {}", scratch.display()))?;

    finalize(&scratch, cache_dir)
}

fn finalize(scratch: &Path, cache_dir: &Path) -> Result<()> {
    fs::File::create(scratch.join(READY_MARKER))
        .with_context(|| format!("writing {} in {}", READY_MARKER, scratch.display()))?;

    if cache_dir.exists() {
        fs::remove_dir_all(cache_dir)
            .with_context(|| format!("removing stale cache {}", cache_dir.display()))?;
    }
    fs::rename(scratch, cache_dir).with_context(|| {
        format!(
            "promoting scratch {} to cache {}",
            scratch.display(),
            cache_dir.display()
        )
    })?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_corpus_returns_actionable_error() {
        let err = ensure_corpus("does-not-exist-xyz").unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("snapshot_corpus.sh"),
            "unexpected error: {}",
            msg
        );
    }

    #[test]
    fn synthetic_generates_and_caches() {
        let name = "synthetic-test-fixture";
        let cache = cache_root().unwrap().join(name);
        if cache.exists() {
            fs::remove_dir_all(&cache).unwrap();
        }

        let marker_calls = std::sync::atomic::AtomicUsize::new(0);
        let gen = |dir: &Path| -> Result<()> {
            marker_calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            fs::write(dir.join("a.rb"), "class A; end\n")?;
            Ok(())
        };

        // Use a one-off ensure that hardcodes the name so we don't collide
        // with real synthetic-<scale> caches.
        let cache_dir = cache_root().unwrap().join(name);
        let scratch = scratch_path(&cache_dir);
        fs::create_dir_all(&scratch).unwrap();
        gen(&scratch).unwrap();
        finalize(&scratch, &cache_dir).unwrap();

        assert!(cache_dir.join("a.rb").is_file());
        assert!(is_ready(&cache_dir));

        fs::remove_dir_all(&cache_dir).unwrap();
    }

    #[test]
    fn invalid_corpus_names_panic() {
        let bad = ["", "../evil", "foo/bar"];
        for name in bad {
            let r = std::panic::catch_unwind(|| ensure_corpus(name));
            assert!(r.is_err(), "expected panic for name {:?}", name);
        }
    }
}
