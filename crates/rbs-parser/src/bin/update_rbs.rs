//! Tool to update RBS type definitions from the ruby/rbs GitHub repository.
//!
//! This binary fetches the latest RBS type definitions from GitHub and updates
//! the local rbs_types/ directory. It also updates the commit hash in Cargo.toml.
//!
//! # Usage
//!
//! ```bash
//! # Update to latest from configured branch (default: master)
//! cargo run --bin update-rbs --features update-tool
//!
//! # Update to specific branch
//! cargo run --bin update-rbs --features update-tool -- --branch v3.4
//!
//! # Update to specific commit
//! cargo run --bin update-rbs --features update-tool -- --commit abc123
//!
//! # Dry run (show what would be downloaded)
//! cargo run --bin update-rbs --features update-tool -- --dry-run
//! ```

use flate2::read::GzDecoder;
use serde::Deserialize;
use std::env;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::Archive;
use toml_edit::{value, DocumentMut};

const GITHUB_API_BASE: &str = "https://api.github.com";
const GITHUB_RAW_BASE: &str = "https://github.com";

#[derive(Debug, Deserialize)]
struct GitHubCommit {
    sha: String,
}

#[derive(Debug)]
struct Config {
    repository: String,
    branch: String,
    commit: Option<String>,
    dry_run: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = env::args().collect();
    let config = parse_args(&args)?;

    println!("🔍 RBS Type Definitions Updater");
    println!("================================");
    println!("Repository: {}", config.repository);
    println!("Branch: {}", config.branch);
    if let Some(ref commit) = config.commit {
        println!("Commit: {}", commit);
    }
    println!();

    // Find the crate root (where Cargo.toml is)
    let crate_root = find_crate_root()?;
    println!("📁 Crate root: {}", crate_root.display());

    // Get the target commit (either specified or latest from branch)
    let target_commit = if let Some(commit) = &config.commit {
        commit.clone()
    } else {
        println!(
            "🔄 Fetching latest commit from branch '{}'...",
            config.branch
        );
        get_latest_commit(&config.repository, &config.branch)?
    };
    println!("📌 Target commit: {}", target_commit);

    if config.dry_run {
        println!("\n🔍 DRY RUN - No files will be modified");
        println!(
            "Would download from: {}/{}/archive/{}.tar.gz",
            GITHUB_RAW_BASE, config.repository, target_commit
        );
        return Ok(());
    }

    // Download and extract the tarball
    println!("\n📥 Downloading RBS definitions...");
    let tarball_url = format!(
        "{}/{}/archive/{}.tar.gz",
        GITHUB_RAW_BASE, config.repository, target_commit
    );

    let tarball_data = download_tarball(&tarball_url)?;
    println!("   Downloaded {} bytes", tarball_data.len());

    // Extract core/ and stdlib/ directories
    println!("\n📦 Extracting RBS files...");
    let rbs_types_dir = crate_root.join("rbs_types");
    extract_rbs_files(&tarball_data, &rbs_types_dir, &target_commit)?;

    // Update Cargo.toml with new commit hash
    println!("\n📝 Updating Cargo.toml metadata...");
    update_cargo_toml(&crate_root, &target_commit, &config.branch)?;

    println!("\n✅ Successfully updated RBS type definitions!");
    println!("   Commit: {}", target_commit);
    println!("   Branch: {}", config.branch);
    println!("\n💡 Don't forget to rebuild the crate to embed the new definitions.");

    Ok(())
}

fn parse_args(args: &[String]) -> Result<Config, Box<dyn std::error::Error>> {
    let mut config = Config {
        repository: "ruby/rbs".to_string(),
        branch: "master".to_string(),
        commit: None,
        dry_run: false,
    };

    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--branch" | "-b" => {
                i += 1;
                if i < args.len() {
                    config.branch = args[i].clone();
                } else {
                    return Err("--branch requires a value".into());
                }
            }
            "--commit" | "-c" => {
                i += 1;
                if i < args.len() {
                    config.commit = Some(args[i].clone());
                } else {
                    return Err("--commit requires a value".into());
                }
            }
            "--repository" | "-r" => {
                i += 1;
                if i < args.len() {
                    config.repository = args[i].clone();
                } else {
                    return Err("--repository requires a value".into());
                }
            }
            "--dry-run" | "-n" => {
                config.dry_run = true;
            }
            "--help" | "-h" => {
                print_help();
                std::process::exit(0);
            }
            arg => {
                return Err(format!("Unknown argument: {}", arg).into());
            }
        }
        i += 1;
    }

    Ok(config)
}

fn print_help() {
    println!(
        r#"update-rbs - Update RBS type definitions from GitHub

USAGE:
    cargo run --bin update-rbs --features update-tool [OPTIONS]

OPTIONS:
    -b, --branch <BRANCH>      Branch to fetch from (default: master)
    -c, --commit <COMMIT>      Specific commit hash to fetch
    -r, --repository <REPO>    GitHub repository (default: ruby/rbs)
    -n, --dry-run              Show what would be done without making changes
    -h, --help                 Print this help message

EXAMPLES:
    # Update to latest from master
    cargo run --bin update-rbs --features update-tool

    # Update to specific branch
    cargo run --bin update-rbs --features update-tool -- --branch v3.4

    # Update to specific commit
    cargo run --bin update-rbs --features update-tool -- --commit abc123def
"#
    );
}

