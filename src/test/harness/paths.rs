use std::path::{Path, PathBuf};
use tower_lsp::lsp_types::Url;

/// Give an in-memory fixture an absolute native path, without touching the disk.
pub fn fixture_path(path: impl AsRef<Path>) -> PathBuf {
    let path = path.as_ref();
    if path.is_absolute() {
        return path.to_path_buf();
    }
    #[cfg(windows)]
    let root = Path::new("C:/");
    #[cfg(not(windows))]
    let root = Path::new("/");
    root.join(path.strip_prefix("/").unwrap_or(path))
}

pub fn fixture_uri(path: impl AsRef<Path>) -> Url {
    let is_directory = path.as_ref().to_string_lossy().ends_with('/');
    let path = fixture_path(path);
    let uri = if is_directory {
        Url::from_directory_path(path)
    } else {
        Url::from_file_path(path)
    };
    uri.expect("fixture path must form a native file URI")
}

/// Preserve the fixture's portable name when an assertion compares URI paths.
pub fn fixture_uri_path(uri: &Url) -> &str {
    #[cfg(windows)]
    if let Some(path) = uri.path().strip_prefix("/C:") {
        return path;
    }
    uri.path()
}

#[test]
fn fixture_paths_round_trip_through_native_file_uris() {
    for name in [
        "inline.rb",
        "/project/nested/source.rb",
        "project/space and 🦀.rb",
    ] {
        let path = fixture_path(name);
        assert!(path.is_absolute());
        let uri = fixture_uri(name);
        assert_eq!(uri.to_file_path().unwrap(), path);
    }
    assert_eq!(
        fixture_uri_path(&fixture_uri("project/source.rb")),
        "/project/source.rb"
    );
    assert_eq!(fixture_uri_path(&fixture_uri("project/")), "/project/");
}
