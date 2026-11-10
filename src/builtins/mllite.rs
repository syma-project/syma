//! neural-net-lite: layers, containers, initialization and SGD training.
//!
//! Gap-fill module; see the individual builtin docs for semantics.
//!
//! # Representation
//!
//! A "net" is a [`Value::Object`] with class name `"NeuralNet"`. The `Type`
//! field distinguishes the layer/container kind (`"Chain"`, `"Graph"`,
//! `"Linear"`, `"Elementwise"`, `"Softmax"`, `"Convolution"`, `"Pooling"`,
//! `"Dropout"`, `"BatchNorm"`, `"Flatten"`, `"Reshape"`, `"Threading"`,
//! `"MeanSquaredLoss"`, `"CrossEntropyLoss"`) and the remaining fields carry
//! its architecture:
//!
//! * `Linear` — `Out` (int), optional `In` (int), optional `Weights`/`Biases`.
//! * `Elementwise`/`Threading` — `Activation`/`Function` (function value).
//! * `Convolution` — `Kernels`, `KernelShape`, `Stride`, `Padding`, weights.
//! * `Pooling` — `Size`, `Stride`, `Function` (`"Max"`/`"Average"`).
//! * `BatchNorm` — `Mean`, `Variance`, `Epsilon` (inference statistics).
//! * `Flatten`/`Reshape`/`Dropout`/`Softmax` — no parameters.
//! * `Chain` — `Layers` (a list of layer objects).
//! * `Graph` — `Nodes` (layers) and `Edges` (1-based `{i, j}` pairs).
//!
//! An `Object` (rather than a `Value::Call`) is used because applying a value
//! dispatches to `"<Class>.__method__"` (see `eval::apply_function`), which
//! gives an actual callable `net[x]`. A bare `Value::Call` head is not a valid
//! symbol and would be re-stringified on each application.
//!
//! # Tensors
//!
//! Inputs and outputs of layers are nested lists (row-major). Internally they
//! are carried as a [`Tensor`] (flat buffer + shape) so convolutions, pooling
//! and reshaping are straightforward, and re-materialized to nested lists on
//! the way out. Rank ≤ 1 keeps the familiar flat-vector shape.
//!
//! # Determinism
//!
//! Weights are generated deterministically from a per-layer seed using the
//! shared `random` RNG (the same one driven by `SeedRandom`). Thus
//! `NetInitialize`/first evaluation are reproducible, and `SeedRandom[n]` is
//! reused as the seeding mechanism. `NetTrain` is full-batch gradient descent
//! with fixed defaults, so it is deterministic too.
//!
//! Unsupported argument shapes return the unevaluated `Value::Call`.

#![allow(clippy::needless_range_loop)]

use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::Integer;
use std::collections::HashMap;

/// Class name shared by every net object.
const NET_CLASS: &str = "NeuralNet";
/// RNG seed reused for `NetInitialize` (deterministic initialization).
const NET_INIT_SEED: u64 = 0x4E45_5457_4B01_0001;
/// Numerical floor inside batch normalization.
const BN_EPSILON: f64 = 1e-5;

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "LinearLayer", builtin_linear_layer);
    super::register_builtin(env, "ElementwiseLayer", builtin_elementwise_layer);
    super::register_builtin(env, "ThreadingLayer", builtin_threading_layer);
    super::register_builtin(env, "SoftmaxLayer", builtin_softmax_layer);
    super::register_builtin(env, "ConvolutionLayer", builtin_convolution_layer);
    super::register_builtin(env, "PoolingLayer", builtin_pooling_layer);
    super::register_builtin(env, "DropoutLayer", builtin_dropout_layer);
    super::register_builtin(env, "BatchNormalizationLayer", builtin_batchnorm_layer);
    super::register_builtin(env, "FlattenLayer", builtin_flatten_layer);
    super::register_builtin(env, "ReshapeLayer", builtin_reshape_layer);
    super::register_builtin(env, "MeanSquaredLossLayer", builtin_mse_layer);
    super::register_builtin(env, "CrossEntropyLossLayer", builtin_ce_layer);
    super::register_builtin(env, "LossFunction", builtin_loss_function);
    super::register_builtin(env, "NetChain", builtin_net_chain);
    super::register_builtin(env, "NetGraph", builtin_net_graph);
    super::register_builtin(env, "NetExtract", builtin_net_extract);
    super::register_builtin_env(env, "NetInitialize", builtin_net_initialize);
    super::register_builtin_env(env, "NetTrain", builtin_net_train);
    super::register_builtin_env(env, "NetEvaluate", builtin_net_evaluate);
    // `net[x]` — method dispatch for net objects.
    super::register_builtin_env(env, "NeuralNet.__method__", builtin_net_call);
}

// ── Small helpers ───────────────────────────────────────────────────────────

fn unevaluated(name: &str, args: &[Value]) -> Value {
    Value::Call {
        head: name.to_string(),
        args: args.to_vec(),
    }
}

fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

fn as_list(v: &Value) -> Option<&Vec<Value>> {
    match v {
        Value::List(items) => Some(items),
        _ => None,
    }
}

fn obj_fields(v: &Value) -> Option<&HashMap<String, Value>> {
    match v {
        Value::Object { class_name, fields } if class_name == NET_CLASS => Some(fields),
        _ => None,
    }
}

fn net_type(v: &Value) -> Option<&str> {
    match obj_fields(v)?.get("Type")? {
        Value::Str(s) => Some(s.as_str()),
        _ => None,
    }
}

fn get_i64(fields: &HashMap<String, Value>, key: &str) -> Option<i64> {
    match fields.get(key)? {
        Value::Integer(n) => n.to_i64(),
        _ => None,
    }
}

fn get_f64(fields: &HashMap<String, Value>, key: &str) -> Option<f64> {
    fields.get(key).and_then(super::to_f64)
}

fn make_net(fields: &[(&str, Value)]) -> Value {
    let mut map = HashMap::new();
    for (k, v) in fields {
        map.insert((*k).to_string(), v.clone());
    }
    Value::Object {
        class_name: NET_CLASS.to_string(),
        fields: map,
    }
}

/// Unwrap a `Pattern`/`Hold` wrapper to the underlying value.
fn deref(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => crate::pattern::unwrap_expr_to_value(e),
        Value::Hold(b) | Value::HoldComplete(b) => (**b).clone(),
        other => other.clone(),
    }
}

/// Convert a scalar or flat list-of-numbers value into a `Vec<f64>`.
fn to_vec(v: &Value) -> Option<Vec<f64>> {
    let v = deref(v);
    match &v {
        Value::List(items) => items.iter().map(super::to_f64).collect(),
        other => super::to_f64(other).map(|x| vec![x]),
    }
}

fn value_from_vec(xs: &[f64]) -> Value {
    Value::List(xs.iter().map(|x| super::real(*x)).collect())
}

fn is_loss(v: &Value) -> bool {
    matches!(
        net_type(v),
        Some("MeanSquaredLoss") | Some("CrossEntropyLoss")
    )
}

fn is_linear(v: &Value) -> bool {
    net_type(v) == Some("Linear")
}

fn product(xs: &[usize]) -> usize {
    xs.iter().product()
}

/// Row-major linear index of `coords` in a tensor with `shape`.
fn lin_index(coords: &[usize], shape: &[usize]) -> usize {
    coords
        .iter()
        .zip(shape.iter())
        .fold(0usize, |acc, (&c, &s)| acc * s + c)
}

/// Row-major multi-index of `idx` in a tensor with `shape`.
fn decompose(mut idx: usize, shape: &[usize]) -> Vec<usize> {
    let mut coords = vec![0usize; shape.len()];
    for i in (0..shape.len()).rev() {
        if shape[i] == 0 {
            continue;
        }
        coords[i] = idx % shape[i];
        idx /= shape[i];
    }
    coords
}

/// Deterministic per-layer seed derived from its position and shape.
fn layer_seed(index: usize, out: usize, input: usize) -> u64 {
    const M1: u64 = 0x9E37_79B9_7F4A_7C15;
    const M2: u64 = 0xBF58_476D_1CE4_E5B9;
    M1 ^ (index as u64)
        .wrapping_mul(M2)
        .wrapping_add((out as u64).wrapping_mul(31))
        .wrapping_add(input as u64)
}

/// Parse a positive whole number, or a non-empty list of them, into `Vec<usize>`.
fn usize_list(v: &Value) -> Option<Vec<usize>> {
    let whole = |x: f64| -> Option<usize> {
        if x >= 1.0 && x.fract() == 0.0 {
            Some(x as usize)
        } else {
            None
        }
    };
    let v = deref(v);
    match &v {
        Value::List(items) => {
            let mut out = Vec::with_capacity(items.len());
            for it in items {
                out.push(whole(super::to_f64(&deref(it))?)?);
            }
            if out.is_empty() { None } else { Some(out) }
        }
        other => Some(vec![whole(super::to_f64(other)?)?]),
    }
}

// ── Tensors ─────────────────────────────────────────────────────────────────

/// Row-major dense tensor with an explicit shape.
#[derive(Clone, Debug, PartialEq)]
struct Tensor {
    shape: Vec<usize>,
    data: Vec<f64>,
}

impl Tensor {
    fn vector(data: Vec<f64>) -> Self {
        Tensor {
            shape: vec![data.len()],
            data,
        }
    }

    fn from_flat(shape: Vec<usize>, data: Vec<f64>) -> Option<Self> {
        if product(&shape) == data.len() {
            Some(Tensor { shape, data })
        } else {
            None
        }
    }

    fn rank(&self) -> usize {
        self.shape.len()
    }

    fn numel(&self) -> usize {
        self.data.len()
    }

    /// Parse a nested list (or scalar) into a rectangular tensor.
    fn from_value(v: &Value) -> Option<Self> {
        let v = deref(v);
        let mut shape = Vec::new();
        let mut data = Vec::new();
        if !collect_tensor(&v, 0, &mut shape, &mut data) {
            return None;
        }
        Some(Tensor { shape, data })
    }

    /// Materialize back into a (possibly nested) `Value::List`.
    fn to_value(&self) -> Value {
        let mut idx = 0usize;
        self.build_value(0, &mut idx)
    }

    fn build_value(&self, depth: usize, idx: &mut usize) -> Value {
        if depth == self.shape.len() {
            let x = self.data[*idx];
            *idx += 1;
            return super::real(x);
        }
        let n = self.shape[depth];
        let mut items = Vec::with_capacity(n);
        for _ in 0..n {
            items.push(self.build_value(depth + 1, idx));
        }
        Value::List(items)
    }

    /// Reshape to `shape` when the element count matches.
    fn reshape(&self, shape: &[usize]) -> Option<Tensor> {
        Tensor::from_flat(shape.to_vec(), self.data.clone())
    }
}

/// Recursive scalar/nested-list collector with rectangularity checking.
fn collect_tensor(v: &Value, depth: usize, shape: &mut Vec<usize>, data: &mut Vec<f64>) -> bool {
    match v {
        Value::List(items) => {
            if shape.len() == depth {
                shape.push(items.len());
            } else if shape[depth] != items.len() {
                return false;
            }
            if items.is_empty() {
                return true;
            }
            for it in items {
                let it = deref(it);
                if !collect_tensor(&it, depth + 1, shape, data) {
                    return false;
                }
            }
            true
        }
        other => {
            if shape.len() != depth {
                return false;
            }
            match super::to_f64(other) {
                Some(x) => {
                    data.push(x);
                    true
                }
                None => false,
            }
        }
    }
}

/// Convert any value to a tensor, falling back to a flat vector interpretation.
fn tensor_from_value(v: &Value) -> Option<Tensor> {
    Tensor::from_value(v).or_else(|| to_vec(v).map(Tensor::vector))
}

// ── Weights ─────────────────────────────────────────────────────────────────

/// A dense linear layer's weights `w[out][in]` and biases `b[out]`.
#[derive(Clone)]
struct Lin {
    w: Vec<Vec<f64>>,
    b: Vec<f64>,
}

