//! neural-net-lite: LinearLayer, NetChain, NetInitialize, NetTrain (basic).
//!
//! Gap-fill module; see the individual builtin docs for semantics.
//!
//! # Representation
//!
//! A "net" is a [`Value::Object`] with class name `"NeuralNet"`. The `Type`
//! field distinguishes the layer/container kind (`"Chain"`, `"Linear"`,
//! `"Elementwise"`, `"Softmax"`, `"MeanSquaredLoss"`, `"CrossEntropyLoss"`)
//! and the remaining fields carry its architecture:
//!
//! * `Linear` — `Out` (int), optional `In` (int), optional `Weights`/`Biases`.
//! * `Elementwise` — `Activation` (a function value applied elementwise).
//! * `Chain` — `Layers` (a list of layer objects).
//!
//! An `Object` (rather than a `Value::Call`) is used because applying a value
//! dispatches to `"<Class>.__method__"` (see `eval::apply_function`), which
//! gives an actual callable `net[x]`. A bare `Value::Call` head is not a valid
//! symbol and would be re-stringified on each application.
//!
//! # Determinism
//!
//! Linear weights are generated deterministically from a per-layer seed using
//! the shared `random` RNG (the same one driven by `SeedRandom`). Thus
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

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "LinearLayer", builtin_linear_layer);
    super::register_builtin(env, "ElementwiseLayer", builtin_elementwise_layer);
    super::register_builtin(env, "SoftmaxLayer", builtin_softmax_layer);
    super::register_builtin(env, "MeanSquaredLossLayer", builtin_mse_layer);
    super::register_builtin(env, "CrossEntropyLossLayer", builtin_ce_layer);
    super::register_builtin(env, "NetChain", builtin_net_chain);
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

/// Convert a scalar or list-of-numbers value into a `Vec<f64>`.
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

/// Deterministic per-layer seed derived from its position and shape.
fn layer_seed(index: usize, out: usize, input: usize) -> u64 {
    const M1: u64 = 0x9E37_79B9_7F4A_7C15;
    const M2: u64 = 0xBF58_476D_1CE4_E5B9;
    M1 ^ (index as u64)
        .wrapping_mul(M2)
        .wrapping_add((out as u64).wrapping_mul(31))
        .wrapping_add(input as u64)
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
        Some("LogisticSigmoid") => 1.0 / (1.0 + (-x).exp()),
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
        Some("LogisticSigmoid") => y * (1.0 - y),
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

/// `SoftmaxLayer[]`.
fn builtin_softmax_layer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() > 1 {
        return Ok(unevaluated("SoftmaxLayer", args));
    }
    Ok(make_net(&[("Type", Value::Str("Softmax".to_string()))]))
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

/// `NetChain[{layer, ...}]`.
fn builtin_net_chain(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("NetChain", args));
    }
    let layers = match as_list(&args[0]) {
        Some(l) => l.clone(),
        None => return Ok(unevaluated("NetChain", args)),
    };
    if !layers.iter().all(|l| obj_fields(l).is_some()) {
        return Ok(unevaluated("NetChain", args));
    }
    Ok(make_net(&[
        ("Type", Value::Str("Chain".to_string())),
        ("Layers", Value::List(layers)),
    ]))
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

fn layer_forward(layer: &Value, x: &[f64], index: usize, env: &Env) -> Result<Vec<f64>, EvalError> {
    let fields =
        obj_fields(layer).ok_or_else(|| EvalError::Error("not a net layer".to_string()))?;
    match net_type(layer) {
        Some("Linear") => {
            let out = get_i64(fields, "Out").unwrap_or(x.len() as i64) as usize;
            let lin = materialize_linear(layer, x.len(), index)
                .ok_or_else(|| EvalError::Error("bad linear layer".to_string()))?;
            let mut y = lin.b.clone();
            for r in 0..out.min(y.len()) {
                for c in 0..x.len().min(lin.w[r].len()) {
                    y[r] += lin.w[r][c] * x[c];
                }
            }
            Ok(y)
        }
        Some("Elementwise") => {
            let f = fields
                .get("Activation")
                .cloned()
                .unwrap_or_else(|| Value::Symbol("Identity".to_string()));
            Ok(x.iter().map(|&xi| act_forward(&f, xi, env)).collect())
        }
        Some("Softmax") => Ok(softmax(x)),
        _ => Ok(x.to_vec()),
    }
}

fn eval_network(layers: &[Value], input: &[f64], env: &Env) -> Result<Vec<f64>, EvalError> {
    let mut act = input.to_vec();
    for (i, layer) in layers.iter().enumerate() {
        if is_loss(layer) {
            continue;
        }
        act = layer_forward(layer, &act, i, env)?;
    }
    Ok(act)
}

