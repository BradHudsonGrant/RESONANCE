use std::fs;
use std::path::Path;
use std::sync::{Mutex, OnceLock, PoisonError};

use ort::environment::Environment;
#[cfg(all(windows, target_arch = "x86_64"))]
use ort::ep::{self, ExecutionProvider};
use ort::session::Session;
use ort::session::builder::SessionBuilder;
use ort::value::Tensor;

use crate::AnalysisError;

/// Where a session's work runs. Each is tried in order and the first that opens the model wins,
/// so a machine without an NVIDIA GPU or its libraries still runs on the CPU.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionTarget {
    #[cfg(all(windows, target_arch = "x86_64"))]
    TensorRt,
    #[cfg(all(windows, target_arch = "x86_64"))]
    Cuda,
    Cpu,
}

impl ExecutionTarget {
    pub const PREFERENCE: &[Self] = &[
        #[cfg(all(windows, target_arch = "x86_64"))]
        Self::TensorRt,
        #[cfg(all(windows, target_arch = "x86_64"))]
        Self::Cuda,
        Self::Cpu,
    ];

    pub const fn name(self) -> &'static str {
        match self {
            #[cfg(all(windows, target_arch = "x86_64"))]
            Self::TensorRt => "TensorRT",
            #[cfg(all(windows, target_arch = "x86_64"))]
            Self::Cuda => "CUDA",
            Self::Cpu => "CPU",
        }
    }

    pub const fn uses_gpu(self) -> bool {
        !matches!(self, Self::Cpu)
    }

    #[cfg_attr(
        not(all(windows, target_arch = "x86_64")),
        expect(
            unused_variables,
            reason = "only the GPU providers need a cache folder"
        )
    )]
    fn register(self, builder: &mut SessionBuilder, tensorrt_cache: &Path) -> ort::Result<()> {
        match self {
            #[cfg(all(windows, target_arch = "x86_64"))]
            Self::TensorRt => {
                let cache = tensorrt_cache.display().to_string();
                ep::TensorRT::default()
                    .with_engine_cache(true)
                    .with_engine_cache_path(&cache)
                    .with_timing_cache(true)
                    .with_timing_cache_path(&cache)
                    .register(builder)
            }
            #[cfg(all(windows, target_arch = "x86_64"))]
            Self::Cuda => ep::CUDA::default().register(builder),
            Self::Cpu => Ok(()),
        }
    }
}

/// ONNX Runtime allows one environment per process.
fn environment() -> Result<&'static Environment, AnalysisError> {
    static ENVIRONMENT: OnceLock<Environment> = OnceLock::new();
    static BUILDING: Mutex<()> = Mutex::new(());
    if let Some(environment) = ENVIRONMENT.get() {
        return Ok(environment);
    }
    let _building = BUILDING.lock().unwrap_or_else(PoisonError::into_inner);
    if let Some(environment) = ENVIRONMENT.get() {
        return Ok(environment);
    }
    let built = ort::init().with_name("resonance").build()?;
    Ok(ENVIRONMENT.get_or_init(|| built))
}

/// A float tensor in or out of a model, in row-major order.
#[derive(Debug, Clone, PartialEq)]
pub struct FloatTensor {
    pub shape: Vec<i64>,
    pub data: Vec<f32>,
}

/// One model loaded for inference.
pub struct ModelSession {
    session: Session,
    target: ExecutionTarget,
}

impl ModelSession {
    /// Opens the model on the fastest target that loads it. The CPU comes last, and its error is
    /// the one returned if nothing opens. `tensorrt_cache` keeps TensorRT's compiled engines.
    pub fn open(weights: &Path, tensorrt_cache: &Path) -> Result<Self, AnalysisError> {
        for &target in ExecutionTarget::PREFERENCE {
            if target.uses_gpu()
                && let Ok(session) = Self::open_on(weights, target, tensorrt_cache)
            {
                return Ok(session);
            }
        }
        Self::open_on(weights, ExecutionTarget::Cpu, tensorrt_cache)
    }

    /// Opens the model on `target` only.
    pub fn open_on(
        weights: &Path,
        target: ExecutionTarget,
        tensorrt_cache: &Path,
    ) -> Result<Self, AnalysisError> {
        if target.uses_gpu() {
            fs::create_dir_all(tensorrt_cache).map_err(|source| AnalysisError::File {
                path: tensorrt_cache.to_path_buf(),
                source,
            })?;
        }
        let mut builder = Session::builder(environment()?)?;
        target.register(&mut builder, tensorrt_cache)?;
        let session = builder.commit_from_file(weights)?;
        Ok(Self { session, target })
    }

    pub fn target(&self) -> ExecutionTarget {
        self.target
    }

    /// Runs the model on one named input and returns its outputs in the model's order.
    pub fn run(
        &mut self,
        input_name: &str,
        input: FloatTensor,
    ) -> Result<Vec<FloatTensor>, AnalysisError> {
        let tensor = Tensor::from_array((input.shape, input.data))?;
        let outputs = self.session.run(ort::inputs![input_name => tensor])?;
        let mut results = Vec::with_capacity(outputs.len());
        for (name, value) in outputs.iter() {
            let (shape, data) =
                value
                    .try_extract_tensor::<f32>()
                    .map_err(|_| AnalysisError::UnexpectedOutput {
                        output: name.to_owned(),
                    })?;
            results.push(FloatTensor {
                shape: shape.to_vec(),
                data: data.to_vec(),
            });
        }
        Ok(results)
    }
}