fn init_weights(input: usize, out: usize, seed: u64) -> Lin {
    // Reuse the shared `SeedRandom` RNG, reseeded deterministically.
    super::random::seed_rng(seed);
    super::random::with_rng(|rng| {
        let scale = 1.0 / (input.max(1) as f64).sqrt();
        let mut w = vec![vec![0.0f64; input]; out];
        for row in w.iter_mut() {
            for c in row.iter_mut() {
                *c = (rng.f64() * 2.0 - 1.0) * scale;
            }
        }
        Lin {
            w,
            b: vec![0.0; out],
        }
    })
}

/// Read stored weights if present and shape-compatible.
fn stored_weights(fields: &HashMap<String, Value>, input: usize, out: usize) -> Option<Lin> {
    let w_val = as_list(fields.get("Weights")?)?;
    let b_val = as_list(fields.get("Biases")?)?;
    if w_val.len() != out || b_val.len() != out {
        return None;
    }
    let mut w = Vec::with_capacity(out);
    for row in w_val {
        let row = as_list(row)?;
        if row.len() != input {
            return None;
        }
        w.push(row.iter().map(super::to_f64).collect::<Option<Vec<_>>>()?);
    }
    let b = b_val
        .iter()
        .map(super::to_f64)
        .collect::<Option<Vec<_>>>()?;
    Some(Lin { w, b })
}

/// Materialize the weights of a linear layer, generating them when absent.
fn materialize_linear(layer: &Value, input: usize, index: usize) -> Option<Lin> {
    let fields = obj_fields(layer)?;
    let out = get_i64(fields, "Out")? as usize;
    if let Some(l) = stored_weights(fields, input, out) {
        return Some(l);
    }
    Some(init_weights(input, out, layer_seed(index, out, input)))
}

fn lin_to_fields(layer: &Value, lin: &Lin) -> Value {
    let mut fields: Vec<(String, Value)> = match layer {
        Value::Object { fields, .. } => {
            fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        }
        _ => Vec::new(),
    };
    let out = lin.w.len();
    let input = lin.w.first().map(Vec::len).unwrap_or(0);
    let set = |fields: &mut Vec<(String, Value)>, k: &str, v: Value| {
        if let Some(slot) = fields.iter_mut().find(|(name, _)| name == k) {
            slot.1 = v;
        } else {
            fields.push((k.to_string(), v));
        }
    };
    set(&mut fields, "Type", Value::Str("Linear".to_string()));
    set(&mut fields, "Out", int(out as i64));
    set(&mut fields, "In", int(input as i64));
    set(
        &mut fields,
        "Weights",
        Value::List(lin.w.iter().map(|r| value_from_vec(r)).collect()),
    );
    set(&mut fields, "Biases", value_from_vec(&lin.b));
    make_net(
        &fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect::<Vec<_>>(),
    )
}

// ── Convolution / pooling parameters ────────────────────────────────────────

#[derive(Clone)]
struct Conv {
    kernels: usize,
    kernel: Vec<usize>,
    stride: Vec<usize>,
    pad: Vec<usize>,
    /// `w[k][channel * volume + j]`.
    w: Vec<Vec<f64>>,
    b: Vec<f64>,
    channels: usize,
}

#[derive(Clone)]
struct Pool {
    size: Vec<usize>,
    stride: Vec<usize>,
    max: bool,
}

fn conv_input_dims(kernel: &[usize], shape: &[usize]) -> Option<(usize, Vec<usize>)> {
    let r = kernel.len();
    if shape.len() == r {
        Some((1, shape.to_vec()))
    } else if shape.len() == r + 1 {
        Some((shape[0], shape[1..].to_vec()))
    } else {
        None
    }
}

fn spatial_out(size: usize, k: usize, pad: usize, stride: usize) -> usize {
    let eff = size + 2 * pad;
    if eff < k { 0 } else { (eff - k) / stride + 1 }
}

fn read_conv(fields: &HashMap<String, Value>) -> Option<Conv> {
    let kernels = get_i64(fields, "Kernels")? as usize;
    let kernel = usize_list(fields.get("KernelShape")?)?;
    let stride = fields
        .get("Stride")
        .and_then(usize_list)
        .unwrap_or_else(|| vec![1; kernel.len()]);
    let pad = fields
        .get("Padding")
        .and_then(usize_list)
        .unwrap_or_else(|| vec![0; kernel.len()]);
    if stride.len() != kernel.len() || pad.len() != kernel.len() {
        return None;
    }
    let vol = product(&kernel);
    let w_val = as_list(fields.get("Weights")?)?;
    if w_val.len() != kernels {
        return None;
    }
    let mut w = Vec::with_capacity(kernels);
    let mut channels = 0usize;
    for row in w_val {
        let row = as_list(row)?;
        if vol == 0 || row.len() % vol != 0 {
            return None;
        }
        channels = row.len() / vol;
        w.push(row.iter().map(super::to_f64).collect::<Option<Vec<_>>>()?);
    }
    let b = as_list(fields.get("Biases")?)?
        .iter()
        .map(super::to_f64)
        .collect::<Option<Vec<_>>>()?;
    if b.len() != kernels || channels == 0 {
        return None;
    }
    Some(Conv {
        kernels,
        kernel,
        stride,
        pad,
        w,
        b,
        channels,
    })
}

fn init_conv(kernels: usize, kernel: Vec<usize>, channels: usize, seed: u64) -> Conv {
    let vol = product(&kernel);
    let fan_in = (channels * vol).max(1);
    super::random::seed_rng(seed);
    super::random::with_rng(|rng| {
        let scale = 1.0 / (fan_in as f64).sqrt();
        let mut w = vec![vec![0.0f64; fan_in]; kernels];
        for row in w.iter_mut() {
            for c in row.iter_mut() {
                *c = (rng.f64() * 2.0 - 1.0) * scale;
            }
        }
        let rank = kernel.len();
        Conv {
            kernels,
            kernel,
            stride: vec![1; rank],
            pad: vec![0; rank],
            w,
            b: vec![0.0; kernels],
            channels,
        }
    })
}

fn conv_to_fields(layer: &Value, conv: &Conv) -> Value {
    let mut fields: Vec<(String, Value)> = match layer {
        Value::Object { fields, .. } => {
            fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect()
        }
        _ => Vec::new(),
    };
    let set = |fields: &mut Vec<(String, Value)>, k: &str, v: Value| {
        if let Some(slot) = fields.iter_mut().find(|(name, _)| name == k) {
            slot.1 = v;
        } else {
            fields.push((k.to_string(), v));
        }
    };
    set(&mut fields, "Type", Value::Str("Convolution".to_string()));
    set(&mut fields, "Kernels", int(conv.kernels as i64));
    set(
        &mut fields,
        "KernelShape",
        Value::List(conv.kernel.iter().map(|&x| int(x as i64)).collect()),
    );
    set(
        &mut fields,
        "Stride",
        Value::List(conv.stride.iter().map(|&x| int(x as i64)).collect()),
    );
    set(
        &mut fields,
        "Padding",
        Value::List(conv.pad.iter().map(|&x| int(x as i64)).collect()),
    );
    set(
        &mut fields,
        "Weights",
        Value::List(conv.w.iter().map(|r| value_from_vec(r)).collect()),
    );
    set(&mut fields, "Biases", value_from_vec(&conv.b));
    make_net(
        &fields
            .iter()
            .map(|(k, v)| (k.as_str(), v.clone()))
            .collect::<Vec<_>>(),
    )
}

fn materialize_conv(layer: &Value, shape: &[usize], index: usize) -> Option<Conv> {
    let fields = obj_fields(layer)?;
    if let Some(c) = read_conv(fields) {
        // Stored weights are reused only when the channel count matches.
        let want = conv_input_dims(&c.kernel, shape).map(|(ch, _)| ch);
        if want.is_none() || want == Some(c.channels) {
            return Some(c);
        }
    }
    let kernels = get_i64(fields, "Kernels")? as usize;
    let kernel = usize_list(fields.get("KernelShape")?)?;
    let (channels, _) = conv_input_dims(&kernel, shape).unwrap_or((1, Vec::new()));
    let stride = fields
        .get("Stride")
        .and_then(usize_list)
        .unwrap_or_else(|| vec![1; kernel.len()]);
    let pad = fields
        .get("Padding")
        .and_then(usize_list)
        .unwrap_or_else(|| vec![0; kernel.len()]);
    let mut conv = init_conv(
        kernels,
        kernel,
        channels,
        layer_seed(index, kernels, channels),
    );
    conv.stride = stride;
    conv.pad = pad;
    Some(conv)
}

fn read_pool(fields: &HashMap<String, Value>) -> Option<Pool> {
    let size = usize_list(fields.get("Size")?)?;
    let stride = fields
        .get("Stride")
        .and_then(usize_list)
        .unwrap_or_else(|| size.clone());
    if stride.len() != size.len() {
        return None;
    }
    let max = match fields.get("Function")? {
        Value::Str(s) => s == "Max",
        _ => true,
    };
    Some(Pool { size, stride, max })
}

// ── Activations ─────────────────────────────────────────────────────────────

/// Name of a function value, when it has one (for analytic derivatives).
fn fn_name(f: &Value) -> Option<String> {
    match f {
        Value::Builtin(n, _) => Some(n.clone()),
        Value::Symbol(s) => Some(s.clone()),
        Value::Function(fd) => Some(fd.name.clone()),
        _ => None,
    }
}

fn act_forward(f: &Value, x: f64, env: &Env) -> f64 {
    match fn_name(f).as_deref() {
        Some("Identity") | None => x,
        Some("Tanh") => x.tanh(),
        Some("LogisticSigmoid" | "Sigmoid") => 1.0 / (1.0 + (-x).exp()),
        Some("Ramp" | "ReLU") => x.max(0.0),
        Some("Sin") => x.sin(),
        Some("Cos") => x.cos(),
        Some("Exp") => x.exp(),
        Some("ArcTan") => x.atan(),
        Some("Sqrt") => x.max(0.0).sqrt(),
        _ => match crate::eval::apply_function(f, &[super::real(x)], env) {
            Ok(v) => super::to_f64(&v).unwrap_or(x),
            Err(_) => x,
        },
    }
}

fn act_deriv(f: &Value, x: f64, y: f64, env: &Env) -> f64 {
    match fn_name(f).as_deref() {
        Some("Identity") | None => 1.0,
        // Use the already-computed activation when it is cheaper/safer.
        Some("Tanh") => 1.0 - y * y,
        Some("LogisticSigmoid" | "Sigmoid") => y * (1.0 - y),
        Some("Ramp" | "ReLU") => {
            if x > 0.0 {
                1.0
            } else {
                0.0
            }
        }
        Some("Sin") => x.cos(),
        Some("Cos") => -x.sin(),
        Some("Exp") => y,
        Some("ArcTan") => 1.0 / (1.0 + x * x),
        Some("Sqrt") if x > 0.0 => 0.5 / x.sqrt(),
        _ => {
            let h = 1e-6 * (1.0 + x.abs());
            let fp = act_forward(f, x + h, env);
            let fm = act_forward(f, x - h, env);
            (fp - fm) / (2.0 * h)
        }
    }
}

// ── Layer constructors ──────────────────────────────────────────────────────

/// `LinearLayer[n]` / `LinearLayer[{n}]` / `LinearLayer[{nin, nout}]`.
fn builtin_linear_layer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("LinearLayer", args));
    }
    let (input, out) = match &args[0] {
        Value::Integer(n) if *n > 0 => (None, n.to_i64().unwrap()),
        Value::List(items) => match items.as_slice() {
            [Value::Integer(n)] if *n > 0 => (None, n.to_i64().unwrap()),
            [Value::Integer(a), Value::Integer(b)] if *a > 0 && *b > 0 => {
                (Some(a.to_i64().unwrap()), b.to_i64().unwrap())
            }
            _ => return Ok(unevaluated("LinearLayer", args)),
        },
        _ => return Ok(unevaluated("LinearLayer", args)),
    };
    let in_val = match input {
        Some(n) => int(n),
        None => Value::Null,
    };
    Ok(make_net(&[
        ("Type", Value::Str("Linear".to_string())),
        ("In", in_val),
        ("Out", int(out)),
    ]))
}

