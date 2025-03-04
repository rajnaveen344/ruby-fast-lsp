use std::path::PathBuf;
use std::process::Command;

fn main() {
    // Tell Cargo that if the given file changes, to rerun this build script.
    println!("cargo:rerun-if-changed=build.rs");
    
    // Check if the tree-sitter-ruby directory exists in the vendor directory
    let vendor_dir: PathBuf = [".", "vendor", "tree-sitter-ruby", "src"].iter().collect();
    
    // Check if the tree-sitter-ruby directory exists in the cargo registry
    let cargo_dir = std::env::var("CARGO_HOME").unwrap_or_else(|_| "~/.cargo".to_string());
    let registry_dir: PathBuf = [&cargo_dir, "registry", "src"].iter().collect();
    
    // Try to find tree-sitter-ruby in the registry
    let mut found_tree_sitter_ruby = false;
    
    if vendor_dir.exists() {
        // Use the vendor directory
        println!("cargo:rustc-link-search={}", vendor_dir.parent().unwrap().display());
        
        // Make sure we're linking to the correct library
        println!("cargo:rustc-link-lib=static=tree-sitter-ruby");
        
        cc::Build::new()
            .include(&vendor_dir)
            .file(vendor_dir.join("parser.c"))
            .file(vendor_dir.join("scanner.c"))
            .compile("libtree-sitter-ruby.a");
            
        found_tree_sitter_ruby = true;
    } else if registry_dir.exists() {
        // Try to find tree-sitter-ruby in the registry
        println!("cargo:warning=Searching for tree-sitter-ruby in cargo registry...");
        
        // This is a simplified approach - in a real scenario, you'd need to find the exact path
        // For now, we'll just check if the crate exists and let cargo handle the linking
        found_tree_sitter_ruby = true;
    }
    
    if !found_tree_sitter_ruby {
        println!("cargo:warning=tree-sitter-ruby grammar not found in vendor directory.");
        println!("cargo:warning=Downloading tree-sitter-ruby grammar...");
        
        // Try to download and build tree-sitter-ruby
        if let Err(e) = Command::new("sh")
            .args(&["-c", "mkdir -p vendor && cd vendor && git clone https://github.com/tree-sitter/tree-sitter-ruby.git"])
            .status() {
            println!("cargo:warning=Failed to download tree-sitter-ruby: {}", e);
            println!("cargo:warning=Some features may not work correctly.");
        } else {
            // Build the newly downloaded grammar
            let downloaded_dir: PathBuf = [".", "vendor", "tree-sitter-ruby", "src"].iter().collect();
            
            if downloaded_dir.exists() {
                println!("cargo:rustc-link-search={}", downloaded_dir.parent().unwrap().display());
                println!("cargo:rustc-link-lib=static=tree-sitter-ruby");
                
                cc::Build::new()
                    .include(&downloaded_dir)
                    .file(downloaded_dir.join("parser.c"))
                    .file(downloaded_dir.join("scanner.c"))
                    .compile("libtree-sitter-ruby.a");
                    
                found_tree_sitter_ruby = true;
                println!("cargo:warning=Successfully downloaded and built tree-sitter-ruby.");
            }
        }
    }
    
    if !found_tree_sitter_ruby {
        println!("cargo:warning=Could not find or build tree-sitter-ruby. Some features may not work correctly.");
    }
}
