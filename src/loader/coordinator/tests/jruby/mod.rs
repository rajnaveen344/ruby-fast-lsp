//! JRuby runtime, Java catalog, and decompiled navigation coordination.

use super::*;
use crate::environment::runtime::jruby::classpath;
use crate::environment::runtime::jruby::java_catalog;
use crate::utils::admission;

mod decompiled_navigation;
mod import_facts;
mod java_artifacts;
mod runtime_companion;

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

fn write_jar(path: &Path, entry: &str, contents: &[u8]) {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    writer
        .start_file(entry, SimpleFileOptions::default())
        .expect("fixture JAR entry must start");
    writer
        .write_all(contents)
        .expect("fixture JAR entry must write");
    let bytes = writer
        .finish()
        .expect("fixture JAR must finish")
        .into_inner();
    fs::write(path, bytes).expect("fixture JAR must be written");
}