/// `ElementwiseLayer[f]`.
fn builtin_elementwise_layer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("ElementwiseLayer", args));
    }
    Ok(make_net(&[
        ("Type", Value::Str("Elementwise".to_string())),
        ("Activation", args[0].clone()),
    ]))
}

/// `ThreadingLayer[f]`.
fn builtin_threading_layer(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("ThreadingLayer", args));
    }
    Ok(make_net(&[
        ("Type", Value::Str("Threading".to_string())),
        ("Function", args[0].clone()),
    ]))
}

/// `SoftmaxLayer[]`.
fn builtin_softmax_layer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() > 1 {
        return Ok(unevaluated("SoftmaxLayer", args));
    }
    Ok(make_net(&[("Type", Value::Str("Softmax".to_string()))]))
}

/// `FlattenLayer[]`.
fn builtin_flatten_layer(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return Ok(unevaluated("FlattenLayer", args));
    }
    Ok(make_net(&[("Type", Value::Str("Flatten".to_string()))]))
}

/// `ReshapeLayer[{dims}]` / `ReshapeLayer[d]`.
fn builtin_reshape_layer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("ReshapeLayer", args));
    }
    let dims = match usize_list(&args[0]) {
        Some(d) => d,
        None => return Ok(unevaluated("ReshapeLayer", args)),
    };
    Ok(make_net(&[
        ("Type", Value::Str("Reshape".to_string())),
        (
            "Dimensions",
            Value::List(dims.iter().map(|&d| int(d as i64)).collect()),
        ),
    ]))
}

/// `DropoutLayer[]` / `DropoutLayer[p]` (inference = identity).
fn builtin_dropout_layer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() > 1 {
        return Ok(unevaluated("DropoutLayer", args));
    }
    let ratio = args.first().and_then(super::to_f64).unwrap_or(0.5);
    Ok(make_net(&[
        ("Type", Value::Str("Dropout".to_string())),
        ("Ratio", super::real(ratio)),
    ]))
}

/// `BatchNormalizationLayer[]` (inference with default statistics).
fn builtin_batchnorm_layer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() > 1 {
        return Ok(unevaluated("BatchNormalizationLayer", args));
    }
    Ok(make_net(&[
        ("Type", Value::Str("BatchNorm".to_string())),
        ("Mean", super::real(0.0)),
        ("Variance", super::real(1.0)),
        ("Epsilon", super::real(BN_EPSILON)),
    ]))
}

/// `ConvolutionLayer[k]` / `ConvolutionLayer[n, k]` / `...[n, {kh, kw}]`
/// with optional `"Stride"`/`"PaddingSize"` rules.
fn builtin_convolution_layer(args: &[Value]) -> Result<Value, EvalError> {
    let (spec, opts) = split_spec_opts(args);
    let (kernels, kernel) = match spec.as_slice() {
        [Value::Integer(k)] if *k > 0 => (1usize, vec![k.to_i64().unwrap() as usize]),
        [a, Value::Integer(k)] if matches!(a, Value::Integer(n) if *n > 0) => {
            let n = match a {
                Value::Integer(n) => n.to_i64().unwrap() as usize,
                _ => unreachable!(),
            };
            (n, vec![k.to_i64().unwrap() as usize])
        }
        [Value::List(_)] => match usize_list(&spec[0]) {
            Some(d) => (1usize, d),
            None => return Ok(unevaluated("ConvolutionLayer", args)),
        },
        [a, Value::List(_)] if matches!(a, Value::Integer(n) if *n > 0) => {
            let n = match a {
                Value::Integer(n) => n.to_i64().unwrap() as usize,
                _ => unreachable!(),
            };
            match usize_list(&spec[1]) {
                Some(d) => (n, d),
                None => return Ok(unevaluated("ConvolutionLayer", args)),
            }
        }
        _ => return Ok(unevaluated("ConvolutionLayer", args)),
    };
    let stride = opts
        .get("Stride")
        .and_then(usize_list)
        .unwrap_or_else(|| vec![1; kernel.len()]);
    let pad = opts
        .get("PaddingSize")
        .or_else(|| opts.get("Padding"))
        .and_then(usize_list)
        .unwrap_or_else(|| vec![0; kernel.len()]);
    if stride.len() != kernel.len() || pad.len() != kernel.len() {
        return Ok(unevaluated("ConvolutionLayer", args));
    }
    Ok(make_net(&[
        ("Type", Value::Str("Convolution".to_string())),
        ("Kernels", int(kernels as i64)),
        (
            "KernelShape",
            Value::List(kernel.iter().map(|&k| int(k as i64)).collect()),
        ),
        (
            "Stride",
            Value::List(stride.iter().map(|&s| int(s as i64)).collect()),
        ),
        (
            "Padding",
            Value::List(pad.iter().map(|&p| int(p as i64)).collect()),
        ),
    ]))
}

/// `PoolingLayer[k]` / `PoolingLayer[k, "Average"]` / `PoolingLayer[{h, w}]`.
fn builtin_pooling_layer(args: &[Value]) -> Result<Value, EvalError> {
    let (spec, opts) = split_spec_opts(args);
    let (size, func) = match spec.as_slice() {
        [Value::Integer(k)] if *k > 0 => (vec![k.to_i64().unwrap() as usize], "Max"),
        [Value::List(_)] => match usize_list(&spec[0]) {
            Some(d) => (d, "Max"),
            None => return Ok(unevaluated("PoolingLayer", args)),
        },
        [a, Value::Str(s)] if matches!(a, Value::Integer(k) if *k > 0) => {
            let k = match a {
                Value::Integer(k) => k.to_i64().unwrap() as usize,
                _ => unreachable!(),
            };
            (vec![k], s.as_str())
        }
        [Value::List(_), Value::Str(s)] => match usize_list(&spec[0]) {
            Some(d) => (d, s.as_str()),
            None => return Ok(unevaluated("PoolingLayer", args)),
        },
        _ => return Ok(unevaluated("PoolingLayer", args)),
    };
    let func = match func {
        "Max" | "Average" => func,
        _ => return Ok(unevaluated("PoolingLayer", args)),
    };
    let stride = opts
        .get("Stride")
        .and_then(usize_list)
        .unwrap_or_else(|| size.clone());
    if stride.len() != size.len() {
        return Ok(unevaluated("PoolingLayer", args));
    }
    Ok(make_net(&[
        ("Type", Value::Str("Pooling".to_string())),
        (
            "Size",
            Value::List(size.iter().map(|&s| int(s as i64)).collect()),
        ),
        (
            "Stride",
            Value::List(stride.iter().map(|&s| int(s as i64)).collect()),
        ),
        ("Function", Value::Str(func.to_string())),
    ]))
}

fn builtin_mse_layer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() > 1 {
        return Ok(unevaluated("MeanSquaredLossLayer", args));
    }
    Ok(make_net(&[(
        "Type",
        Value::Str("MeanSquaredLoss".to_string()),
    )]))
}

fn builtin_ce_layer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() > 1 {
        return Ok(unevaluated("CrossEntropyLossLayer", args));
    }
    Ok(make_net(&[(
        "Type",
        Value::Str("CrossEntropyLoss".to_string()),
    )]))
}

/// `LossFunction["MeanSquaredLossLayer"]` and friends.
fn builtin_loss_function(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("LossFunction", args));
    }
    let kind = match &args[0] {
        Value::Str(s) | Value::Symbol(s) => s.as_str(),
        _ => return Ok(unevaluated("LossFunction", args)),
    };
    let ty = match kind {
        "MeanSquaredLossLayer" | "MeanSquaredLoss" | "MSE" => "MeanSquaredLoss",
        "CrossEntropyLossLayer" | "CrossEntropyLoss" | "CrossEntropy" => "CrossEntropyLoss",
        _ => return Ok(unevaluated("LossFunction", args)),
    };
    Ok(make_net(&[("Type", Value::Str(ty.to_string()))]))
}

/// `NetChain[{layer, ...}]`.
fn builtin_net_chain(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("NetChain", args));
    }
    let layers = match as_list(&args[0]) {
        Some(l) => l.clone(),
        None => return Ok(unevaluated("NetChain", args)),
    };
    if layers.is_empty() || !layers.iter().all(|l| obj_fields(l).is_some()) {
        return Ok(unevaluated("NetChain", args));
    }
    Ok(make_net(&[
        ("Type", Value::Str("Chain".to_string())),
        ("Layers", Value::List(layers)),
    ]))
}

/// `NetGraph[{layer, ...}]` / `NetGraph[{layer, ...}, {i -> j, ...}]`.
fn builtin_net_graph(args: &[Value]) -> Result<Value, EvalError> {
    let (nodes, edges) = match args {
        [nodes] => match as_list(nodes) {
            Some(n) if !n.is_empty() => (n.clone(), default_chain_edges(n.len())),
            _ => return Ok(unevaluated("NetGraph", args)),
        },
        [nodes, edges] => match (as_list(nodes), parse_edges(edges)) {
            (Some(n), Some(e)) if !n.is_empty() => (n.clone(), e),
            _ => return Ok(unevaluated("NetGraph", args)),
        },
        _ => return Ok(unevaluated("NetGraph", args)),
    };
    if !nodes.iter().all(|l| obj_fields(l).is_some()) {
        return Ok(unevaluated("NetGraph", args));
    }
    if edges
        .iter()
        .any(|&(a, b)| a == 0 || b == 0 || a > nodes.len() || b > nodes.len())
    {
        return Ok(unevaluated("NetGraph", args));
    }
    Ok(make_net(&[
        ("Type", Value::Str("Graph".to_string())),
        ("Nodes", Value::List(nodes)),
        (
            "Edges",
            Value::List(
                edges
                    .iter()
                    .map(|&(a, b)| Value::List(vec![int(a as i64), int(b as i64)]))
                    .collect(),
            ),
        ),
    ]))
}

fn default_chain_edges(n: usize) -> Vec<(usize, usize)> {
    (1..n).map(|i| (i, i + 1)).collect()
}

fn parse_edges(v: &Value) -> Option<Vec<(usize, usize)>> {
    let dv = deref(v);
    let items = as_list(&dv)?;
    let mut edges = Vec::with_capacity(items.len());
    for it in items {
        let it = deref(it);
        let pair = match &it {
            Value::Rule { lhs, rhs, .. } => vec![(**lhs).clone(), (**rhs).clone()],
            Value::List(p) if p.len() == 2 => p.clone(),
            _ => return None,
        };
        let a = super::to_f64(&deref(&pair[0]))? as usize;
        let b = super::to_f64(&deref(&pair[1]))? as usize;
        edges.push((a, b));
    }
    Some(edges)
}

/// `NetExtract[net, i]` — the i-th layer (1-based) of a container.
fn builtin_net_extract(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("NetExtract", args));
    }
    let idx = match super::to_f64(&deref(&args[1])) {
        Some(x) if x >= 1.0 => x as usize,
        _ => return Ok(unevaluated("NetExtract", args)),
    };
    let layers = match net_type(&args[0]) {
        Some("Chain") | Some("Graph") => match forward_layers_of(&args[0]) {
            Some(l) => l,
            None => return Ok(unevaluated("NetExtract", args)),
        },
        _ => vec![args[0].clone()],
    };
    match layers.get(idx - 1) {
        Some(l) => Ok(l.clone()),
        None => Ok(unevaluated("NetExtract", args)),
    }
}

fn split_spec_opts(args: &[Value]) -> (Vec<Value>, HashMap<String, Value>) {
    let mut spec = Vec::new();
    let mut opts = HashMap::new();
    for a in args {
        match a {
            Value::Rule { lhs, rhs, .. } => {
                if let Some(k) = rule_key(lhs) {
                    opts.insert(k, (**rhs).clone());
                }
            }
            Value::Assoc(m) => {
                for (k, v) in m {
                    opts.insert(k.clone(), v.clone());
                }
            }
            other => spec.push(other.clone()),
        }
    }
    (spec, opts)
}

