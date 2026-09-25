#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

mod archive;

/// Current signed Ghost media manifest version.
pub const GHOST_MEDIA_MANIFEST_VERSION: u16 = 1;
pub use archive::{
    archive_payload_overhead, decode_archive_chunk, encode_archive_chunk, KaspaArchiveChunk,
    KaspaArchiveLocator,
};

/// Stable content-addressed identity for media bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaReference {
    pub media_id: String,
    pub content_hash: String,
    pub content_type: String,
    pub size: u64,
    pub location: MediaLocation,
}

/// Retrieval hint. Integrity never depends on this location being trusted.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "camelCase")]
pub enum MediaLocation {
    #[default]
    Local,
    Remote(String),
    KaspaArchive(String),
}

/// Independently verifiable chunk in a media object.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaChunk {
    pub index: u32,
    pub offset: u64,
    pub size: u32,
    pub content_hash: String,
    pub reference: String,
}

/// Optional encryption description carried by the signed manifest.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EncryptionMetadata {
    pub scheme: String,
    pub key_reference: String,
}

/// Versioned signed description of a content-addressed media object.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GhostMediaManifest {
    pub version: u16,
    pub media_id: String,
    pub creator: String,
    pub title: String,
    pub content_type: String,
    pub codec: String,
    pub duration_ms: u64,
    pub total_size: u64,
    pub content_hash: String,
    #[serde(default)]
    pub chunks: Vec<MediaChunk>,
    #[serde(default)]
    pub encryption: Option<EncryptionMetadata>,
    pub creator_signature: String,
}

impl GhostMediaManifest {
    /// Canonical manifest bytes covered by the creator signature.
    pub fn signing_bytes(&self) -> Result<Vec<u8>, String> {
        let mut unsigned = self.clone();
        unsigned.creator_signature.clear();
        serde_json::to_vec(&unsigned)
            .map_err(|error| format!("media manifest serialization failed: {error}"))
    }

    /// Checks structural invariants before signing, publishing, or accepting a manifest.
    pub fn validate(&self) -> Result<(), String> {
        if self.version != GHOST_MEDIA_MANIFEST_VERSION {
            return Err("unsupported Ghost media manifest version".into());
        }
        validate_digest("media id", &self.media_id)?;
        validate_digest("content hash", &self.content_hash)?;
        if !self.media_id.eq_ignore_ascii_case(&self.content_hash) {
            return Err("media id must equal the canonical content hash".into());
        }
        if self.creator.trim().is_empty() || self.content_type.trim().is_empty() {
            return Err("media manifest creator and content type are required".into());
        }
        if self.total_size == 0 {
            return Err("media manifest cannot describe empty media".into());
        }
        validate_manifest_chunks(&self.chunks, self.total_size)
    }
}

fn validate_digest(label: &str, value: &str) -> Result<(), String> {
    let decoded = hex::decode(value).map_err(|_| format!("{label} must be hexadecimal"))?;
    if decoded.len() != 32 {
        return Err(format!("{label} must be a 32-byte digest"));
    }
    Ok(())
}

fn validate_manifest_chunks(chunks: &[MediaChunk], total_size: u64) -> Result<(), String> {
    if chunks.is_empty() {
        return Ok(());
    }
    let mut offset = 0_u64;
    for (expected_index, chunk) in chunks.iter().enumerate() {
        if chunk.index as usize != expected_index || chunk.offset != offset || chunk.size == 0 {
            return Err("media manifest chunks must be contiguous, ordered, and non-empty".into());
        }
        validate_digest("chunk content hash", &chunk.content_hash)?;
        offset = offset
            .checked_add(chunk.size as u64)
            .ok_or_else(|| "media manifest chunk size overflow".to_string())?;
    }
    if offset != total_size {
        return Err("media manifest chunk sizes do not match total size".into());
    }
    Ok(())
}

/// Build a manifest from a verified media reference. Signing is performed by
/// the owning identity/Kaspa layer, not by this media-integrity crate.
pub fn manifest_for_reference(
    reference: &MediaReference,
    creator: String,
    title: String,
    codec: String,
    duration_ms: u64,
) -> GhostMediaManifest {
    GhostMediaManifest {
        version: GHOST_MEDIA_MANIFEST_VERSION,
        media_id: reference.media_id.clone(),
        creator,
        title,
        content_type: reference.content_type.clone(),
        codec,
        duration_ms,
        total_size: reference.size,
        content_hash: reference.content_hash.clone(),
        chunks: Vec::new(),
        encryption: None,
        creator_signature: String::new(),
    }
}

/// Compute Ghost's canonical media content hash.
pub fn content_hash(bytes: &[u8]) -> String {
    hex::encode(blake3::hash(bytes).as_bytes())
}

