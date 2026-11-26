use std::fs;
use std::path::PathBuf;

use resonance_analysis::AnalysisError;
use resonance_analysis::registry::{InputSpec, ModelRegistry, ModelSpec};
use resonance_analysis::runtime::{ExecutionTarget, FloatTensor, ModelSession};
use sha2::{Digest, Sha256};

fn test_folder(name: &str) -> PathBuf {
    let folder = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&folder);
    fs::create_dir_all(&folder).expect("test folder created");
    folder
}

/// Protocol buffer fields, enough to write a small ONNX model without a Python toolchain.
fn varint(mut value: u64, out: &mut Vec<u8>) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn int_field(field: u64, value: u64, out: &mut Vec<u8>) {
    varint(field << 3, out);
    varint(value, out);
}

fn bytes_field(field: u64, bytes: &[u8], out: &mut Vec<u8>) {
    varint((field << 3) | 2, out);
    varint(bytes.len() as u64, out);
    out.extend_from_slice(bytes);
}

fn node(inputs: &[&str], output: &str, op_type: &str) -> Vec<u8> {
    let mut node = Vec::new();
    for input in inputs {
        bytes_field(1, input.as_bytes(), &mut node);
    }
    bytes_field(2, output.as_bytes(), &mut node);
    bytes_field(4, op_type.as_bytes(), &mut node);
    node
}

/// A float tensor named `name` of shape `[1, frames]`.
fn value_info(name: &str) -> Vec<u8> {
    let mut batch = Vec::new();
    int_field(1, 1, &mut batch);
    let mut frames = Vec::new();
    bytes_field(2, b"frames", &mut frames);
    let mut shape = Vec::new();
    bytes_field(1, &batch, &mut shape);
    bytes_field(1, &frames, &mut shape);
    let mut tensor = Vec::new();
    int_field(1, 1, &mut tensor);
    bytes_field(2, &shape, &mut tensor);
    let mut kind = Vec::new();
    bytes_field(1, &tensor, &mut kind);
    let mut info = Vec::new();
    bytes_field(1, name.as_bytes(), &mut info);
    bytes_field(2, &kind, &mut info);
    info
}

/// `y = relu(x) + relu(x)`.
fn doubled_relu_model() -> Vec<u8> {
    let mut graph = Vec::new();
    bytes_field(1, &node(&["x"], "r", "Relu"), &mut graph);
    bytes_field(1, &node(&["r", "r"], "y", "Add"), &mut graph);
    bytes_field(2, b"doubled_relu", &mut graph);
    bytes_field(11, &value_info("x"), &mut graph);
    bytes_field(12, &value_info("y"), &mut graph);
    let mut opset = Vec::new();
    bytes_field(1, b"", &mut opset);
    int_field(2, 13, &mut opset);
    let mut model = Vec::new();
    int_field(1, 8, &mut model);
    bytes_field(7, &graph, &mut model);
    bytes_field(8, &opset, &mut model);
    model
}

fn written_model(folder: &str) -> (PathBuf, PathBuf) {
    let folder = test_folder(folder);
    let weights = folder.join("doubled_relu.onnx");
    fs::write(&weights, doubled_relu_model()).expect("model written");
    (weights, folder.join("trt-cache"))
}

fn input() -> FloatTensor {
    FloatTensor {
        shape: vec![1, 5],
        data: vec![-1.0, 2.0, -3.0, 4.5, 0.0],
    }
}

const EXPECTED: [f32; 5] = [0.0, 4.0, 0.0, 9.0, 0.0];

#[test]
fn a_small_graph_runs_on_the_cpu() {
    let (weights, cache) = written_model("runtime_cpu");
    let mut session =
        ModelSession::open(&weights, &[ExecutionTarget::Cpu], &cache).expect("session opens");
    assert_eq!(session.target(), ExecutionTarget::Cpu);
    let outputs = session.run("x", input()).expect("inference runs");
    assert_eq!(outputs.len(), 1);
    assert_eq!(outputs[0].shape, vec![1, 5]);
    assert_eq!(outputs[0].data, EXPECTED);
}