fn rule_key(v: &Value) -> Option<String> {
    match deref(v) {
        Value::Str(s) | Value::Symbol(s) => Some(s),
        _ => None,
    }
}

// ── Forward evaluation ──────────────────────────────────────────────────────

fn softmax(xs: &[f64]) -> Vec<f64> {
    if xs.is_empty() {
        return Vec::new();
    }
    let max = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let mut exps: Vec<f64> = xs.iter().map(|x| (x - max).exp()).collect();
    let sum: f64 = exps.iter().sum();
    if sum > 0.0 {
        for e in exps.iter_mut() {
            *e /= sum;
        }
    }
    exps
}

/// Softmax applied along the last axis (whole tensor when rank ≤ 1).
fn softmax_t(x: &Tensor) -> Tensor {
    if x.rank() <= 1 {
        return Tensor::vector(softmax(&x.data));
    }
    let n = *x.shape.last().unwrap();
    if n == 0 {
        return x.clone();
    }
    let mut out = vec![0.0f64; x.data.len()];
    for (s, chunk) in out.chunks_mut(n).enumerate() {
        let src = &x.data[s * n..s * n + n];
        chunk.copy_from_slice(&softmax(src));
    }
    Tensor {
        shape: x.shape.clone(),
        data: out,
    }
}

fn conv_forward(c: &Conv, x: &Tensor) -> Option<Tensor> {
    let (channels, sp) = conv_input_dims(&c.kernel, &x.shape)?;
    if channels != c.channels {
        return None;
    }
    let r = c.kernel.len();
    let out_sp: Vec<usize> = (0..r)
        .map(|i| spatial_out(sp[i], c.kernel[i], c.pad[i], c.stride[i]))
        .collect();
    let vol = product(&c.kernel);
    let has_cdim = x.rank() > r;
    let has_kdim = !(c.kernels == 1 && x.rank() == r);
    let mut shape = Vec::new();
    if has_kdim {
        shape.push(c.kernels);
    }
    shape.extend_from_slice(&out_sp);
    let total = product(&out_sp);
    let mut out = vec![0.0f64; c.kernels * total];
    for oi in 0..total {
        let oc = decompose(oi, &out_sp);
        for k in 0..c.kernels {
            let mut acc = c.b[k];
            for ch in 0..channels {
                for j in 0..vol {
                    let jc = decompose(j, &c.kernel);
                    let mut full = Vec::with_capacity(r + 1);
                    if has_cdim {
                        full.push(ch);
                    }
                    let mut inb = true;
                    for d in 0..r {
                        let pos = oc[d] * c.stride[d] + jc[d];
                        if pos < c.pad[d] {
                            inb = false;
                            break;
                        }
                        let pos = pos - c.pad[d];
                        if pos >= sp[d] {
                            inb = false;
                            break;
                        }
                        full.push(pos);
                    }
                    if !inb {
                        continue;
                    }
                    acc += c.w[k][ch * vol + j] * x.data[lin_index(&full, &x.shape)];
                }
            }
            let idx = if has_kdim {
                let mut f = vec![k];
                f.extend_from_slice(&oc);
                lin_index(&f, &shape)
            } else {
                lin_index(&oc, &shape)
            };
            out[idx] = acc;
        }
    }
    Tensor::from_flat(shape, out)
}

fn pool_forward(p: &Pool, x: &Tensor) -> Option<Tensor> {
    let (channels, sp) = conv_input_dims(&p.size, &x.shape)?;
    let r = p.size.len();
    let out_sp: Vec<usize> = (0..r)
        .map(|i| spatial_out(sp[i], p.size[i], 0, p.stride[i]))
        .collect();
    let vol = product(&p.size);
    let has_cdim = x.rank() > r;
    let mut shape = Vec::new();
    if has_cdim {
        shape.push(channels);
    }
    shape.extend_from_slice(&out_sp);
    let total = product(&out_sp);
    let mut out = vec![0.0f64; channels * total];
    for oi in 0..total {
        let oc = decompose(oi, &out_sp);
        for ch in 0..channels {
            let mut best = f64::NEG_INFINITY;
            let mut sum = 0.0;
            for j in 0..vol {
                let jc = decompose(j, &p.size);
                let mut full = Vec::with_capacity(r + 1);
                if has_cdim {
                    full.push(ch);
                }
                for d in 0..r {
                    full.push(oc[d] * p.stride[d] + jc[d]);
                }
                let v = x.data[lin_index(&full, &x.shape)];
                best = best.max(v);
                sum += v;
            }
            let val = if p.max { best } else { sum / vol as f64 };
            let idx = if has_cdim {
                let mut f = vec![ch];
                f.extend_from_slice(&oc);
                lin_index(&f, &shape)
            } else {
                lin_index(&oc, &shape)
            };
            out[idx] = val;
        }
    }
    Tensor::from_flat(shape, out)
}

fn batchnorm_forward(fields: &HashMap<String, Value>, x: &Tensor) -> Tensor {
    let mean = get_f64(fields, "Mean").unwrap_or(0.0);
    let var = get_f64(fields, "Variance").unwrap_or(1.0).max(0.0);
    let eps = get_f64(fields, "Epsilon").unwrap_or(BN_EPSILON);
    let inv = 1.0 / (var + eps).sqrt();
    Tensor {
        shape: x.shape.clone(),
        data: x.data.iter().map(|&v| (v - mean) * inv).collect(),
    }
}

fn activation_of(fields: &HashMap<String, Value>, key: &str) -> Value {
    fields
        .get(key)
        .cloned()
        .unwrap_or_else(|| Value::Symbol("Identity".to_string()))
}

fn layer_forward_t(
    layer: &Value,
    x: &Tensor,
    index: usize,
    env: &Env,
) -> Result<Tensor, EvalError> {
    let fields =
        obj_fields(layer).ok_or_else(|| EvalError::Error("not a net layer".to_string()))?;
    match net_type(layer) {
        Some("Linear") => {
            let out = get_i64(fields, "Out").unwrap_or(x.numel() as i64) as usize;
            let lin = materialize_linear(layer, x.numel(), index)
                .ok_or_else(|| EvalError::Error("bad linear layer".to_string()))?;
            let mut y = lin.b.clone();
            for r in 0..out.min(y.len()) {
                for c in 0..x.numel().min(lin.w[r].len()) {
                    y[r] += lin.w[r][c] * x.data[c];
                }
            }
            Ok(Tensor::vector(y))
        }
        Some("Elementwise") | Some("Threading") => {
            let key = if net_type(layer) == Some("Threading") {
                "Function"
            } else {
                "Activation"
            };
            let f = activation_of(fields, key);
            Ok(Tensor {
                shape: x.shape.clone(),
                data: x.data.iter().map(|&v| act_forward(&f, v, env)).collect(),
            })
        }
        Some("Softmax") => Ok(softmax_t(x)),
        Some("Convolution") => {
            let conv = materialize_conv(layer, &x.shape, index)
                .ok_or_else(|| EvalError::Error("bad convolution layer".to_string()))?;
            conv_forward(&conv, x)
                .ok_or_else(|| EvalError::Error("bad conv input shape".to_string()))
        }
        Some("Pooling") => {
            let pool = read_pool(fields)
                .ok_or_else(|| EvalError::Error("bad pooling layer".to_string()))?;
            pool_forward(&pool, x)
                .ok_or_else(|| EvalError::Error("bad pool input shape".to_string()))
        }
        Some("BatchNorm") => Ok(batchnorm_forward(fields, x)),
        Some("Flatten") => Ok(Tensor::vector(x.data.clone())),
        Some("Reshape") => {
            let dims = fields
                .get("Dimensions")
                .and_then(usize_list)
                .ok_or_else(|| EvalError::Error("bad reshape layer".to_string()))?;
            x.reshape(&dims)
                .ok_or_else(|| EvalError::Error("reshape size mismatch".to_string()))
        }
        Some("Dropout") => Ok(x.clone()),
        _ => Ok(x.clone()),
    }
}

fn eval_layers(layers: &[Value], input: &Tensor, env: &Env) -> Result<Tensor, EvalError> {
    let mut act = input.clone();
    for (i, layer) in layers.iter().enumerate() {
        if is_loss(layer) {
            continue;
        }
        act = layer_forward_t(layer, &act, i, env)?;
    }
    Ok(act)
}

fn forward_layers_of(net: &Value) -> Option<Vec<Value>> {
    match net_type(net)? {
        "Chain" => Some(as_list(obj_fields(net)?.get("Layers")?)?.clone()),
        "Graph" => Some(graph_nodes(net)?.0),
        _ => Some(vec![net.clone()]),
    }
}

#[allow(clippy::type_complexity)]
fn graph_nodes(net: &Value) -> Option<(Vec<Value>, Vec<(usize, usize)>)> {
    let fields = obj_fields(net)?;
    if net_type(net) != Some("Graph") {
        return None;
    }
    let nodes = as_list(fields.get("Nodes")?)?.clone();
    let edges = parse_edges(fields.get("Edges")?)?;
    Some((nodes, edges))
}

/// Evaluate a graph by topological order; multi-input nodes concatenate.
fn eval_graph(net: &Value, input: &Tensor, env: &Env) -> Option<Tensor> {
    let (nodes, edges) = graph_nodes(net)?;
    let n = nodes.len();
    let mut incoming: Vec<Vec<usize>> = vec![Vec::new(); n];
    for &(a, b) in &edges {
        incoming[b - 1].push(a - 1);
    }
    // Kahn topological sort.
    let mut indeg: Vec<usize> = incoming.iter().map(Vec::len).collect();
    let mut order: Vec<usize> = (0..n).filter(|&i| indeg[i] == 0).collect();
    order.sort_unstable();
    let mut qi = 0;
    while qi < order.len() {
        let u = order[qi];
        qi += 1;
        for &(a, b) in &edges {
            if a - 1 == u {
                indeg[b - 1] -= 1;
                if indeg[b - 1] == 0 {
                    order.push(b - 1);
                }
            }
        }
    }
    if order.len() != n {
        return None;
    }
    let mut outputs: Vec<Option<Tensor>> = vec![None; n];
    for &node in &order {
        let x = if incoming[node].is_empty() {
            input.clone()
        } else if incoming[node].len() == 1 {
            outputs[incoming[node][0]].clone()?
        } else {
            let mut data = Vec::new();
            for &pred in &incoming[node] {
                data.extend_from_slice(&outputs[pred].as_ref()?.data);
            }
            Tensor::vector(data)
        };
        outputs[node] = Some(layer_forward_t(&nodes[node], &x, node, env).ok()?);
    }
    // Sink = node with no outgoing edge (prefer the highest index).
    let mut has_out = vec![false; n];
    for &(a, _) in &edges {
        has_out[a - 1] = true;
    }
    let sink = (0..n).rev().find(|&i| !has_out[i])?;
    outputs[sink].clone()
}

fn loss_scalar(loss: &Value, pred: &Tensor, target: &Tensor) -> Result<f64, EvalError> {
    let n = pred.numel().min(target.numel());
    if n == 0 {
        return Ok(0.0);
    }
    let v = match net_type(loss) {
        Some("MeanSquaredLoss") => {
            let s: f64 = (0..n)
                .map(|i| (pred.data[i] - target.data[i]).powi(2))
                .sum();
            s / n as f64
        }
        Some("CrossEntropyLoss") => {
            let mut s = 0.0;
            for i in 0..n {
                let p = pred.data[i].clamp(1e-300, f64::INFINITY);
                s -= target.data[i] * p.ln();
            }
            s / n as f64
        }
        _ => 0.0,
    };
    Ok(v)
}