/// Verify bytes against a signed/reference hash without trusting their source.
pub fn verify_content(bytes: &[u8], expected_hash: &str) -> bool {
    content_hash(bytes).eq_ignore_ascii_case(expected_hash.trim())
}

/// Verify an independently retrievable media chunk.
pub fn verify_chunk(bytes: &[u8], chunk: &MediaChunk) -> bool {
    bytes.len() == chunk.size as usize && verify_content(bytes, &chunk.content_hash)
}

/// Build independently verifiable fixed-size chunks from encoded media bytes.
pub fn chunk_media(bytes: &[u8], chunk_size: usize, reference_prefix: &str) -> Vec<MediaChunk> {
    let size = chunk_size.max(1);
    bytes
        .chunks(size)
        .enumerate()
        .map(|(index, chunk)| MediaChunk {
            index: index as u32,
            offset: (index * size) as u64,
            size: chunk.len() as u32,
            content_hash: content_hash(chunk),
            reference: format!("{reference_prefix}/{index}"),
        })
        .collect()
}

/// Verify a reconstructed object against both chunk metadata and the manifest root hash.
pub fn verify_reconstructed(manifest: &GhostMediaManifest, chunks: &[Vec<u8>]) -> bool {
    if manifest.chunks.len() != chunks.len() {
        return false;
    }
    if !manifest
        .chunks
        .iter()
        .zip(chunks)
        .all(|(meta, bytes)| verify_chunk(bytes, meta))
    {
        return false;
    }
    let joined: Vec<u8> = chunks
        .iter()
        .flat_map(|chunk| chunk.iter().copied())
        .collect();
    joined.len() as u64 == manifest.total_size && verify_content(&joined, &manifest.content_hash)
}

/// Minimal cache contract shared by avatars, attachments, recordings, podcasts and archives.
pub trait MediaCache {
    fn get(&self, media_id: &str) -> Option<Vec<u8>>;
    fn put(&mut self, reference: &MediaReference, bytes: Vec<u8>) -> Result<(), String>;
}

/// Retrieval transport is intentionally separate from integrity verification.
pub trait MediaRetriever {
    fn retrieve(&self, reference: &MediaReference) -> Result<Vec<u8>, String>;
}

pub fn retrieve_verified<R: MediaRetriever>(
    retriever: &R,
    reference: &MediaReference,
) -> Result<Vec<u8>, String> {
    let bytes = retriever.retrieve(reference)?;
    if bytes.len() as u64 != reference.size {
        return Err("media size mismatch".into());
    }
    if !verify_content(&bytes, &reference.content_hash) {
        return Err("media content hash mismatch".into());
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_verification_detects_substitution() {
        let hash = content_hash(b"avatar");
        assert!(verify_content(b"avatar", &hash));
        assert!(!verify_content(b"substitute", &hash));
    }

    #[test]
    fn chunked_reconstruction_is_verified_end_to_end() {
        let bytes = b"0123456789";
        let chunks = chunk_media(bytes, 4, "mem://asset");
        let manifest = GhostMediaManifest {
            total_size: bytes.len() as u64,
            content_hash: content_hash(bytes),
            chunks: chunks.clone(),
            ..GhostMediaManifest::default()
        };
        let bodies = bytes
            .chunks(4)
            .map(|part| part.to_vec())
            .collect::<Vec<_>>();
        assert!(verify_reconstructed(&manifest, &bodies));
    }

    fn valid_manifest(bytes: &[u8]) -> GhostMediaManifest {
        let hash = content_hash(bytes);
        GhostMediaManifest {
            version: GHOST_MEDIA_MANIFEST_VERSION,
            media_id: hash.clone(),
            creator: "kaspa:creator".into(),
            content_type: "audio/webm".into(),
            total_size: bytes.len() as u64,
            content_hash: hash,
            chunks: chunk_media(bytes, 4, "kaspa://archive"),
            ..GhostMediaManifest::default()
        }
    }

    #[test]
    fn manifest_rejects_non_hex_or_noncanonical_content_identity() {
        let mut manifest = valid_manifest(b"media payload");
        manifest.media_id = "z".repeat(64);
        assert!(manifest.validate().is_err());
        manifest = valid_manifest(b"media payload");
        manifest.media_id = content_hash(b"different payload");
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn manifest_rejects_non_contiguous_or_wrong_sized_chunks() {
        let mut manifest = valid_manifest(b"0123456789");
        manifest.chunks[1].offset = 99;
        assert!(manifest.validate().is_err());
        manifest = valid_manifest(b"0123456789");
        manifest.chunks.pop();
        assert!(manifest.validate().is_err());
    }
}