fn find_crate_root() -> Result<PathBuf, Box<dyn std::error::Error>> {
    // Try to find Cargo.toml by walking up from current dir
    let mut current = env::current_dir()?;

    // First check if we're already in the rbs-parser crate
    if current.join("Cargo.toml").exists() {
        let content = fs::read_to_string(current.join("Cargo.toml"))?;
        if content.contains("name = \"rbs-parser\"") {
            return Ok(current);
        }
    }

    // Try the crates/rbs-parser subdirectory
    let rbs_parser_dir = current.join("crates/rbs-parser");
    if rbs_parser_dir.exists() {
        return Ok(rbs_parser_dir);
    }

    // Walk up looking for it
    loop {
        let cargo_toml = current.join("Cargo.toml");
        if cargo_toml.exists() {
            let content = fs::read_to_string(&cargo_toml)?;
            if content.contains("name = \"rbs-parser\"") {
                return Ok(current);
            }
            // Check if this is a workspace root
            if content.contains("[workspace]") {
                let rbs_parser = current.join("crates/rbs-parser");
                if rbs_parser.exists() {
                    return Ok(rbs_parser);
                }
            }
        }

        if !current.pop() {
            break;
        }
    }

    Err("Could not find rbs-parser crate root. Run from within the ruby-fast-lsp project.".into())
}

fn get_latest_commit(repository: &str, branch: &str) -> Result<String, Box<dyn std::error::Error>> {
    let url = format!(
        "{}/repos/{}/commits/{}",
        GITHUB_API_BASE, repository, branch
    );

    let client = reqwest::blocking::Client::new();
    let response = client
        .get(&url)
        .header("User-Agent", "rbs-parser-updater")
        .header("Accept", "application/vnd.github.v3+json")
        .send()?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to fetch commit info: {} - {}",
            response.status(),
            response.text().unwrap_or_default()
        )
        .into());
    }

    let commit: GitHubCommit = response.json()?;
    Ok(commit.sha)
}

fn download_tarball(url: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let client = reqwest::blocking::Client::new();
    let response = client
        .get(url)
        .header("User-Agent", "rbs-parser-updater")
        .send()?;

    if !response.status().is_success() {
        return Err(format!(
            "Failed to download tarball: {} - {}",
            response.status(),
            response.text().unwrap_or_default()
        )
        .into());
    }

    let bytes = response.bytes()?;
    Ok(bytes.to_vec())
}

fn extract_rbs_files(
    tarball_data: &[u8],
    dest_dir: &Path,
    _commit: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    // Clean existing directories
    let core_dir = dest_dir.join("core");
    let stdlib_dir = dest_dir.join("stdlib");

    if core_dir.exists() {
        println!("   Removing existing core/ directory...");
        fs::remove_dir_all(&core_dir)?;
    }
    if stdlib_dir.exists() {
        println!("   Removing existing stdlib/ directory...");
        fs::remove_dir_all(&stdlib_dir)?;
    }

    // Create destination directories
    fs::create_dir_all(&core_dir)?;
    fs::create_dir_all(&stdlib_dir)?;

    // Decompress and extract
    let decoder = GzDecoder::new(tarball_data);
    let mut archive = Archive::new(decoder);

    let mut core_count = 0;
    let mut stdlib_count = 0;

    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.to_path_buf();
        let path_str = path.to_string_lossy();

        // Check if this is a core/ file
        let is_core = path_str.contains("/core/") && path_str.ends_with(".rbs");
        let is_stdlib = path_str.contains("/stdlib/") && path_str.ends_with(".rbs");

        if is_core {
            // Extract relative path after "core/"
            if let Some(pos) = path_str.find("/core/") {
                let relative = &path_str[pos + 6..]; // Skip "/core/"
                let dest_path = core_dir.join(relative);

                // Create parent directories
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                // Extract file
                let mut content = Vec::new();
                entry.read_to_end(&mut content)?;
                fs::write(&dest_path, &content)?;
                core_count += 1;
            }
        } else if is_stdlib {
            // Extract relative path after "stdlib/"
            if let Some(pos) = path_str.find("/stdlib/") {
                let relative = &path_str[pos + 8..]; // Skip "/stdlib/"
                let dest_path = stdlib_dir.join(relative);

                // Create parent directories
                if let Some(parent) = dest_path.parent() {
                    fs::create_dir_all(parent)?;
                }

                // Extract file
                let mut content = Vec::new();
                entry.read_to_end(&mut content)?;
                fs::write(&dest_path, &content)?;
                stdlib_count += 1;
            }
        }
    }

    println!("   Extracted {} core RBS files", core_count);
    println!("   Extracted {} stdlib RBS files", stdlib_count);

    if core_count == 0 {
        return Err("No core RBS files found in tarball. Check the repository structure.".into());
    }

    Ok(())
}

fn update_cargo_toml(
    crate_root: &Path,
    commit: &str,
    branch: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let cargo_toml_path = crate_root.join("Cargo.toml");
    let content = fs::read_to_string(&cargo_toml_path)?;

    let mut doc = content.parse::<DocumentMut>()?;

    // Update the metadata section
    if let Some(package) = doc.get_mut("package") {
        if let Some(metadata) = package.get_mut("metadata") {
            if let Some(rbs) = metadata.get_mut("rbs") {
                rbs["commit"] = value(commit);
                rbs["branch"] = value(branch);
                rbs["last_updated"] = value(chrono_date());
            }
        }
    }

    fs::write(&cargo_toml_path, doc.to_string())?;
    println!(
        "   Updated Cargo.toml with commit: {}",
        &commit[..12.min(commit.len())]
    );

    Ok(())
}

fn chrono_date() -> String {
    // Simple date without chrono dependency
    use std::time::{SystemTime, UNIX_EPOCH};
    let duration = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let secs = duration.as_secs();

    // Approximate date calculation (not perfect but good enough)
    let days = secs / 86400;
    let years = days / 365;
    let year = 1970 + years;
    let remaining_days = days % 365;
    let month = remaining_days / 30 + 1;
    let day = remaining_days % 30 + 1;

    format!("{:04}-{:02}-{:02}", year, month.min(12), day.min(31))
}