fn forward_layers_of(net: &Value) -> Option<Vec<Value>> {
    match net_type(net)? {
        "Chain" => Some(as_list(obj_fields(net)?.get("Layers")?)?.clone()),
        _ => Some(vec![net.clone()]),
    }
}

fn loss_scalar(loss: &Value, pred: &[f64], target: &[f64]) -> Result<f64, EvalError> {
    let n = pred.len().min(target.len());
    if n == 0 {
        return Ok(0.0);
    }
    let v = match net_type(loss) {
        Some("MeanSquaredLoss") => {
            let s: f64 = (0..n).map(|i| (pred[i] - target[i]).powi(2)).sum();
            s / n as f64
        }
        Some("CrossEntropyLoss") => {
            let mut s = 0.0;
            for i in 0..n {
                let p = pred[i].clamp(1e-300, f64::INFINITY);
                s -= target[i] * p.ln();
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
    let layers = match forward_layers_of(net) {
        Some(l) if !l.is_empty() => l,
        _ => return Ok(unevaluated("NeuralNet", &[net.clone(), input.clone()])),
    };
    if let Some(last) = layers.last()
        && is_loss(last)
    {
        if let Value::List(pair) = input
            && pair.len() == 2
            && pair.iter().all(|p| matches!(p, Value::List(_)))
        {
            if layers.len() == 1 {
                let pred = to_vec(&pair[0]).unwrap_or_default();
                let target = to_vec(&pair[1]).unwrap_or_default();
                return Ok(super::real(loss_scalar(last, &pred, &target)?));
            }
            let pred = eval_network(
                &layers[..layers.len() - 1],
                &to_vec(&pair[0]).unwrap_or_default(),
                env,
            )?;
            let target = to_vec(&pair[1]).unwrap_or_default();
            return Ok(super::real(loss_scalar(last, &pred, &target)?));
        }
        let x = to_vec(input).unwrap_or_default();
        let out = eval_network(&layers[..layers.len() - 1], &x, env)?;
        return Ok(value_from_vec(&out));
    }
    let x = match to_vec(input) {
        Some(x) => x,
        None => return Ok(unevaluated("NeuralNet", &[net.clone(), input.clone()])),
    };
    let out = eval_network(&layers, &x, env)?;
    Ok(value_from_vec(&out))
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

// ── NetInitialize ───────────────────────────────────────────────────────────

/// Initialize one layer in place of a chain walk, updating `cur_in`.
///
/// `first_in` is only consumed for the first (index 0) layer when that layer
/// has no explicit input dimension.
fn init_one_layer(
    layer: &Value,
    index: usize,
    cur_in: &mut usize,
    first_in: &mut Option<usize>,
) -> Value {
    if !is_linear(layer) {
        return layer.clone();
    }
    let fields = match obj_fields(layer) {
        Some(f) => f,
        None => return layer.clone(),
    };
    let out = match get_i64(fields, "Out") {
        Some(o) if o > 0 => o as usize,
        _ => return layer.clone(),
    };
    if let Some(inp) = get_i64(fields, "In")
        && inp > 0
    {
        *cur_in = inp as usize;
    } else if index == 0
        && let Some(fi) = first_in.take()
    {
        *cur_in = fi;
    }
    let lin = init_weights(*cur_in, out, layer_seed(index, out, *cur_in));
    *cur_in = out;
    lin_to_fields(layer, &lin)
}

/// `NetInitialize[net]` / `NetInitialize[net, sampleInput]`.
fn builtin_net_initialize(args: &[Value], _env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("NetInitialize", args));
    }
    let net = &args[0];
    let mut first_in = args.get(1).and_then(to_vec).map(|v| v.len());
    super::random::seed_rng(NET_INIT_SEED);

    let rebuilt = match net_type(net) {
        Some("Chain") => {
            let layers = match as_list(obj_fields(net).unwrap().get("Layers").unwrap()) {
                Some(l) => l.clone(),
                None => return Ok(unevaluated("NetInitialize", args)),
            };
            let mut cur_in = first_in.unwrap_or(1);
            first_in = args.get(1).and_then(to_vec).map(|v| v.len());
            let new_layers = layers
                .iter()
                .enumerate()
                .map(|(i, l)| init_one_layer(l, i, &mut cur_in, &mut first_in))
                .collect();
            make_net(&[
                ("Type", Value::Str("Chain".to_string())),
                ("Layers", Value::List(new_layers)),
            ])
        }
        Some("Linear") => {
            let mut cur_in = first_in.unwrap_or(1);
            init_one_layer(net, 0, &mut cur_in, &mut first_in)
        }
        _ => net.clone(),
    };
    Ok(rebuilt)
}

// ── NetTrain ────────────────────────────────────────────────────────────────

fn parse_training_data(data: &Value) -> Option<Vec<(Vec<f64>, Vec<f64>)>> {
    let items = as_list(data)?;
    if items.is_empty() {
        return None;
    }
    // List of rules `input -> target`.
    if items.iter().all(|it| matches!(it, Value::Rule { .. })) {
        let mut out = Vec::with_capacity(items.len());
        for it in items {
            if let Value::Rule { lhs, rhs, .. } = it {
                out.push((to_vec(lhs)?, to_vec(rhs)?));
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
            out.push((to_vec(i)?, to_vec(t)?));
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
        out.push((to_vec(&pair[0])?, to_vec(&pair[1])?));
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

enum Param {
    Lin(Lin),
    Act(Value),
    Softmax,
}

/// Accumulated weight/bias gradients for one linear layer.
type Grad = (Vec<Vec<f64>>, Vec<f64>);
/// Per-layer gradient slots (indexed like `params`; `None` for non-linear).
type Grads = Vec<Option<Grad>>;

fn out_grad(loss: Option<&Value>, pred: &[f64], target: &[f64]) -> Vec<f64> {
    let n = pred.len().min(target.len()).max(1);
    match loss.and_then(net_type) {
        Some("CrossEntropyLoss") => (0..n)
            .map(|i| {
                let p = pred[i].clamp(1e-300, f64::INFINITY);
                -target[i] / p / n as f64
            })
            .collect(),
        _ => (0..n)
            .map(|i| 2.0 * (pred[i] - target[i]) / n as f64)
            .collect(),
    }
}

/// Full-batch SGD over a dense chain. Returns the trained linear weights in
/// layer order (matching the linear layers of the input net).
#[allow(clippy::too_many_arguments)]
fn train(
    layers: &[Value],
    loss: Option<&Value>,
    data: &[(Vec<f64>, Vec<f64>)],
    input_dim: usize,
    rounds: usize,
    lr: f64,
    env: &Env,
) -> Option<Vec<Lin>> {
    // Materialize parameters (generating any missing weights).
    let mut params: Vec<Param> = Vec::with_capacity(layers.len());
    let mut cur_in = input_dim;
    for (i, layer) in layers.iter().enumerate() {
        match net_type(layer) {
            Some("Linear") => {
                let out = get_i64(obj_fields(layer)?, "Out")? as usize;
                let lin = materialize_linear(layer, cur_in, i)?;
                cur_in = out;
                params.push(Param::Lin(lin));
            }
            Some("Elementwise") => {
                let f = obj_fields(layer)?.get("Activation")?.clone();
                params.push(Param::Act(f));
            }
            Some("Softmax") => params.push(Param::Softmax),
            _ => return None,
        }
    }

    for _ in 0..rounds {
        let mut grads: Grads = params
            .iter()
            .map(|p| match p {
                Param::Lin(l) => {
                    let width = l.w.first().map(Vec::len).unwrap_or(0);
                    Some((
                        vec![vec![0.0f64; width]; l.w.len()],
                        vec![0.0f64; l.b.len()],
                    ))
                }
                _ => None,
            })
            .collect();

        for (x, y) in data {
            // Forward, caching each layer's input.
            let mut acts = Vec::with_capacity(params.len());
            let mut act = x.clone();
            for p in &params {
                acts.push(act.clone());
                act = match p {
                    Param::Lin(l) => {
                        let mut z = l.b.clone();
                        for r in 0..l.w.len() {
                            for c in 0..act.len().min(l.w[r].len()) {
                                z[r] += l.w[r][c] * act[c];
                            }
                        }
                        z
                    }
                    Param::Act(f) => act.iter().map(|&v| act_forward(f, v, env)).collect(),
                    Param::Softmax => softmax(&act),
                };
            }

            // Backward.
            let mut g = out_grad(loss, &act, y);
            for i in (0..params.len()).rev() {
                let x_in = &acts[i];
                match &params[i] {
                    Param::Lin(l) => {
                        let grad = grads[i].as_mut().unwrap();
                        for r in 0..l.w.len().min(g.len()) {
                            grad.1[r] += g[r];
                            for c in 0..x_in.len() {
                                grad.0[r][c] += g[r] * x_in[c];
                            }
                        }
                        let mut new_g = vec![0.0; x_in.len()];
                        for r in 0..l.w.len().min(g.len()) {
                            for c in 0..new_g.len().min(l.w[r].len()) {
                                new_g[c] += l.w[r][c] * g[r];
                            }
                        }
                        g = new_g;
                    }
                    Param::Act(f) => {
                        let y_out: Vec<f64> =
                            x_in.iter().map(|&v| act_forward(f, v, env)).collect();
                        g = (0..x_in.len())
                            .map(|j| {
                                g.get(j).copied().unwrap_or(0.0)
                                    * act_deriv(f, x_in[j], y_out[j], env)
                            })
                            .collect();
                    }
                    Param::Softmax => {
                        let s = softmax(x_in);
                        let dot: f64 = (0..s.len())
                            .map(|j| g.get(j).copied().unwrap_or(0.0) * s[j])
                            .sum();
                        g = (0..s.len())
                            .map(|j| s[j] * (g.get(j).copied().unwrap_or(0.0) - dot))
                            .collect();
                    }
                }
            }
        }

        // Apply the update.
        for (i, p) in params.iter_mut().enumerate() {
            if let Param::Lin(l) = p
                && let Some((gw, gb)) = &grads[i]
            {
                for r in 0..l.w.len() {
                    for c in 0..l.w[r].len().min(gw.get(r).map(Vec::len).unwrap_or(0)) {
                        l.w[r][c] -= lr * gw[r][c];
                    }
                    l.b[r] -= lr * *gb.get(r).unwrap_or(&0.0);
                }
            }
        }
    }

    Some(
        params
            .into_iter()
            .filter_map(|p| match p {
                Param::Lin(l) => Some(l),
                _ => None,
            })
            .collect(),
    )
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
    let input_dim = data[0].0.len();
    if data.iter().any(|(x, _)| x.len() != input_dim) {
        return Ok(unevaluated("NetTrain", args));
    }

    let trained = match train(
        &train_layers,
        loss.as_ref(),
        &data,
        input_dim,
        rounds,
        lr,
        env,
    ) {
        Some(t) => t,
        None => return Ok(unevaluated("NetTrain", args)),
    };

    // Rebuild the net with the trained weights written back.
    let mut queue = trained.into_iter();
    let rebuilt = match net_type(net) {
        Some("Chain") => {
            let layers = as_list(obj_fields(net).unwrap().get("Layers").unwrap())
                .cloned()
                .unwrap_or_default();
            let new_layers: Vec<Value> = layers
                .into_iter()
                .map(|l| {
                    if is_linear(&l) {
                        match queue.next() {
                            Some(lin) => lin_to_fields(&l, &lin),
                            None => l,
                        }
                    } else {
                        l
                    }
                })
                .collect();
            make_net(&[
                ("Type", Value::Str("Chain".to_string())),
                ("Layers", Value::List(new_layers)),
            ])
        }
        Some("Linear") => match queue.next() {
            Some(lin) => lin_to_fields(net, &lin),
            None => net.clone(),
        },
        _ => net.clone(),
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

    fn field(v: &Value, key: &str) -> Value {
        match v {
            Value::Object { fields, .. } => fields.get(key).cloned().unwrap(),
            other => panic!("not a net object: {other:?}"),
        }
    }

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
        // `{n}` form.
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
    fn test_elementwise_tanh() {
        let v = eval_str("ElementwiseLayer[Tanh][{0.}]");
        assert!((as_vec(&v)[0] - 0.0).abs() < 1e-9);
        let v2 = eval_str("ElementwiseLayer[LogisticSigmoid][{0.}]");
        assert!((as_vec(&v2)[0] - 0.5).abs() < 1e-9);
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

    #[test]
    fn test_mse_loss_zero() {
        let v = num("MeanSquaredLossLayer[][{{1., 2.}, {1., 2.}}]");
        assert!(v.abs() < 1e-12);
    }

    #[test]
    fn test_mse_loss_value() {
        // mean((1-0)^2 + (2-0)^2)/2? -> mean over 2 entries of squares = (1+4)/2
        let v = num("MeanSquaredLossLayer[][{{1., 2.}, {0., 0.}}]");
        assert!((v - 2.5).abs() < 1e-9, "got {v}");
    }

    #[test]
    fn test_ce_loss_value() {
        // -(1*ln 0.5 + 0*ln 0.5)/2 = ln 2 / 2
        let v = num("CrossEntropyLossLayer[][{{0.5, 0.5}, {1., 0.}}]");
        let expected = 2f64.ln() / 2.0;
        assert!((v - expected).abs() < 1e-9, "got {v}");
    }

    #[test]
    fn test_net_train_fits_line() {
        // Fit y = 2x through the origin with a single dense layer.
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
    fn test_debug_train() {
        let v = eval_str(
            "SeedRandom[1]; net = NetTrain[NetChain[{LinearLayer[{1, 1}]}], {{0.} -> {0.}, {1.} -> {2.}}]; {net, net[{3.}]}",
        );
        eprintln!("DEBUG TRAIN => {v:?}");
        let v2 = eval_str("NetTrain[LinearLayer[{1, 1}], {{0.} -> {0.}}]");
        eprintln!("DEBUG SIMPLE => {v2:?}");
    }

    #[test]
    fn test_field_access() {
        let v = eval_str("LinearLayer[5][\"Out\"]");
        assert_eq!(to_f64(&v), Some(5.0));
    }
}