/// Evaluate a net: `net[input]`, stripping a trailing loss layer. When the last
/// layer *is* a loss layer and the input is `{pred, target}`, the scalar loss
/// is returned instead.
fn net_forward_value(net: &Value, input: &Value, env: &Env) -> Result<Value, EvalError> {
    let fail = || unevaluated("NeuralNet", &[net.clone(), input.clone()]);
    if net_type(net).is_none() {
        return Ok(fail());
    }

    // Loss handling for chains / bare layers.
    if net_type(net) != Some("Graph")
        && let Some(layers) = forward_layers_of(net)
        && let Some(last) = layers.last()
        && is_loss(last)
    {
        if let Value::List(pair) = input
            && pair.len() == 2
            && pair.iter().all(|p| matches!(p, Value::List(_)))
        {
            let xt = match tensor_from_value(&pair[0]) {
                Some(t) => t,
                None => return Ok(fail()),
            };
            let pred = if layers.len() == 1 {
                xt
            } else {
                match eval_layers(&layers[..layers.len() - 1], &xt, env) {
                    Ok(p) => p,
                    Err(_) => return Ok(fail()),
                }
            };
            let target = match tensor_from_value(&pair[1]) {
                Some(t) => t,
                None => return Ok(fail()),
            };
            return Ok(super::real(loss_scalar(last, &pred, &target)?));
        }
        let xt = match tensor_from_value(input) {
            Some(t) => t,
            None => return Ok(fail()),
        };
        return match eval_layers(&layers[..layers.len() - 1], &xt, env) {
            Ok(p) => Ok(p.to_value()),
            Err(_) => Ok(fail()),
        };
    }

    let xt = match tensor_from_value(input) {
        Some(t) => t,
        None => return Ok(fail()),
    };
    let out = match net_type(net) {
        Some("Chain") => match as_list(obj_fields(net).unwrap().get("Layers").unwrap()) {
            Some(layers) => eval_layers(layers, &xt, env),
            None => return Ok(fail()),
        },
        Some("Graph") => match eval_graph(net, &xt, env) {
            Some(o) => Ok(o),
            None => return Ok(fail()),
        },
        _ => layer_forward_t(net, &xt, 0, env),
    };
    match out {
        Ok(o) => Ok(o.to_value()),
        Err(_) => Ok(fail()),
    }
}

/// The `net[...]` method: forward, or field access for a string argument.
fn builtin_net_call(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(unevaluated("NeuralNet", args));
    }
    let net = &args[0];
    if obj_fields(net).is_none() {
        return Ok(unevaluated("NeuralNet", args));
    }
    if args.len() == 2 {
        if let Value::Str(key) = &args[1]
            && let Some(v) = obj_fields(net).and_then(|f| f.get(key))
        {
            return Ok(v.clone());
        }
        return net_forward_value(net, &args[1], env);
    }
    // Multiple call arguments: `{a, b}`.
    net_forward_value(net, &Value::List(args[1..].to_vec()), env)
}

/// `NetEvaluate[net, input]`.
fn builtin_net_evaluate(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("NetEvaluate", args));
    }
    net_forward_value(&args[0], &args[1], env)
}

// ── Initialization ──────────────────────────────────────────────────────────

/// Output shape of a layer given its input shape (for initialization).
fn layer_output_shape(layer: &Value, in_shape: &[usize]) -> Option<Vec<usize>> {
    let fields = obj_fields(layer)?;
    match net_type(layer)? {
        "Linear" => Some(vec![get_i64(fields, "Out")? as usize]),
        "Elementwise" | "Softmax" | "Dropout" | "BatchNorm" | "Threading" => {
            Some(in_shape.to_vec())
        }
        "Flatten" => Some(vec![product(in_shape)]),
        "Reshape" => Some(
            fields
                .get("Dimensions")
                .and_then(usize_list)
                .unwrap_or_default(),
        ),
        "Convolution" => {
            let kernel = usize_list(fields.get("KernelShape")?)?;
            let stride = fields
                .get("Stride")
                .and_then(usize_list)
                .unwrap_or_else(|| vec![1; kernel.len()]);
            let pad = fields
                .get("Padding")
                .and_then(usize_list)
                .unwrap_or_else(|| vec![0; kernel.len()]);
            let (_, sp) = conv_input_dims(&kernel, in_shape)?;
            let r = kernel.len();
            let out_sp: Vec<usize> = (0..r)
                .map(|i| spatial_out(sp[i], kernel[i], pad[i], stride[i]))
                .collect();
            let kernels = get_i64(fields, "Kernels")? as usize;
            let mut shape = Vec::new();
            if !(kernels == 1 && in_shape.len() == r) {
                shape.push(kernels);
            }
            shape.extend_from_slice(&out_sp);
            Some(shape)
        }
        "Pooling" => {
            let size = usize_list(fields.get("Size")?)?;
            let stride = fields
                .get("Stride")
                .and_then(usize_list)
                .unwrap_or_else(|| size.clone());
            let (channels, sp) = conv_input_dims(&size, in_shape)?;
            let r = size.len();
            let out_sp: Vec<usize> = (0..r)
                .map(|i| spatial_out(sp[i], size[i], 0, stride[i]))
                .collect();
            let mut shape = Vec::new();
            if in_shape.len() > r {
                shape.push(channels);
            }
            shape.extend_from_slice(&out_sp);
            Some(shape)
        }
        _ => Some(in_shape.to_vec()),
    }
}

/// Initialize one layer, returning it with concrete parameters (if any).
fn ensure_init(layer: &Value, in_shape: Option<&[usize]>, index: usize) -> Value {
    let fields = match obj_fields(layer) {
        Some(f) => f,
        None => return layer.clone(),
    };
    match net_type(layer) {
        Some("Linear") => {
            let out = match get_i64(fields, "Out") {
                Some(o) if o > 0 => o as usize,
                _ => return layer.clone(),
            };
            let input = if let Some(shape) = in_shape {
                product(shape).max(1)
            } else if let Some(i) = get_i64(fields, "In").filter(|&i| i > 0) {
                i as usize
            } else {
                1
            };
            let lin = init_weights(input, out, layer_seed(index, out, input));
            lin_to_fields(layer, &lin)
        }
        Some("Convolution") => {
            let kernels = get_i64(fields, "Kernels").unwrap_or(1).max(1) as usize;
            let kernel = match fields.get("KernelShape").and_then(usize_list) {
                Some(k) => k,
                None => return layer.clone(),
            };
            let channels = in_shape
                .and_then(|s| conv_input_dims(&kernel, s).map(|(c, _)| c))
                .unwrap_or(1)
                .max(1);
            let mut conv = init_conv(
                kernels,
                kernel,
                channels,
                layer_seed(index, kernels, channels),
            );
            if let Some(s) = fields.get("Stride").and_then(usize_list) {
                conv.stride = s;
            }
            if let Some(p) = fields.get("Padding").and_then(usize_list) {
                conv.pad = p;
            }
            conv_to_fields(layer, &conv)
        }
        Some("BatchNorm") => {
            let mut fields_vec: Vec<(String, Value)> =
                fields.iter().map(|(k, v)| (k.clone(), v.clone())).collect();
            let set = |fv: &mut Vec<(String, Value)>, k: &str, v: Value| {
                if let Some(slot) = fv.iter_mut().find(|(name, _)| name == k) {
                    slot.1 = v;
                } else {
                    fv.push((k.to_string(), v));
                }
            };
            if get_f64(fields, "Mean").is_none() {
                set(&mut fields_vec, "Mean", super::real(0.0));
            }
            if get_f64(fields, "Variance").is_none() {
                set(&mut fields_vec, "Variance", super::real(1.0));
            }
            if get_f64(fields, "Epsilon").is_none() {
                set(&mut fields_vec, "Epsilon", super::real(BN_EPSILON));
            }
            make_net(
                &fields_vec
                    .iter()
                    .map(|(k, v)| (k.as_str(), v.clone()))
                    .collect::<Vec<_>>(),
            )
        }
        _ => layer.clone(),
    }
}

/// `NetInitialize[net]` / `NetInitialize[net, sampleInput]`.
fn builtin_net_initialize(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("NetInitialize", args));
    }
    let net = &args[0];
    super::random::seed_rng(NET_INIT_SEED);
    let sample_shape = args.get(1).and_then(tensor_from_value).map(|t| t.shape);

    match net_type(net) {
        Some("Chain") => {
            let layers = match as_list(obj_fields(net).unwrap().get("Layers").unwrap()) {
                Some(l) => l.clone(),
                None => return Ok(unevaluated("NetInitialize", args)),
            };
            let mut cur = sample_shape.clone();
            let new_layers: Vec<Value> = layers
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let v = ensure_init(l, cur.as_deref(), i);
                    cur = cur.as_deref().and_then(|s| layer_output_shape(&v, s));
                    v
                })
                .collect();
            Ok(make_net(&[
                ("Type", Value::Str("Chain".to_string())),
                ("Layers", Value::List(new_layers)),
            ]))
        }
        Some("Graph") => {
            let nodes = match as_list(obj_fields(net).unwrap().get("Nodes").unwrap()) {
                Some(n) => n.clone(),
                None => return Ok(unevaluated("NetInitialize", args)),
            };
            let mut cur = sample_shape.clone();
            let new_nodes: Vec<Value> = nodes
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let v = ensure_init(l, cur.as_deref(), i);
                    cur = cur.as_deref().and_then(|s| layer_output_shape(&v, s));
                    v
                })
                .collect();
            Ok(make_net(&[
                ("Type", Value::Str("Graph".to_string())),
                ("Nodes", Value::List(new_nodes)),
                (
                    "Edges",
                    obj_fields(net).unwrap().get("Edges").unwrap().clone(),
                ),
            ]))
        }
        Some(_) => Ok(ensure_init(net, sample_shape.as_deref(), 0)),
        None => Ok(unevaluated("NetInitialize", args)),
    }
}

// ── NetTrain ────────────────────────────────────────────────────────────────

/// Parse training data into `(input, target)` value pairs.
fn parse_training_data(data: &Value) -> Option<Vec<(Value, Value)>> {
    let items = as_list(data)?;
    if items.is_empty() {
        return None;
    }
    // List of rules `input -> target`.
    if items.iter().all(|it| matches!(it, Value::Rule { .. })) {
        let mut out = Vec::with_capacity(items.len());
        for it in items {
            if let Value::Rule { lhs, rhs, .. } = it {
                out.push(((**lhs).clone(), (**rhs).clone()));
            }
        }
        return Some(out);
    }
    // Parallel lists `{inputs, targets}`.
    if items.len() == 2
        && let (Some(inputs), Some(targets)) = (as_list(&items[0]), as_list(&items[1]))
        && inputs.len() == targets.len()
    {
        let mut out = Vec::with_capacity(inputs.len());
        for (i, t) in inputs.iter().zip(targets.iter()) {
            out.push((i.clone(), t.clone()));
        }
        return Some(out);
    }
    // List of `{input, target}` pairs.
    let mut out = Vec::with_capacity(items.len());
    for it in items {
        let pair = as_list(it)?;
        if pair.len() != 2 {
            return None;
        }
        out.push((pair[0].clone(), pair[1].clone()));
    }
    Some(out)
}

fn parse_options(args: &[Value], rounds: &mut usize, lr: &mut f64) {
    for opt in args {
        match opt {
            Value::Rule { lhs, rhs, .. } => apply_option(lhs, rhs, rounds, lr),
            Value::List(items) => parse_options(items, rounds, lr),
            Value::Assoc(m) => {
                for (k, v) in m {
                    apply_option(&Value::Str(k.clone()), v, rounds, lr);
                }
            }
            _ => {}
        }
    }
}

fn apply_option(lhs: &Value, rhs: &Value, rounds: &mut usize, lr: &mut f64) {
    let lhs = deref(lhs);
    let rhs = deref(rhs);
    let key = match &lhs {
        Value::Symbol(s) | Value::Str(s) => s.as_str(),
        _ => return,
    };
    match key {
        "MaxTrainingRounds" | "TrainingRounds" => {
            if let Some(n) = super::to_f64(&rhs) {
                *rounds = n.max(0.0) as usize;
            }
        }
        "LearningRate" => {
            if let Some(v) = super::to_f64(&rhs) {
                *lr = v;
            }
        }
        _ => {}
    }
}

