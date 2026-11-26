use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;

use resonance_core::analysis::may_fetch_model;
use sha2::{Digest, Sha256};

use crate::AnalysisError;

/// The folder under the data folder that holds model weights. They are fetched on first use and
/// never kept in git.
pub const MODELS_FOLDER: &str = "models";
/// TensorRT's compiled engines, kept so a session opens quickly the second time.
pub const TENSORRT_CACHE_FOLDER: &str = "trt-cache";

const CONNECT_TIMEOUT: Duration = Duration::from_secs(15);
const READ_TIMEOUT: Duration = Duration::from_secs(30);
const USER_AGENT: &str = concat!(
    "Resonance/",
    env!("CARGO_PKG_VERSION"),
    " ( https://github.com/BradHudsonGrant/RESONANCE )"
);

/// The tensor a model reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InputSpec {
    pub name: &'static str,
    /// `-1` marks a dimension that varies, such as the number of frames.
    pub shape: &'static [i64],
    pub sample_rate_hz: u32,
}

/// Everything known about one model before it is fetched.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelSpec {
    pub id: &'static str,
    pub version: &'static str,
    pub url: &'static str,
    /// Lowercase hex.
    pub sha256: &'static str,
    pub size_bytes: u64,
    pub licence: &'static str,
    pub commercial_ok: bool,
    pub input: InputSpec,
}

/// The models the app knows, and where their weights live once fetched.
pub struct ModelRegistry {
    models: Vec<ModelSpec>,
    folder: PathBuf,
    non_commercial_allowed: bool,
}

impl ModelRegistry {
    pub fn new(data_folder: &Path, models: Vec<ModelSpec>, non_commercial_allowed: bool) -> Self {
        Self {
            models,
            folder: data_folder.join(MODELS_FOLDER),
            non_commercial_allowed,
        }
    }

    pub fn models(&self) -> &[ModelSpec] {
        &self.models
    }

    pub fn tensorrt_cache(&self) -> PathBuf {
        self.folder.join(TENSORRT_CACHE_FOLDER)
    }

    /// Where the model's weights sit once fetched.
    pub fn weights_path(&self, spec: &ModelSpec) -> PathBuf {
        self.folder
            .join(spec.id)
            .join(spec.version)
            .join("model.onnx")
    }

    /// The model's weights, fetched and checked first if they are not here yet. A file only
    /// takes its final name once it has been checked, so one of the right size is trusted.
    pub fn weights(&self, id: &str) -> Result<PathBuf, AnalysisError> {
        let spec = self.allowed(id)?;
        let path = self.weights_path(spec);
        if fs::metadata(&path).is_ok_and(|meta| meta.len() == spec.size_bytes) {
            return Ok(path);
        }
        let response = agent()
            .get(spec.url)
            .call()
            .map_err(|source| AnalysisError::Download {
                url: spec.url,
                source: Box::new(source),
            })?;
        self.install(id, response.into_body().into_reader())
    }

    /// Writes the weights read from `body` into place. They land under a temporary name first
    /// and only take the real one once their size and checksum match the spec, so a broken or
    /// tampered download is never loaded.
    pub fn install(&self, id: &str, body: impl Read) -> Result<PathBuf, AnalysisError> {
        let spec = self.allowed(id)?;
        let path = self.weights_path(spec);
        let partial = path.with_extension("onnx.part");
        let file_error = |path: &Path| {
            let path = path.to_path_buf();
            move |source| AnalysisError::File { path, source }
        };
        if let Some(folder) = path.parent() {
            fs::create_dir_all(folder).map_err(file_error(folder))?;
        }

        // One byte past the expected size is enough to tell that a download is too long.
        let (size, found) =
            write_hashed(body.take(spec.size_bytes + 1), &partial).map_err(|source| {
                let _ = fs::remove_file(&partial);
                AnalysisError::File {
                    path: partial.clone(),
                    source,
                }
            })?;
        let mismatch = if size != spec.size_bytes {
            Some(AnalysisError::WrongSize {
                id: spec.id.to_owned(),
                expected: spec.size_bytes,
                found: size,
            })
        } else if found != spec.sha256 {
            Some(AnalysisError::WrongChecksum {
                id: spec.id.to_owned(),
                expected: spec.sha256,
                found,
            })
        } else {
            None
        };
        if let Some(error) = mismatch {
            let _ = fs::remove_file(&partial);
            return Err(error);
        }
        fs::rename(&partial, &path).map_err(file_error(&partial))?;
        Ok(path)
    }

    fn allowed(&self, id: &str) -> Result<&ModelSpec, AnalysisError> {
        let spec = self
            .models
            .iter()
            .find(|spec| spec.id == id)
            .ok_or_else(|| AnalysisError::UnknownModel { id: id.to_owned() })?;
        if !may_fetch_model(spec.commercial_ok, self.non_commercial_allowed) {
            return Err(AnalysisError::NonCommercial {
                id: spec.id.to_owned(),
                licence: spec.licence,
            });
        }
        Ok(spec)
    }
}

fn agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::PlatformVerifier)
        .build();
    ureq::Agent::config_builder()
        .timeout_connect(Some(CONNECT_TIMEOUT))
        .timeout_recv_body(None)
        .timeout_recv_response(Some(READ_TIMEOUT))
        .user_agent(USER_AGENT)
        .tls_config(tls)
        .build()
        .into()
}

/// Copies `body` into a new file at `path` and returns its size and lowercase hex SHA-256.
fn write_hashed(mut body: impl Read, path: &Path) -> io::Result<(u64, String)> {
    let mut file = File::create(path)?;
    let mut hasher = Sha256::new();
    let mut size = 0u64;
    let mut buffer = vec![0u8; 1 << 16];
    loop {
        let read = match body.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(error),
        };
        hasher.update(&buffer[..read]);
        file.write_all(&buffer[..read])?;
        size += read as u64;
    }
    file.sync_all()?;
    let found = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    Ok((size, found))
}
