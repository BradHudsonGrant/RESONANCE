//! Trait seam for in-memory preview fingerprinting and excerpt extraction, implemented in
//! higher crates over `resonance_library::identity` without adding audio codec deps here.

use std::fmt;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDecodeError(pub String);

impl fmt::Display for AudioDecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AudioDecodeError {}

/// Decodes in-memory previews and read-only library audio files into raw Chromaprint words or
/// compact PCM WAV excerpts.
pub trait AudioFingerprinter: Send + Sync {
    fn fingerprint_bytes(
        &self,
        bytes: &[u8],
        ext_hint: Option<&str>,
    ) -> Result<Vec<u32>, AudioDecodeError>;

    fn fingerprint_file_full(&self, path: &Path) -> Result<Vec<u32>, AudioDecodeError>;

    fn extract_wav_excerpt(
        &self,
        path: &Path,
        start_ratio: f32,
        duration_s: u32,
    ) -> Result<Vec<u8>, AudioDecodeError>;

    /// Resolves `relative_path` under `source_folder` (allowing platform canonicalisation on
    /// Windows folders ending in spaces or dots).
    fn resolve_asset_path(&self, source_folder: &str, relative_path: &str) -> PathBuf {
        let mut path = PathBuf::from(source_folder);
        for segment in relative_path.split('/').filter(|part| !part.is_empty()) {
            path.push(segment);
        }
        path
    }
}
