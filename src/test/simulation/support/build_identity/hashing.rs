use sha2::{Digest, Sha256};
use std::io::{self, Read};

pub(crate) fn reader_sha256(mut reader: impl Read) -> io::Result<String> {
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let length = match reader.read(&mut buffer) {
            Ok(length) => length,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        if length == 0 {
            return Ok(format!("{:x}", hasher.finalize()));
        }
        hasher.update(&buffer[..length]);
    }
}

pub(crate) fn fields_sha256<'a>(fields: impl IntoIterator<Item = &'a [u8]>) -> String {
    let mut hasher = Sha256::new();
    for field in fields {
        hasher.update(
            u64::try_from(field.len())
                .expect(
                    "INVARIANT VIOLATED: a simulation identity field exceeded u64. This is a bug because one build cannot hold such an input. Fix: reject oversized identity inputs before hashing.",
                )
                .to_le_bytes(),
        );
        hasher.update(field);
    }
    format!("{:x}", hasher.finalize())
}

pub(crate) fn manifest_sha256<'a>(records: impl IntoIterator<Item = (&'a str, &'a str)>) -> String {
    let mut records = records.into_iter().collect::<Vec<_>>();
    records.sort_unstable_by_key(|(path, _)| *path);
    assert!(
        records.windows(2).all(|pair| pair[0].0 != pair[1].0),
        "INVARIANT VIOLATED: a simulation source manifest contains duplicate paths. This is a bug because each path must name exactly one source identity. Fix: deduplicate the collected workspace paths before hashing."
    );
    fields_sha256(
        [b"ruby-fast-lsp-simulation-source-manifest-v1".as_slice()]
            .into_iter()
            .chain(
                records
                    .iter()
                    .flat_map(|(path, sha256)| [path.as_bytes(), sha256.as_bytes()]),
            ),
    )
}
