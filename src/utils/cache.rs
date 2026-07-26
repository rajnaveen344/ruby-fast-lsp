use anyhow::{anyhow, Result};
use std::path::PathBuf;

pub fn ruby_fast_lsp_user_cache_root() -> Result<PathBuf> {
    if let Some(root) = std::env::var_os("RUBY_FAST_LSP_CACHE_DIR") {
        let root = PathBuf::from(root);
        if !root.is_absolute() {
            return Err(anyhow!(
                "RUBY_FAST_LSP_CACHE_DIR must be an absolute path, got {}",
                root.display()
            ));
        }
        return Ok(root);
    }

    if let Some(root) = std::env::var_os("XDG_CACHE_HOME") {
        let root = PathBuf::from(root);
        if !root.is_absolute() {
            return Err(anyhow!(
                "XDG_CACHE_HOME must be an absolute path, got {}",
                root.display()
            ));
        }
        return Ok(root.join("ruby-fast-lsp"));
    }

    #[cfg(target_os = "windows")]
    if let Some(root) = std::env::var_os("LOCALAPPDATA") {
        let root = PathBuf::from(root);
        if !root.is_absolute() {
            return Err(anyhow!(
                "LOCALAPPDATA must be an absolute path, got {}",
                root.display()
            ));
        }
        return Ok(root.join("ruby-fast-lsp").join("Cache"));
    }

    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("cannot locate the user cache because HOME is not set"))?;
    if !home.is_absolute() {
        return Err(anyhow!(
            "HOME must be an absolute path, got {}",
            home.display()
        ));
    }

    #[cfg(target_os = "macos")]
    return Ok(home.join("Library").join("Caches").join("ruby-fast-lsp"));

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    return Ok(home.join(".cache").join("ruby-fast-lsp"));

    #[cfg(target_os = "windows")]
    Err(anyhow!(
        "cannot locate the user cache because LOCALAPPDATA is not set"
    ))
}