/// Accumulated weight/bias gradients for one parameterized layer.
#[derive(Clone)]
enum PGrad {
    Lin(Vec<Vec<f64>>, Vec<f64>),
    Conv(Vec<Vec<f64>>, Vec<f64>),
}

fn zero_pgrad(layer: &Value) -> Option<PGrad> {
    let fields = obj_fields(layer)?;
    if is_linear(layer) {
        let w = as_list(fields.get("Weights")?)?;
        let mut gw = Vec::with_capacity(w.len());
        for row in w {
            gw.push(vec![0.0f64; as_list(row)?.len()]);
        }
        return Some(PGrad::Lin(gw, vec![0.0; w.len()]));
    }
    match net_type(layer)? {
        "Linear" => {
            let w = as_list(fields.get("Weights")?)?;
            let mut gw = Vec::with_capacity(w.len());
            for row in w {
                gw.push(vec![0.0f64; as_list(row)?.len()]);
            }
            Some(PGrad::Lin(gw, vec![0.0; w.len()]))
        }
        "Convolution" => {
            let w = as_list(fields.get("Weights")?)?;
            let mut gw = Vec::with_capacity(w.len());
            for row in w {
                gw.push(vec![0.0f64; as_list(row)?.len()]);
            }
            Some(PGrad::Conv(gw, vec![0.0; w.len()]))
        }
        _ => None,
    }
}

fn acc_lin(slot: &mut Option<PGrad>, w: &[Vec<f64>], b: &[f64]) {
    match slot {
        Some(PGrad::Lin(gw, gb)) => {
            for (gr, r) in gw.iter_mut().zip(w.iter()) {
                for (g, v) in gr.iter_mut().zip(r.iter()) {
                    *g += v;
                }
            }
            for (g, v) in gb.iter_mut().zip(b.iter()) {
                *g += v;
            }
        }
        _ => *slot = Some(PGrad::Lin(w.to_vec(), b.to_vec())),
    }
}

fn acc_conv(slot: &mut Option<PGrad>, w: &[Vec<f64>], b: &[f64]) {
    match slot {
        Some(PGrad::Conv(gw, gb)) => {
            for (gr, r) in gw.iter_mut().zip(w.iter()) {
                for (g, v) in gr.iter_mut().zip(r.iter()) {
                    *g += v;
                }
            }
            for (g, v) in gb.iter_mut().zip(b.iter()) {
                *g += v;
            }
        }
        _ => *slot = Some(PGrad::Conv(w.to_vec(), b.to_vec())),
    }
}

fn conv_backward(c: &Conv, x: &Tensor, gout: &Tensor) -> Option<(Tensor, Vec<Vec<f64>>, Vec<f64>)> {
    let (channels, sp) = conv_input_dims(&c.kernel, &x.shape)?;
    let r = c.kernel.len();
    let out_sp: Vec<usize> = (0..r)
        .map(|i| spatial_out(sp[i], c.kernel[i], c.pad[i], c.stride[i]))
        .collect();
    let vol = product(&c.kernel);
    let has_cdim = x.rank() > r;
    let has_kdim = !(c.kernels == 1 && x.rank() == r);
    let total = product(&out_sp);
    let mut gin = vec![0.0f64; x.data.len()];
    let mut gw = vec![vec![0.0f64; channels * vol]; c.kernels];
    let mut gb = vec![0.0f64; c.kernels];
    for oi in 0..total {
        let oc = decompose(oi, &out_sp);
        for k in 0..c.kernels {
            let g = if has_kdim {
                let mut f = vec![k];
                f.extend_from_slice(&oc);
                gout.data[lin_index(&f, &gout.shape)]
            } else {
                gout.data[lin_index(&oc, &gout.shape)]
            };
            gb[k] += g;
            for ch in 0..channels {
                for j in 0..vol {
                    let jc = decompose(j, &c.kernel);
                    let mut full = Vec::with_capacity(r + 1);
                    if has_cdim {
                        full.push(ch);
                    }
                    let mut inb = true;
                    for d in 0..r {
                        let pos = oc[d] * c.stride[d] + jc[d];
                        if pos < c.pad[d] {
                            inb = false;
                            break;
                        }
                        let pos = pos - c.pad[d];
                        if pos >= sp[d] {
                            inb = false;
                            break;
                        }
                        full.push(pos);
                    }
                    if !inb {
                        continue;
                    }
                    let xi = lin_index(&full, &x.shape);
                    gin[xi] += g * c.w[k][ch * vol + j];
                    gw[k][ch * vol + j] += g * x.data[xi];
                }
            }
        }
    }
    Some((Tensor::from_flat(x.shape.clone(), gin)?, gw, gb))
}

fn pool_backward(p: &Pool, x: &Tensor, gout: &Tensor) -> Option<Tensor> {
    let (channels, sp) = conv_input_dims(&p.size, &x.shape)?;
    let r = p.size.len();
    let out_sp: Vec<usize> = (0..r)
        .map(|i| spatial_out(sp[i], p.size[i], 0, p.stride[i]))
        .collect();
    let vol = product(&p.size);
    let has_cdim = x.rank() > r;
    let total = product(&out_sp);
    let mut gin = vec![0.0f64; x.data.len()];
    for oi in 0..total {
        let oc = decompose(oi, &out_sp);
        for ch in 0..channels {
            let g = if has_cdim {
                let mut f = vec![ch];
                f.extend_from_slice(&oc);
                gout.data[lin_index(&f, &gout.shape)]
            } else {
                gout.data[lin_index(&oc, &gout.shape)]
            };
            let mut best = f64::NEG_INFINITY;
            let mut arg = 0usize;
            for j in 0..vol {
                let jc = decompose(j, &p.size);
                let mut full = Vec::with_capacity(r + 1);
                if has_cdim {
                    full.push(ch);
                }
                for d in 0..r {
                    full.push(oc[d] * p.stride[d] + jc[d]);
                }
                let v = x.data[lin_index(&full, &x.shape)];
                if v > best {
                    best = v;
                    arg = lin_index(&full, &x.shape);
                }
            }
            if p.max {
                gin[arg] += g;
            } else {
                let share = g / vol as f64;
                for j in 0..vol {
                    let jc = decompose(j, &p.size);
                    let mut full = Vec::with_capacity(r + 1);
                    if has_cdim {
                        full.push(ch);
                    }
                    for d in 0..r {
                        full.push(oc[d] * p.stride[d] + jc[d]);
                    }
                    gin[lin_index(&full, &x.shape)] += share;
                }
            }
        }
    }
    Tensor::from_flat(x.shape.clone(), gin)
}

/// Backward through one layer.
fn layer_backward_t(
    layer: &Value,
    x_in: &Tensor,
    gout: &Tensor,
    env: &Env,
) -> Option<(Tensor, Option<PGrad>)> {
    let fields = obj_fields(layer)?;
    match net_type(layer)? {
        "Linear" => {
            let out = get_i64(fields, "Out")? as usize;
            let lin = stored_weights(fields, x_in.numel(), out)?;
            let mut g = vec![0.0f64; x_in.numel()];
            let mut gw = vec![vec![0.0f64; x_in.numel()]; out];
            let mut gb = vec![0.0f64; out];
            for r in 0..out.min(gout.numel()) {
                gb[r] += gout.data[r];
                for c in 0..x_in.numel().min(lin.w[r].len()) {
                    gw[r][c] += gout.data[r] * x_in.data[c];
                    g[c] += lin.w[r][c] * gout.data[r];
                }
            }
            Some((Tensor::vector(g), Some(PGrad::Lin(gw, gb))))
        }
        "Elementwise" | "Threading" => {
            let key = if net_type(layer) == Some("Threading") {
                "Function"
            } else {
                "Activation"
            };
            let f = activation_of(fields, key);
            let deriv: Vec<f64> = x_in
                .data
                .iter()
                .map(|&v| act_deriv(&f, v, act_forward(&f, v, env), env))
                .collect();
            let g: Vec<f64> = gout
                .data
                .iter()
                .zip(deriv.iter())
                .map(|(g, d)| g * d)
                .collect();
            Some((Tensor::from_flat(x_in.shape.clone(), g)?, None))
        }
        "Softmax" => {
            let s = softmax_t(x_in);
            let n = if x_in.rank() <= 1 {
                x_in.numel()
            } else {
                *x_in.shape.last().unwrap()
            }
            .max(1);
            let mut g = vec![0.0f64; x_in.numel()];
            for (chunk_i, sc) in s.data.chunks(n).enumerate() {
                let gc = &gout.data[chunk_i * n..chunk_i * n + n];
                let dot: f64 = sc.iter().zip(gc.iter()).map(|(a, b)| a * b).sum();
                for j in 0..n {
                    g[chunk_i * n + j] = sc[j] * (gc[j] - dot);
                }
            }
            Some((Tensor::from_flat(x_in.shape.clone(), g)?, None))
        }
        "Convolution" => {
            let conv = read_conv(fields)?;
            let (gin, gw, gb) = conv_backward(&conv, x_in, gout)?;
            Some((gin, Some(PGrad::Conv(gw, gb))))
        }
        "Pooling" => {
            let pool = read_pool(fields)?;
            Some((pool_backward(&pool, x_in, gout)?, None))
        }
        "BatchNorm" => {
            let var = get_f64(fields, "Variance").unwrap_or(1.0).max(0.0);
            let eps = get_f64(fields, "Epsilon").unwrap_or(BN_EPSILON);
            let inv = 1.0 / (var + eps).sqrt();
            let g: Vec<f64> = gout.data.iter().map(|&v| v * inv).collect();
            Some((Tensor::from_flat(x_in.shape.clone(), g)?, None))
        }
        "Flatten" | "Reshape" | "Dropout" => Some((
            Tensor::from_flat(x_in.shape.clone(), gout.data.clone())?,
            None,
        )),
        _ => Some((gout.clone(), None)),
    }
}

fn out_grad(loss: Option<&Value>, pred: &Tensor, target: &Tensor) -> Tensor {
    let n = pred.numel().min(target.numel()).max(1);
    let data: Vec<f64> = match loss.and_then(net_type) {
        Some("CrossEntropyLoss") => (0..pred.numel().min(target.numel()))
            .map(|i| {
                let p = pred.data[i].clamp(1e-300, f64::INFINITY);
                -target.data[i] / p / n as f64
            })
            .collect(),
        _ => (0..pred.numel().min(target.numel()))
            .map(|i| 2.0 * (pred.data[i] - target.data[i]) / n as f64)
            .collect(),
    };
    Tensor::from_flat(pred.shape.clone(), data.clone()).unwrap_or_else(|| Tensor::vector(data))
}

