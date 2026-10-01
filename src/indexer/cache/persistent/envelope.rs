//! Versioned, compressed, checksummed envelope around persistent product payloads.

use anyhow::{anyhow, Context, Result};
use sha2::{Digest, Sha256};
use std::io::Read;

use super::{ENVELOPE_HEADER_BYTES, ENVELOPE_SCHEMA, MAX_COMPRESSED_ENTRY_BYTES};

pub(super) fn encode_envelope(
    magic: &[u8; 8],
    max_logical_entry_bytes: u64,
    payload: &[u8],
) -> Result<Vec<u8>> {
    let logical_len = u64::try_from(payload.len())
        .map_err(|_| anyhow!("persistent product payload length exceeded u64"))?;
    if logical_len > max_logical_entry_bytes {
        return Err(anyhow!(
            "persistent product payload is {logical_len} bytes; maximum is {max_logical_entry_bytes}"
        ));
    }
    let compressed = zstd::stream::encode_all(payload, 3)
        .context("compressing persistent gem dependency product")?;
    let compressed_len = u64::try_from(compressed.len())
        .map_err(|_| anyhow!("compressed persistent product length exceeded u64"))?;
    let mut encoded = Vec::with_capacity(
        ENVELOPE_HEADER_BYTES
            .checked_add(compressed.len())
            .ok_or_else(|| anyhow!("persistent envelope size overflowed usize"))?,
    );
    encoded.extend_from_slice(magic);
    encoded.extend_from_slice(&ENVELOPE_SCHEMA.to_le_bytes());
    encoded.extend_from_slice(&logical_len.to_le_bytes());
    encoded.extend_from_slice(&compressed_len.to_le_bytes());
    encoded.extend_from_slice(&Sha256::digest(payload));
    encoded.extend_from_slice(&compressed);
    Ok(encoded)
}

pub(super) fn decode_envelope(
    magic: &[u8; 8],
    max_logical_entry_bytes: u64,
    encoded: &[u8],
) -> Result<Vec<u8>> {
    if encoded.len() < ENVELOPE_HEADER_BYTES {
        return Err(anyhow!("persistent product is shorter than its envelope"));
    }
    if &encoded[..8] != magic {
        return Err(anyhow!("persistent product magic does not match"));
    }
    let schema = u32::from_le_bytes(
        encoded[8..12]
            .try_into()
            .map_err(|_| anyhow!("persistent product schema bytes are not exactly four bytes"))?,
    );
    if schema != ENVELOPE_SCHEMA {
        return Err(anyhow!(
            "persistent product envelope schema {schema} does not match {ENVELOPE_SCHEMA}"
        ));
    }
    let logical_len = encoded_payload_logical_len(encoded)?;
    let compressed_len = u64::from_le_bytes(
        encoded[20..28]
            .try_into()
            .map_err(|_| anyhow!("persistent product length field is malformed"))?,
    );
    if logical_len > max_logical_entry_bytes {
        return Err(anyhow!(
            "persistent product declares {logical_len} logical bytes; maximum is {max_logical_entry_bytes}"
        ));
    }
    if compressed_len > MAX_COMPRESSED_ENTRY_BYTES {
        return Err(anyhow!(
            "persistent product declares {compressed_len} compressed bytes; maximum is {MAX_COMPRESSED_ENTRY_BYTES}"
        ));
    }
    let compressed_len_usize = usize::try_from(compressed_len)
        .map_err(|_| anyhow!("compressed persistent product length does not fit usize"))?;
    let expected_total = ENVELOPE_HEADER_BYTES
        .checked_add(compressed_len_usize)
        .ok_or_else(|| anyhow!("persistent product total length overflowed usize"))?;
    if encoded.len() != expected_total {
        return Err(anyhow!(
            "persistent product length {} does not match declared {}",
            encoded.len(),
            expected_total
        ));
    }
    let decoder = zstd::stream::read::Decoder::new(&encoded[ENVELOPE_HEADER_BYTES..])
        .context("initializing persistent product decompressor")?;
    let limit = logical_len
        .checked_add(1)
        .ok_or_else(|| anyhow!("persistent product logical read limit overflowed"))?;
    let mut payload = Vec::with_capacity(
        usize::try_from(logical_len)
            .map_err(|_| anyhow!("persistent product logical length does not fit usize"))?,
    );
    decoder
        .take(limit)
        .read_to_end(&mut payload)
        .context("decompressing persistent gem dependency product")?;
    if u64::try_from(payload.len()).ok() != Some(logical_len) {
        return Err(anyhow!(
            "persistent product decompressed to {} bytes; expected {logical_len}",
            payload.len()
        ));
    }
    let expected_checksum = &encoded[28..60];
    let actual_checksum = Sha256::digest(&payload);
    if actual_checksum.as_slice() != expected_checksum {
        return Err(anyhow!(
            "persistent product payload checksum does not match"
        ));
    }
    Ok(payload)
}

pub(super) fn encoded_payload_logical_len(encoded: &[u8]) -> Result<u64> {
    if encoded.len() < 20 {
        return Err(anyhow!(
            "persistent product is too short to contain logical length"
        ));
    }
    Ok(u64::from_le_bytes(encoded[12..20].try_into().map_err(
        |_| anyhow!("persistent product logical length field is malformed"),
    )?))
}