#[test]
fn the_preferred_targets_fall_back_until_one_opens_and_agree_with_the_cpu() {
    let (weights, cache) = written_model("runtime_preferred");
    let mut session =
        ModelSession::open(&weights, ExecutionTarget::PREFERENCE, &cache).expect("a target opens");
    println!("opened on {}", session.target().name());
    let outputs = session.run("x", input()).expect("inference runs");
    assert_eq!(outputs[0].data, EXPECTED);
}

fn spec(bytes: &[u8], commercial_ok: bool) -> ModelSpec {
    let sha256: String = Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    ModelSpec {
        id: if commercial_ok {
            "doubled-relu"
        } else {
            "doubled-relu-nc"
        },
        version: "1",
        url: "https://example.invalid/doubled_relu.onnx",
        sha256: sha256.leak(),
        size_bytes: bytes.len() as u64,
        licence: if commercial_ok { "MIT" } else { "CC BY-NC 4.0" },
        commercial_ok,
        input: InputSpec {
            name: "x",
            shape: &[1, -1],
            sample_rate_hz: 22_050,
        },
    }
}

#[test]
fn installed_weights_are_checked_before_they_are_kept() {
    let data = test_folder("registry_checksum");
    let model = doubled_relu_model();
    let good = spec(&model, true);
    let registry = ModelRegistry::new(&data, vec![good.clone()], false);

    let path = registry
        .install(&good, model.as_slice())
        .expect("matching weights install");
    assert_eq!(path, data.join("models/doubled-relu/1/model.onnx"));
    assert_eq!(fs::read(&path).expect("weights read"), model);
    assert_eq!(
        registry.weights("doubled-relu").expect("cached weights"),
        path
    );
    fs::remove_file(&path).expect("weights removed");

    let mut tampered = model.clone();
    tampered[10] ^= 0xff;
    let error = registry
        .install(&good, tampered.as_slice())
        .expect_err("tampered weights are rejected");
    assert!(
        matches!(error, AnalysisError::WrongChecksum { .. }),
        "{error}"
    );
    let short = &model[..model.len() - 1];
    let error = registry
        .install(&good, short)
        .expect_err("short weights are rejected");
    assert!(matches!(error, AnalysisError::WrongSize { .. }), "{error}");
    let mut long = model.clone();
    long.push(0);
    let error = registry
        .install(&good, long.as_slice())
        .expect_err("long weights are rejected");
    assert!(matches!(error, AnalysisError::WrongSize { .. }), "{error}");

    let folder = path.parent().expect("model folder");
    assert_eq!(fs::read_dir(folder).expect("folder lists").count(), 0);
}

#[test]
fn non_commercial_models_fetch_only_once_allowed() {
    let data = test_folder("registry_licence");
    let model = doubled_relu_model();
    let restricted = spec(&model, false);

    let default = ModelRegistry::new(&data, vec![restricted.clone()], false);
    let error = default
        .weights(restricted.id)
        .expect_err("blocked by default");
    assert!(
        matches!(error, AnalysisError::NonCommercial { licence, .. } if licence == "CC BY-NC 4.0")
    );
    let error = default
        .install(&restricted, model.as_slice())
        .expect_err("blocked by default");
    assert!(matches!(error, AnalysisError::NonCommercial { .. }));
    assert!(!default.weights_path(&restricted).exists());

    let allowed = ModelRegistry::new(&data, vec![restricted.clone()], true);
    let path = allowed
        .install(&restricted, model.as_slice())
        .expect("installs once allowed");
    assert!(path.exists());

    let error = allowed.weights("missing").expect_err("unknown model");
    assert!(matches!(error, AnalysisError::UnknownModel { .. }));
}
