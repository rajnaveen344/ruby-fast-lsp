use anyhow::{Context, Result};
use flate2::read::GzDecoder;
use flate2::write::GzEncoder;
use flate2::Compression;
use std::io::{Read, Write};

/// Compress stub data using gzip compression
///
/// This is used during the build process to reduce the size of stub files
/// that will be packaged with the extension.
pub fn compress_stub_data(data: &str) -> Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(data.as_bytes())
        .context("Failed to write data to gzip encoder")?;

    encoder
        .finish()
        .context("Failed to finish gzip compression")
}

/// Decompress stub data that was compressed with gzip
///
/// This is used at runtime to decompress stub files before parsing them as JSON.
pub fn decompress_stub_data(compressed_data: &[u8]) -> Result<String> {
    let mut decoder = GzDecoder::new(compressed_data);
    let mut decompressed = String::new();

    decoder
        .read_to_string(&mut decompressed)
        .context("Failed to decompress gzip data")?;

    Ok(decompressed)
}

/// Check if data appears to be gzip compressed
///
/// This checks for the gzip magic number (0x1f, 0x8b) at the beginning of the data.
pub fn is_gzip_compressed(data: &[u8]) -> bool {
    data.len() >= 2 && data[0] == 0x1f && data[1] == 0x8b
}

/// Estimate compression ratio for stub data
///
/// This can be used during build to decide whether compression is worthwhile
/// for a particular stub file.
pub fn estimate_compression_ratio(data: &str) -> Result<f64> {
    let original_size = data.len();
    if original_size == 0 {
        return Ok(1.0);
    }

    let compressed = compress_stub_data(data)?;
    let compressed_size = compressed.len();

    Ok(compressed_size as f64 / original_size as f64)
}

/// Compress stub data only if it results in significant size reduction
///
/// Returns the compressed data if compression reduces size by at least the threshold,
/// otherwise returns the original data as bytes.
pub fn compress_if_beneficial(data: &str, threshold: f64) -> Result<(Vec<u8>, bool)> {
    let ratio = estimate_compression_ratio(data)?;

    if ratio < threshold {
        let compressed = compress_stub_data(data)?;
        Ok((compressed, true))
    } else {
        Ok((data.as_bytes().to_vec(), false))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_decompress_roundtrip() {
        let original = r#"{
            "name": "Object",
            "version": "2.7",
            "methods": [
                {
                    "name": "initialize",
                    "visibility": "private",
                    "parameters": []
                }
            ],
            "constants": []
        }"#;

        let compressed = compress_stub_data(original).unwrap();
        let decompressed = decompress_stub_data(&compressed).unwrap();

        assert_eq!(original, decompressed);
    }

    #[test]
    fn test_gzip_detection() {
        let original = "Hello, world!";
        let compressed = compress_stub_data(original).unwrap();

        assert!(is_gzip_compressed(&compressed));
        assert!(!is_gzip_compressed(original.as_bytes()));
    }

    #[test]
    fn test_compression_ratio() {
        // Highly repetitive data should compress well
        let repetitive_data = "a".repeat(1000);
        let ratio = estimate_compression_ratio(&repetitive_data).unwrap();
        assert!(ratio < 0.1); // Should compress to less than 10% of original

        // Random-like data should not compress well
        let random_data = "abcdefghijklmnopqrstuvwxyz0123456789";
        let ratio = estimate_compression_ratio(random_data).unwrap();
        assert!(ratio > 0.8); // Should not compress much
    }

    #[test]
    fn test_compress_if_beneficial() {
        // Test with data that compresses well
        let repetitive_data = "a".repeat(1000);
        let (result, was_compressed) = compress_if_beneficial(&repetitive_data, 0.5).unwrap();
        assert!(was_compressed);
        assert!(is_gzip_compressed(&result));

        // Test with data that doesn't compress well
        let small_data = "abc";
        let (result, was_compressed) = compress_if_beneficial(small_data, 0.5).unwrap();
        assert!(!was_compressed);
        assert_eq!(result, small_data.as_bytes());
    }

    #[test]
    fn test_empty_data() {
        let empty = "";
        let compressed = compress_stub_data(empty).unwrap();
        let decompressed = decompress_stub_data(&compressed).unwrap();
        assert_eq!(empty, decompressed);

        let ratio = estimate_compression_ratio(empty).unwrap();
        assert_eq!(ratio, 1.0);
    }
}