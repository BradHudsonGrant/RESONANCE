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

    /// The model's weights, fetched and checked first if they are not here yet.
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
        let body = response.into_body().into_reader();
        self.install(spec, body)
    }

    /// Writes the weights read from `body` into place. They land under a temporary name first
    /// and only take the real one once their size and checksum match the spec, so a broken or
    /// tampered download is never loaded.
    pub fn install(&self, spec: &ModelSpec, mut body: impl Read) -> Result<PathBuf, AnalysisError> {
        let spec = self.allowed(spec.id)?;
        let path = self.weights_path(spec);
        let folder = path.parent().unwrap_or(&self.folder).to_path_buf();
        fs::create_dir_all(&folder).map_err(|source| AnalysisError::File {
            path: folder.clone(),
            source,
        })?;
        let partial = path.with_extension("onnx.part");
        let file_error = |source| AnalysisError::File {
            path: partial.clone(),
            source,
        };

        let mut file = File::create(&partial).map_err(file_error)?;
        let mut hasher = Sha256::new();
        let mut size = 0u64;
        let mut buffer = vec![0u8; 1 << 16];
        let copied = loop {
            let read = match body.read(&mut buffer) {
                Ok(0) => break Ok(()),
                Ok(read) => read,
                Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                Err(error) => break Err(error),
            };
            hasher.update(&buffer[..read]);
            size += read as u64;
            if size > spec.size_bytes {
                break Ok(());
            }
            if let Err(error) = file.write_all(&buffer[..read]) {
                break Err(error);
            }
        };
        let flushed = copied.and_then(|()| file.sync_all());
        drop(file);
        if let Err(source) = flushed {
            let _ = fs::remove_file(&partial);
            return Err(file_error(source));
        }

        let found = hex(&hasher.finalize());
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
        fs::rename(&partial, &path).map_err(file_error)?;
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

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