/// Full-batch SGD over a dense chain. Returns the trained layers (matching the
/// trainable layer list of the input net, in order).
#[allow(clippy::too_many_arguments)]
fn train(
    layers: &[Value],
    loss: Option<&Value>,
    data: &[(Value, Value)],
    rounds: usize,
    lr: f64,
    env: &Env,
) -> Option<Vec<Value>> {
    // Infer the input shape and initialize all parameters up front.
    let first = tensor_from_value(&data[0].0)?;
    let mut cur = Some(first.shape.clone());
    let mut params: Vec<Value> = Vec::with_capacity(layers.len());
    for (i, layer) in layers.iter().enumerate() {
        let v = ensure_init(layer, cur.as_deref(), i);
        cur = cur.as_deref().and_then(|s| layer_output_shape(&v, s));
        params.push(v);
    }

    for _ in 0..rounds {
        let mut grads: Vec<Option<PGrad>> = params.iter().map(zero_pgrad).collect();

        for (xval, yval) in data {
            let x = tensor_from_value(xval)?;
            let target = tensor_from_value(yval)?;
            // Forward, caching each layer's input.
            let mut acts: Vec<Tensor> = Vec::with_capacity(params.len());
            let mut act = x.clone();
            for p in &params {
                acts.push(act.clone());
                act = layer_forward_t(p, &act, acts.len() - 1, env).ok()?;
            }

            // Backward.
            let mut g = out_grad(loss, &act, &target);
            for i in (0..params.len()).rev() {
                let (gin, pg) = layer_backward_t(&params[i], &acts[i], &g, env)?;
                if let Some(pg) = pg {
                    match &pg {
                        PGrad::Lin(w, b) => acc_lin(&mut grads[i], w, b),
                        PGrad::Conv(w, b) => acc_conv(&mut grads[i], w, b),
                    }
                }
                g = gin;
            }
        }

        // Apply the update.
        for (i, p) in params.iter_mut().enumerate() {
            match &grads[i] {
                Some(PGrad::Lin(gw, gb)) => {
                    if let Some(lin) = stored_weights(
                        obj_fields(p)?,
                        gw.first().map(Vec::len).unwrap_or(0),
                        gw.len(),
                    ) {
                        let mut new = lin.clone();
                        for r in 0..new.w.len() {
                            for c in 0..new.w[r].len() {
                                new.w[r][c] -= lr * gw[r][c];
                            }
                            new.b[r] -= lr * gb[r];
                        }
                        *p = lin_to_fields(p, &new);
                    }
                }
                Some(PGrad::Conv(gw, gb)) => {
                    if let Some(mut conv) = read_conv(obj_fields(p)?) {
                        for k in 0..conv.w.len() {
                            for j in 0..conv.w[k].len() {
                                conv.w[k][j] -= lr * gw[k][j];
                            }
                            conv.b[k] -= lr * gb[k];
                        }
                        *p = conv_to_fields(p, &conv);
                    }
                }
                None => {}
            }
        }
    }
    Some(params)
}

/// Replace the trainable layer list of a net container with `new_layers`.
fn replace_layers(net: &Value, new_layers: Vec<Value>) -> Value {
    match net_type(net) {
        Some("Chain") => make_net(&[
            ("Type", Value::Str("Chain".to_string())),
            ("Layers", Value::List(new_layers)),
        ]),
        Some("Graph") => make_net(&[
            ("Type", Value::Str("Graph".to_string())),
            ("Nodes", Value::List(new_layers)),
            (
                "Edges",
                obj_fields(net).unwrap().get("Edges").unwrap().clone(),
            ),
        ]),
        Some(_) => new_layers.into_iter().next().unwrap_or_else(|| net.clone()),
        None => net.clone(),
    }
}

