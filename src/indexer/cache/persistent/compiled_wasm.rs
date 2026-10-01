//! Compiled Wasm product identity and its persistent payload codec.

use crate::invariant::ExpectInvariant;
use anyhow::{anyhow, Result};
use sha2::{Digest, Sha256};

use super::{COMPILED_WASM_PAYLOAD_HEADER_BYTES, COMPILED_WASM_PAYLOAD_SCHEMA};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompiledWasmProductKey {
    cache_id: String,
    source_length: u64,
    source_sha256: [u8; 32],
    compiler_identity: u64,
}

impl CompiledWasmProductKey {
    pub fn new(wasm_bytes: &[u8], compiler_identity: u64) -> Self {
        let source_length = u64::try_from(wasm_bytes.len()).expect_invariant(
            "Wasm extension length does not fit u64",
            "a source artifact cannot exceed the process address space",
            "reject corrupt extension metadata before constructing a persistent product key",
        );
        let source_sha256: [u8; 32] = Sha256::digest(wasm_bytes).into();
        let mut digest = Sha256::new();
        digest.update(b"ruby-fast-lsp-compiled-wasm-product-v1\0");
        digest.update(source_length.to_le_bytes());
        digest.update(source_sha256);
        digest.update(compiler_identity.to_le_bytes());
        let cache_id = format!("{:x}", digest.finalize());
        Self {
            cache_id,
            source_length,
            source_sha256,
            compiler_identity,
        }
    }

    pub fn cache_id(&self) -> &str {
        &self.cache_id
    }
}

pub(super) fn encode_compiled_wasm_payload(
    key: &CompiledWasmProductKey,
    artifact: &[u8],
) -> Result<Vec<u8>> {
    let artifact_length = u64::try_from(artifact.len())
        .map_err(|_| anyhow!("compiled Wasm artifact length does not fit u64"))?;
    let capacity = COMPILED_WASM_PAYLOAD_HEADER_BYTES
        .checked_add(artifact.len())
        .ok_or_else(|| anyhow!("compiled Wasm payload length overflowed usize"))?;
    let mut payload = Vec::with_capacity(capacity);
    payload.extend_from_slice(&COMPILED_WASM_PAYLOAD_SCHEMA.to_le_bytes());
    payload.extend_from_slice(&key.source_length.to_le_bytes());
    payload.extend_from_slice(&key.source_sha256);
    payload.extend_from_slice(&key.compiler_identity.to_le_bytes());
    payload.extend_from_slice(&artifact_length.to_le_bytes());
    payload.extend_from_slice(&Sha256::digest(artifact));
    payload.extend_from_slice(artifact);
    Ok(payload)
}

pub(super) fn decode_compiled_wasm_payload(
    key: &CompiledWasmProductKey,
    mut payload: Vec<u8>,
) -> Result<Vec<u8>> {
    if payload.len() < COMPILED_WASM_PAYLOAD_HEADER_BYTES {
        return Err(anyhow!(
            "compiled Wasm payload is shorter than its identity header"
        ));
    }
    let schema = u32::from_le_bytes(
        payload[0..4]
            .try_into()
            .map_err(|_| anyhow!("compiled Wasm payload schema is malformed"))?,
    );
    if schema != COMPILED_WASM_PAYLOAD_SCHEMA {
        return Err(anyhow!(
            "compiled Wasm payload schema {schema} does not match {COMPILED_WASM_PAYLOAD_SCHEMA}"
        ));
    }
    let source_length = u64::from_le_bytes(
        payload[4..12]
            .try_into()
            .map_err(|_| anyhow!("compiled Wasm source length is malformed"))?,
    );
    let source_sha256: [u8; 32] = payload[12..44]
        .try_into()
        .map_err(|_| anyhow!("compiled Wasm source checksum is malformed"))?;
    let compiler_identity = u64::from_le_bytes(
        payload[44..52]
            .try_into()
            .map_err(|_| anyhow!("compiled Wasm compiler identity is malformed"))?,
    );
    if source_length != key.source_length
        || source_sha256 != key.source_sha256
        || compiler_identity != key.compiler_identity
    {
        return Err(anyhow!(
            "compiled Wasm payload identity does not match its source/compiler cache key"
        ));
    }
    let artifact_length = u64::from_le_bytes(
        payload[52..60]
            .try_into()
            .map_err(|_| anyhow!("compiled Wasm artifact length is malformed"))?,
    );
    let expected_payload_length = u64::try_from(COMPILED_WASM_PAYLOAD_HEADER_BYTES)
        .expect("compiled Wasm header length must fit u64")
        .checked_add(artifact_length)
        .ok_or_else(|| anyhow!("compiled Wasm payload length overflowed u64"))?;
    if u64::try_from(payload.len()).ok() != Some(expected_payload_length) {
        return Err(anyhow!(
            "compiled Wasm payload length {} does not match declared {expected_payload_length}",
            payload.len()
        ));
    }
    let expected_artifact_sha256 = &payload[60..92];
    let actual_artifact_sha256 = Sha256::digest(&payload[COMPILED_WASM_PAYLOAD_HEADER_BYTES..]);
    if actual_artifact_sha256.as_slice() != expected_artifact_sha256 {
        return Err(anyhow!("compiled Wasm artifact checksum does not match"));
    }
    payload.drain(..COMPILED_WASM_PAYLOAD_HEADER_BYTES);
    Ok(payload)
}