/// `NetTrain[net, data]` / `NetTrain[net, data, opts...]`.
fn builtin_net_train(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(unevaluated("NetTrain", args));
    }
    let net = &args[0];
    if obj_fields(net).is_none() {
        return Ok(unevaluated("NetTrain", args));
    }
    let data = match parse_training_data(&args[1]) {
        Some(d) if !d.is_empty() => d,
        _ => return Ok(unevaluated("NetTrain", args)),
    };

    let mut rounds = 400usize;
    let mut lr = 0.05f64;
    parse_options(&args[2..], &mut rounds, &mut lr);

    let all_layers = match forward_layers_of(net) {
        Some(l) if !l.is_empty() => l,
        _ => return Ok(unevaluated("NetTrain", args)),
    };
    let (train_layers, loss) = match all_layers.last() {
        Some(l) if is_loss(l) => (all_layers[..all_layers.len() - 1].to_vec(), Some(l.clone())),
        _ => (all_layers.clone(), None),
    };
    if train_layers.is_empty() {
        return Ok(unevaluated("NetTrain", args));
    }
    // All inputs must share a shape.
    let input_shape = tensor_from_value(&data[0].0).map(|t| t.shape);
    if data
        .iter()
        .any(|(x, _)| tensor_from_value(x).map(|t| t.shape) != input_shape)
    {
        return Ok(unevaluated("NetTrain", args));
    }

    let trained = match train(&train_layers, loss.as_ref(), &data, rounds, lr, env) {
        Some(t) => t,
        None => return Ok(unevaluated("NetTrain", args)),
    };

    // Write trained weights back; keep any trailing loss layer.
    let rebuilt = if net_type(net) == Some("Chain") || net_type(net) == Some("Graph") {
        let mut new_layers = trained;
        if loss.is_some() {
            new_layers.push(loss.clone().unwrap());
        }
        replace_layers(net, new_layers)
    } else {
        trained.into_iter().next().unwrap_or_else(|| net.clone())
    };
    Ok(rebuilt)
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins::to_f64;

    fn env_with_builtins() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    fn eval_str(input: &str) -> Value {
        let env = env_with_builtins();
        let tokens = crate::lexer::tokenize(input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap_or_else(|e| panic!("parse {input}: {e:?}"));
        crate::eval::eval_program(&ast, &env).unwrap_or_else(|e| panic!("eval {input}: {e:?}"))
    }

    fn num(input: &str) -> f64 {
        to_f64(&eval_str(input)).unwrap_or_else(|| panic!("not numeric: {input}"))
    }

    fn as_vec(v: &Value) -> Vec<f64> {
        match v {
            Value::List(items) => items.iter().filter_map(to_f64).collect(),
            other => vec![to_f64(other).unwrap()],
        }
    }

    /// Flatten an arbitrarily nested numeric list into a flat vector.
    fn flatten(v: &Value) -> Vec<f64> {
        match v {
            Value::List(items) => items.iter().flat_map(flatten).collect(),
            other => vec![to_f64(other).expect("numeric")],
        }
    }

    fn shape_of(v: &Value) -> Vec<usize> {
        let mut shape = Vec::new();
        let mut cur = v;
        while let Value::List(items) = cur {
            shape.push(items.len());
            match items.first() {
                Some(first) => cur = first,
                None => break,
            }
        }
        shape
    }

    fn field(v: &Value, key: &str) -> Value {
        match v {
            Value::Object { fields, .. } => fields.get(key).cloned().unwrap(),
            other => panic!("not a net object: {other:?}"),
        }
    }

    fn is_unevaluated(v: &Value) -> bool {
        matches!(v, Value::Call { .. })
    }

    // ── basic layers ────────────────────────────────────────────────────────

    #[test]
    fn test_linear_layer_shape() {
        let v = eval_str("LinearLayer[3]");
        assert_eq!(net_type(&v), Some("Linear"));
        assert_eq!(to_f64(&field(&v, "Out")), Some(3.0));
    }

    #[test]
    fn test_linear_layer_pair() {
        let v = eval_str("LinearLayer[{2, 3}]");
        assert_eq!(to_f64(&field(&v, "In")), Some(2.0));
        assert_eq!(to_f64(&field(&v, "Out")), Some(3.0));
        let v2 = eval_str("LinearLayer[{4}]");
        assert_eq!(to_f64(&field(&v2, "Out")), Some(4.0));
    }

    #[test]
    fn test_netchain_builds() {
        let v = eval_str("NetChain[{LinearLayer[{2, 3}], ElementwiseLayer[Tanh], LinearLayer[1]}]");
        assert_eq!(net_type(&v), Some("Chain"));
        match field(&v, "Layers") {
            Value::List(l) => assert_eq!(l.len(), 3),
            other => panic!("expected layers list, got {other:?}"),
        }
    }

    #[test]
    fn test_softmax_sums_to_one() {
        let v = eval_str("SoftmaxLayer[][{1., 2., 3.}]");
        let xs = as_vec(&v);
        let sum: f64 = xs.iter().sum();
        assert!((sum - 1.0).abs() < 1e-9, "softmax sum = {sum}");
        assert!(xs.iter().all(|x| *x >= 0.0));
    }

    #[test]
    fn test_softmax_rows_sum_to_one() {
        let v = eval_str("SoftmaxLayer[][{{1., 2.}, {3., 4.}}]");
        let Value::List(rows) = &v else {
            panic!("expected rows");
        };
        assert_eq!(rows.len(), 2);
        for row in rows {
            let s: f64 = as_vec(row).iter().sum();
            assert!((s - 1.0).abs() < 1e-9, "row sum = {s}");
        }
    }

    #[test]
    fn test_elementwise_tanh() {
        let v = eval_str("ElementwiseLayer[Tanh][{0.}]");
        assert!((as_vec(&v)[0] - 0.0).abs() < 1e-9);
        let v2 = eval_str("ElementwiseLayer[LogisticSigmoid][{0.}]");
        assert!((as_vec(&v2)[0] - 0.5).abs() < 1e-9);
        let v3 = eval_str("ElementwiseLayer[Sigmoid][{0.}]");
        assert!((as_vec(&v3)[0] - 0.5).abs() < 1e-9);
        let v4 = eval_str("ElementwiseLayer[Ramp][{-2., 3.}]");
        assert_eq!(as_vec(&v4), vec![0.0, 3.0]);
    }

    #[test]
    fn test_elementwise_preserves_shape() {
        let v = eval_str("ElementwiseLayer[Ramp][{{-1., 2.}, {3., -4.}}]");
        assert_eq!(shape_of(&v), vec![2, 2]);
        assert_eq!(flatten(&v), vec![0.0, 2.0, 3.0, 0.0]);
    }

    #[test]
    fn test_threading_layer() {
        let v = eval_str("ThreadingLayer[Plus][{1., 2., 3.}]");
        assert_eq!(as_vec(&v), vec![1.0, 2.0, 3.0]);
        let v2 = eval_str("ThreadingLayer[Times][{{2., 3.}, {4., 5.}}]");
        assert_eq!(shape_of(&v2), vec![2, 2]);
    }

    #[test]
    fn test_chain_forward_length() {
        let v = eval_str(
            "net = NetChain[{LinearLayer[{2, 3}], ElementwiseLayer[Tanh], LinearLayer[1]}]; net[{1., 2.}]",
        );
        assert_eq!(as_vec(&v).len(), 1);
    }

    #[test]
    fn test_netinitialize_deterministic() {
        let a = eval_str(
            "SeedRandom[1]; net = NetInitialize[NetChain[{LinearLayer[{2, 2}], LinearLayer[1]}]]; net[{1., 2.}]",
        );
        let b = eval_str(
            "SeedRandom[1]; net = NetInitialize[NetChain[{LinearLayer[{2, 2}], LinearLayer[1]}]]; net[{1., 2.}]",
        );
        assert_eq!(as_vec(&a), as_vec(&b));
    }

    // ── tensor shaping ──────────────────────────────────────────────────────

    #[test]
    fn test_flatten_layer() {
        let v = eval_str("FlattenLayer[][{{1., 2.}, {3., 4.}}]");
        assert_eq!(as_vec(&v), vec![1.0, 2.0, 3.0, 4.0]);
        let v2 = eval_str("FlattenLayer[][{1., 2.}]");
        assert_eq!(as_vec(&v2), vec![1.0, 2.0]);
    }

    #[test]
    fn test_reshape_layer() {
        let v = eval_str("ReshapeLayer[{2, 2}][{1., 2., 3., 4.}]");
        assert_eq!(shape_of(&v), vec![2, 2]);
        assert_eq!(flatten(&v), vec![1.0, 2.0, 3.0, 4.0]);
        let v2 = eval_str("ReshapeLayer[4][{{1., 2.}, {3., 4.}}]");
        assert_eq!(as_vec(&v2), vec![1.0, 2.0, 3.0, 4.0]);
    }

    #[test]
    fn test_reshape_mismatch_unevaluated() {
        let v = eval_str("ReshapeLayer[{3}][{1., 2.}]");
        assert!(is_unevaluated(&v), "got {v:?}");
    }

    #[test]
    fn test_dropout_is_identity_in_inference() {
        let v = eval_str("DropoutLayer[0.5][{1., 2., 3.}]");
        assert_eq!(as_vec(&v), vec![1.0, 2.0, 3.0]);
    }

    #[test]
    fn test_batchnorm_inference_near_identity() {
        let v = eval_str("BatchNormalizationLayer[][{1., 2., 3.}]");
        let xs = as_vec(&v);
        for (a, b) in xs.iter().zip([1.0, 2.0, 3.0]) {
            assert!((a - b).abs() < 1e-3, "{a} vs {b}");
        }
    }

    // ── pooling ─────────────────────────────────────────────────────────────

    #[test]
    fn test_pool_max_1d() {
        let v = eval_str("PoolingLayer[2][{1., 3., 2., 5., 4., 6.}]");
        assert_eq!(as_vec(&v), vec![3.0, 5.0, 6.0]);
    }

    #[test]
    fn test_pool_average_1d() {
        let v = eval_str("PoolingLayer[2, \"Average\"][{1., 3., 2., 5., 4., 6.}]");
        assert_eq!(as_vec(&v), vec![2.0, 3.5, 5.0]);
    }

    #[test]
    fn test_pool_max_2d() {
        // Default stride equals the window, so a 3x3 input with a 2x2 window
        // collapses to a single max over the top-left window.
        let v = eval_str("PoolingLayer[{2, 2}][{{1., 2., 3.}, {4., 5., 6.}, {7., 8., 9.}}]");
        assert_eq!(shape_of(&v), vec![1, 1]);
        assert_eq!(flatten(&v), vec![5.0]);
    }

    // ── convolution ─────────────────────────────────────────────────────────

    #[test]
    fn test_conv1d_shape_and_determinism() {
        let a = eval_str(
            "SeedRandom[1]; net = NetInitialize[ConvolutionLayer[3]]; net[{1., 2., 3., 4., 5.}]",
        );
        assert_eq!(shape_of(&a), vec![3]);
        let b = eval_str(
            "SeedRandom[1]; net = NetInitialize[ConvolutionLayer[3]]; net[{1., 2., 3., 4., 5.}]",
        );
        assert_eq!(flatten(&a), flatten(&b));
    }

    #[test]
    fn test_conv1d_matches_manual() {
        // Initialize, read back the weights, and reproduce the convolution.
        let net = eval_str("SeedRandom[1]; NetInitialize[ConvolutionLayer[3]]");
        let w = flatten(&field(&net, "Weights"));
        let b = flatten(&field(&net, "Biases"));
        assert_eq!(w.len(), 3);
        let x = [1.0, 2.0, 3.0, 4.0, 5.0];
        let out = eval_str(
            "SeedRandom[1]; net = NetInitialize[ConvolutionLayer[3]]; net[{1., 2., 3., 4., 5.}]",
        );
        let got = as_vec(&out);
        assert_eq!(got.len(), 3);
        for o in 0..3 {
            let expect: f64 = b[0] + (0..3).map(|k| w[k] * x[o + k]).sum::<f64>();
            assert!((got[o] - expect).abs() < 1e-9, "out[{o}] = {}", got[o]);
        }
    }

    #[test]
    fn test_conv1d_padded_length() {
        // Padding of one keeps the input length for a size-3 kernel.
        let v = eval_str(
            "SeedRandom[2]; net = NetInitialize[ConvolutionLayer[3, \"PaddingSize\" -> 1]]; net[{1., 2., 3., 4., 5.}]",
        );
        assert_eq!(shape_of(&v), vec![5]);
    }

    #[test]
    fn test_conv2d_shape() {
        let v = eval_str(
            "SeedRandom[3]; net = NetInitialize[ConvolutionLayer[{2, 2}]]; net[{{1., 2., 3.}, {4., 5., 6.}, {7., 8., 9.}}]",
        );
        assert_eq!(shape_of(&v), vec![2, 2]);
    }

    #[test]
    fn test_conv_multi_kernel_channels() {
        // Two kernels over a two-channel signal.
        let v = eval_str(
            "SeedRandom[4]; net = NetInitialize[ConvolutionLayer[2, 2]]; net[{{1., 2., 3.}, {4., 5., 6.}}]",
        );
        assert_eq!(shape_of(&v), vec![2, 2]);
    }

    #[test]
    fn test_conv_netinitialize_deterministic() {
        let a = eval_str(
            "SeedRandom[9]; net = NetInitialize[ConvolutionLayer[{2, 2}]]; net[{{1., 2.}, {3., 4.}}]",
        );
        let b = eval_str(
            "SeedRandom[9]; net = NetInitialize[ConvolutionLayer[{2, 2}]]; net[{{1., 2.}, {3., 4.}}]",
        );
        assert_eq!(flatten(&a), flatten(&b));
    }

    // ── graph / extract / loss ──────────────────────────────────────────────

    #[test]
    fn test_netgraph_chain_forward() {
        let v = eval_str(
            "SeedRandom[1]; net = NetInitialize[NetGraph[{LinearLayer[{2, 3}], ElementwiseLayer[Tanh], LinearLayer[1]}, {1 -> 2, 2 -> 3}]]; net[{1., 2.}]",
        );
        assert_eq!(as_vec(&v).len(), 1);
    }

    #[test]
    fn test_netgraph_branch() {
        // Two parallel branches concatenate at the sink.
        let v = eval_str(
            "SeedRandom[1]; net = NetInitialize[NetGraph[{LinearLayer[{2, 2}], LinearLayer[{2, 1}], LinearLayer[{2, 1}]}, {1 -> 2, 1 -> 3}]]; net[{1., 2.}]",
        );
        // Sink is node 3; branch concatenation feeds node 3 a length-2 vector.
        assert_eq!(as_vec(&v).len(), 1);
    }

    #[test]
    fn test_netextract_basic() {
        let out = eval_str(
            "NetExtract[NetChain[{LinearLayer[3], ElementwiseLayer[Tanh], LinearLayer[2]}], 3][\"Out\"]",
        );
        assert_eq!(to_f64(&out), Some(2.0));
    }

    #[test]
    fn test_netextract_graph() {
        let out = eval_str("NetExtract[NetGraph[{LinearLayer[3], LinearLayer[5]}], 2][\"Out\"]");
        assert_eq!(to_f64(&out), Some(5.0));
    }

    #[test]
    fn test_loss_function_builds() {
        assert_eq!(
            net_type(&eval_str("LossFunction[\"MeanSquaredLossLayer\"]")),
            Some("MeanSquaredLoss")
        );
        assert_eq!(
            net_type(&eval_str("LossFunction[\"CrossEntropyLossLayer\"]")),
            Some("CrossEntropyLoss")
        );
    }

    #[test]
    fn test_mse_loss_zero() {
        let v = num("MeanSquaredLossLayer[][{{1., 2.}, {1., 2.}}]");
        assert!(v.abs() < 1e-12);
    }

    #[test]
    fn test_mse_loss_value() {
        let v = num("MeanSquaredLossLayer[][{{1., 2.}, {0., 0.}}]");
        assert!((v - 2.5).abs() < 1e-9, "got {v}");
    }

    #[test]
    fn test_ce_loss_value() {
        let v = num("CrossEntropyLossLayer[][{{0.5, 0.5}, {1., 0.}}]");
        let expected = 2f64.ln() / 2.0;
        assert!((v - expected).abs() < 1e-9, "got {v}");
    }

    // ── training ────────────────────────────────────────────────────────────

    #[test]
    fn test_net_train_fits_line() {
        let v = num(
            "SeedRandom[1]; net = NetTrain[NetChain[{LinearLayer[{1, 1}]}], \
             {{0.} -> {0.}, {1.} -> {2.}, {2.} -> {4.}, {3.} -> {6.}}, \
             MaxTrainingRounds -> 2000, LearningRate -> 0.05]; net[{3.}][[1]]",
        );
        assert!((v - 6.0).abs() < 0.2, "trained output = {v}");
    }

    #[test]
    fn test_net_train_reduces_loss() {
        let before = num(
            "SeedRandom[1]; net = NetChain[{LinearLayer[{1, 1}], MeanSquaredLossLayer[]}]; \
             net[{{0.}, {0.}}]",
        );
        let after = num(
            "SeedRandom[1]; net = NetTrain[NetChain[{LinearLayer[{1, 1}], MeanSquaredLossLayer[]}], \
             {{0.} -> {0.}, {1.} -> {1.}, {2.} -> {2.}}, MaxTrainingRounds -> 300]; \
             net[{{0.}, {0.}}]",
        );
        assert!(
            after <= before + 1e-9,
            "loss did not decrease: {before} -> {after}"
        );
    }

    #[test]
    fn test_net_train_flatten_pipeline() {
        // Nested input -> Flatten -> Linear; learns the entry-wise sum.
        let v = num(
            "SeedRandom[1]; net = NetTrain[NetChain[{FlattenLayer[], LinearLayer[{4, 1}]}], \
             {{{{1., 2.}, {3., 4.}}} -> {10.}, {{{1., 1.}, {1., 1.}}} -> {4.}, \
              {{{0., 0.}, {0., 0.}}} -> {0.}}, MaxTrainingRounds -> 800, LearningRate -> 0.02]; \
             net[{{1., 2.}, {3., 4.}}][[1]]",
        );
        assert!((v - 10.0).abs() < 0.5, "trained output = {v}");
    }

    #[test]
    fn test_net_train_conv_reduces_loss() {
        let before = num(
            "SeedRandom[1]; net = NetChain[{ConvolutionLayer[2], FlattenLayer[], \
             MeanSquaredLossLayer[]}]; net[{{1., 2., 3., 4.}, {1., 1., 1.}}]",
        );
        let after = num(
            "SeedRandom[1]; net = NetTrain[NetChain[{ConvolutionLayer[2], FlattenLayer[], \
             MeanSquaredLossLayer[]}], {{1., 2., 3., 4.} -> {1., 1., 1.}}, \
             MaxTrainingRounds -> 300, LearningRate -> 0.05]; \
             net[{{1., 2., 3., 4.}, {1., 1., 1.}}]",
        );
        assert!(
            after <= before + 1e-9,
            "conv loss did not decrease: {before} -> {after}"
        );
    }

    #[test]
    fn test_net_train_softmax_crossentropy() {
        let before = num(
            "SeedRandom[1]; net = NetChain[{LinearLayer[{2, 2}], SoftmaxLayer[], \
             CrossEntropyLossLayer[]}]; net[{{1., 0.5}, {1., 0.}}]",
        );
        let after = num(
            "SeedRandom[1]; net = NetTrain[NetChain[{LinearLayer[{2, 2}], SoftmaxLayer[], \
             CrossEntropyLossLayer[]}], {{1., 0.5} -> {1., 0.}, {0.5, 1.} -> {0., 1.}}, \
             MaxTrainingRounds -> 200, LearningRate -> 0.1]; net[{{1., 0.5}, {1., 0.}}]",
        );
        assert!(
            after < before,
            "ce loss did not decrease: {before} -> {after}"
        );
    }

    #[test]
    fn test_net_train_pool_chain() {
        let v = eval_str(
            "SeedRandom[1]; net = NetTrain[NetChain[{PoolingLayer[2], LinearLayer[{2, 1}]}], \
             {{1., 3., 2., 5.} -> {2.}, {2., 4., 1., 5.} -> {2.}}, MaxTrainingRounds -> 100]; \
             net[{1., 3., 2., 5.}]",
        );
        assert_eq!(as_vec(&v).len(), 1);
    }

    // ── unevaluated / accessors ─────────────────────────────────────────────

    #[test]
    fn test_unevaluated_shapes() {
        assert!(is_unevaluated(&eval_str("LinearLayer[0]")));
        assert!(is_unevaluated(&eval_str("NetChain[{1, 2}]")));
        assert!(is_unevaluated(&eval_str(
            "ConvolutionLayer[{2, 2}][{1., 2., 3.}]"
        )));
    }

    #[test]
    fn test_field_access() {
        let v = eval_str("LinearLayer[5][\"Out\"]");
        assert_eq!(to_f64(&v), Some(5.0));
    }

    #[test]
    fn test_debug_train() {
        let v = eval_str(
            "SeedRandom[1]; net = NetTrain[NetChain[{LinearLayer[{1, 1}]}], {{0.} -> {0.}, {1.} -> {2.}}]; {net, net[{3.}]}",
        );
        assert_eq!(shape_of(&v), vec![2]);
        let v2 = eval_str("NetTrain[LinearLayer[{1, 1}], {{0.} -> {0.}}]");
        assert!(obj_fields(&v2).is_some());
    }
}
