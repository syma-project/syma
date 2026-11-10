//! time series: TimeSeries, TemporalData, EventSeries, MovingMap,
//! TimeSeriesWindow, TimeSeriesResample, moving statistics.
//!
//! Gap-fill module; see the individual builtin docs for semantics.
//!
//! # Representation
//!
//! A time series is represented as an ordinary [`Value::Call`] with head
//! `"TimeSeries"`, `"TemporalData"`, or `"EventSeries"` whose single argument
//! is a [`Value::List`] of `{t, v}` pairs (each pair being a two-element
//! [`Value::List`]). For example:
//!
//! ```text
//! TimeSeries[{{1, 10}, {2, 20}, {3, 30}}]
//! ```
//!
//! This mirrors Wolfram Language's `TemporalData[{{t1,v1}, ...}]` container:
//! the series stays a fully inspectable symbolic expression, so pattern
//! matching, `Head`, `Normal`, `Values`, formatting, etc. all work on it.
//! Constructing a series from a flat list of values assigns automatic times
//! `1, 2, 3, ...` (matching `TimeSeries[{v1, v2, ...}]`).
//!
//! Every transformation is best-effort: when an argument is not a recognized
//! series (or a value is non-numeric where numbers are required) the call is
//! returned unevaluated rather than raising.

use rug::Integer;

use crate::env::Env;
use crate::eval::apply_function;
use crate::value::{EvalError, Value};

/// Heads that are recognized as time-series containers.
const SERIES_HEADS: &[&str] = &["TimeSeries", "TemporalData", "EventSeries"];

// ── Small helpers ───────────────────────────────────────────────────────────

/// Build an unevaluated `head[args...]` result.
fn uneval(name: &str, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Call {
        head: name.to_string(),
        args: args.to_vec(),
    })
}

fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

/// Return an exact `Integer` when `f` is integral, otherwise a `Real`.
fn num(f: f64) -> Value {
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 9.0e15 {
        int(f as i64)
    } else {
        super::real(f)
    }
}

/// True when `v` is a call with one of the recognized series heads.
fn is_series(v: &Value) -> bool {
    matches!(v, Value::Call { head, .. } if SERIES_HEADS.contains(&head.as_str()))
}

/// If `v` is a well-formed series, return its head and pair list.
fn as_series(v: &Value) -> Option<(&str, &Vec<Value>)> {
    if let Value::Call { head, args } = v
        && SERIES_HEADS.contains(&head.as_str())
        && args.len() == 1
        && let Value::List(pairs) = &args[0]
    {
        Some((head.as_str(), pairs))
    } else {
        None
    }
}

/// Build a series call from its head and `{t, v}` pairs.
fn make_series(head: &str, pairs: Vec<Value>) -> Value {
    Value::Call {
        head: head.to_string(),
        args: vec![Value::List(pairs)],
    }
}

/// Split a pair into its (time, value) halves.
fn pair_fields(p: &Value) -> Option<(&Value, &Value)> {
    if let Value::List(fields) = p
        && fields.len() == 2
    {
        Some((&fields[0], &fields[1]))
    } else {
        None
    }
}

fn pair_time(p: &Value) -> Option<&Value> {
    pair_fields(p).map(|(t, _)| t)
}

fn pair_value(p: &Value) -> Option<&Value> {
    pair_fields(p).map(|(_, v)| v)
}

/// A `{t, v}` two-element list counts as a data point.
fn looks_like_pair(v: &Value) -> bool {
    matches!(v, Value::List(p) if p.len() == 2) || matches!(v, Value::Rule { .. })
}

/// Normalize a pair-like value into a `{t, v}` list.
fn pair_from(v: &Value) -> Option<Value> {
    match v {
        Value::List(p) if p.len() == 2 => Some(v.clone()),
        Value::Rule { lhs, rhs, .. } => Some(Value::List(vec![(**lhs).clone(), (**rhs).clone()])),
        _ => None,
    }
}

/// Extract `{t, v}` pairs from raw data.
///
/// Accepts an existing series (recursing into its payload), a list of pairs or
/// rules, or a flat list of values (to which automatic times `1..n` are
/// assigned). Returns `None` for anything else.
fn pairs_from_data(v: &Value) -> Option<Vec<Value>> {
    if let Value::Call { head, args } = v
        && SERIES_HEADS.contains(&head.as_str())
        && args.len() == 1
    {
        return pairs_from_data(&args[0]);
    }
    let Value::List(items) = v else {
        return None;
    };
    if items.is_empty() {
        return Some(Vec::new());
    }
    if items.iter().all(looks_like_pair) {
        return items.iter().map(pair_from).collect();
    }
    Some(
        items
            .iter()
            .enumerate()
            .map(|(i, val)| Value::List(vec![int(i as i64 + 1), val.clone()]))
            .collect(),
    )
}

/// Numeric time of a pair, if present.
fn time_f64(p: &Value) -> Option<f64> {
    pair_time(p).and_then(super::to_f64)
}

/// Mean of a window of values (as a numeric value).
fn mean_value(window: &[Value]) -> Value {
    if window.is_empty() {
        return super::real(f64::NAN);
    }
    let sum: f64 = window.iter().filter_map(super::to_f64).sum();
    num(sum / window.len() as f64)
}

/// Median of a window of values.
fn median_value(window: &[Value]) -> Value {
    let mut nums: Vec<f64> = window.iter().filter_map(super::to_f64).collect();
    if nums.is_empty() {
        return super::real(f64::NAN);
    }
    nums.sort_by(f64::total_cmp);
    let k = nums.len();
    if k % 2 == 1 {
        num(nums[k / 2])
    } else {
        num((nums[k / 2 - 1] + nums[k / 2]) / 2.0)
    }
}

/// Apply `f` to every length-`n` window of a series, keeping the time of each
/// window's last element (Wolfram aligns moving results to right endpoints).
fn series_sliding(pairs: &[Value], n: usize, f: impl Fn(&[Value]) -> Value) -> Vec<Value> {
    if n == 0 || pairs.len() < n {
        return Vec::new();
    }
    pairs
        .windows(n)
        .map(|window| {
            let vals: Vec<Value> = window.iter().filter_map(pair_value).cloned().collect();
            let t = window
                .last()
                .and_then(pair_time)
                .cloned()
                .unwrap_or(Value::Null);
            Value::List(vec![t, f(&vals)])
        })
        .collect()
}

// ── Constructors ────────────────────────────────────────────────────────────

/// Shared constructor for the series-container heads.
fn construct(head: &str, args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return uneval(head, args);
    }
    match pairs_from_data(&args[0]) {
        Some(pairs) => Ok(make_series(head, pairs)),
        None => uneval(head, args),
    }
}

/// TimeSeries[data] — a time series. A flat value list gets automatic times.
pub fn builtin_time_series(args: &[Value]) -> Result<Value, EvalError> {
    construct("TimeSeries", args)
}

/// TemporalData[data] — the general temporal container.
pub fn builtin_temporal_data(args: &[Value]) -> Result<Value, EvalError> {
    construct("TemporalData", args)
}

/// EventSeries[data] — a series of event values (counts).
pub fn builtin_event_series(args: &[Value]) -> Result<Value, EvalError> {
    construct("EventSeries", args)
}

// ── Moving statistics ───────────────────────────────────────────────────────

/// MovingAverage[series, n] — series of window means. On a plain list this
/// falls through to the list implementation.
pub fn builtin_moving_average(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 2
        && let Some((head, pairs)) = as_series(&args[0])
    {
        let n = match args[1].to_integer() {
            Some(n) if n > 0 => n as usize,
            _ => return uneval("MovingAverage", args),
        };
        let head = head.to_string();
        let out = series_sliding(pairs, n, mean_value);
        return Ok(make_series(&head, out));
    }
    super::list::builtin_moving_average(args)
}

/// MovingMedian[series, n] — series of window medians; also works on lists.
pub fn builtin_moving_median(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return uneval("MovingMedian", args);
    }
    let n = match args[1].to_integer() {
        Some(n) if n > 0 => n as usize,
        _ => return uneval("MovingMedian", args),
    };
    if let Some((head, pairs)) = as_series(&args[0]) {
        let head = head.to_string();
        return Ok(make_series(&head, series_sliding(pairs, n, median_value)));
    }
    let Value::List(items) = &args[0] else {
        return uneval("MovingMedian", args);
    };
    if items.len() < n {
        return Ok(Value::List(Vec::new()));
    }
    Ok(Value::List(items.windows(n).map(median_value).collect()))
}

/// MovingMap[f, series, n] / [f, series, n, align] — map `f` over every
/// length-`n` window. `align` (`Right` default, `Left`, `Center`, or a 1-based
/// element index) selects which window element supplies the result's time.
pub fn builtin_moving_map(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 && args.len() != 4 {
        return uneval("MovingMap", args);
    }
    let n = match args[2].to_integer() {
        Some(n) if n > 0 => n as usize,
        _ => return uneval("MovingMap", args),
    };
    let align = if args.len() == 4 {
        match Align::from_value(&args[3]) {
            Some(a) => a,
            None => return uneval("MovingMap", args),
        }
    } else {
        Align::Right
    };
    let head = as_series(&args[1])
        .map(|(h, _)| h.to_string())
        .unwrap_or_else(|| "TimeSeries".to_string());
    let Some(pairs) = pairs_from_data(&args[1]) else {
        return uneval("MovingMap", args);
    };
    if pairs.len() < n {
        return Ok(make_series(&head, Vec::new()));
    }
    let mut out = Vec::with_capacity(pairs.len() - n + 1);
    for window in pairs.windows(n) {
        let vals: Vec<Value> = window.iter().filter_map(pair_value).cloned().collect();
        let r = apply_function(&args[0], &[Value::List(vals)], env)?;
        let t = pair_time(&window[align.index(n)])
            .cloned()
            .unwrap_or(Value::Null);
        out.push(Value::List(vec![t, r]));
    }
    Ok(make_series(&head, out))
}

// ── Windowing / resampling ──────────────────────────────────────────────────

/// TimeSeriesWindow[series, {tmin, tmax}] — restrict to `tmin <= t <= tmax`.
pub fn builtin_time_series_window(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return uneval("TimeSeriesWindow", args);
    }
    let Some((head, pairs)) = as_series(&args[0]) else {
        return uneval("TimeSeriesWindow", args);
    };
    let Value::List(bounds) = &args[1] else {
        return uneval("TimeSeriesWindow", args);
    };
    if bounds.len() != 2 {
        return uneval("TimeSeriesWindow", args);
    }
    let (Some(lo), Some(hi)) = (super::to_f64(&bounds[0]), super::to_f64(&bounds[1])) else {
        return uneval("TimeSeriesWindow", args);
    };
    let head = head.to_string();
    let out = pairs
        .iter()
        .filter(|p| time_f64(p).is_some_and(|t| t >= lo && t <= hi))
        .cloned()
        .collect();
    Ok(make_series(&head, out))
}

/// TimeSeriesResample[series, dt] — nearest-neighbour resampling onto the
/// regular grid `min(t), min(t)+dt, ...` up to the last time.
pub fn builtin_time_series_resample(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return uneval("TimeSeriesResample", args);
    }
    let Some((head, pairs)) = as_series(&args[0]) else {
        return uneval("TimeSeriesResample", args);
    };
    let Some(dt) = super::to_f64(&args[1]).filter(|d| *d > 0.0) else {
        return uneval("TimeSeriesResample", args);
    };
    let times: Vec<f64> = pairs.iter().filter_map(time_f64).collect();
    if times.len() != pairs.len() || times.is_empty() {
        return uneval("TimeSeriesResample", args);
    }
    let t0 = times[0];
    let t1 = times[times.len() - 1];
    if t1 < t0 {
        return uneval("TimeSeriesResample", args);
    }
    let head = head.to_string();
    let steps = ((t1 - t0) / dt).floor() as i64;
    let mut out = Vec::with_capacity(steps as usize + 1);
    for k in 0..=steps {
        let gt = t0 + k as f64 * dt;
        let idx = (0..times.len())
            .min_by(|&i, &j| (times[i] - gt).abs().total_cmp(&(times[j] - gt).abs()))
            .expect("non-empty times");
        let v = pair_value(&pairs[idx]).cloned().unwrap_or(Value::Null);
        out.push(Value::List(vec![num(gt), v]));
    }
    Ok(make_series(&head, out))
}

// ── Accessors ───────────────────────────────────────────────────────────────

/// Normal[series] — the underlying `{t, v}` pairs. Falls through to the
/// association/dataset implementation for other argument types.
pub fn builtin_normal(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 1
        && is_series(&args[0])
        && let Some(pairs) = pairs_from_data(&args[0])
    {
        return Ok(Value::List(pairs));
    }
    if args.len() == 1
        && super::linalgmore::is_sparse(&args[0])
        && let Some(dense) = super::linalgmore::normal_dense(&args[0])
    {
        return Ok(dense);
    }
    super::association::builtin_normal(args)
}

/// Values[series] — the value components. Falls through to the association
/// implementation for other argument types.
pub fn builtin_values(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 1
        && is_series(&args[0])
        && let Some(pairs) = pairs_from_data(&args[0])
    {
        return Ok(Value::List(
            pairs.iter().filter_map(pair_value).cloned().collect(),
        ));
    }
    super::association::builtin_values(args)
}

// ── Alignment ───────────────────────────────────────────────────────────────

/// Which element of a sliding window a moving result is anchored to.
#[derive(Clone, Copy)]
enum Align {
    /// Anchor to the window's last element (Wolfram default).
    Right,
    /// Anchor to the window's first element.
    Left,
    /// Anchor to the window's middle element.
    Center,
    /// Anchor to a 1-based element index, clamped to the window.
    Index(usize),
}

impl Align {
    fn from_value(v: &Value) -> Option<Align> {
        match v {
            Value::Symbol(s) => match s.as_str() {
                "Right" => Some(Align::Right),
                "Left" => Some(Align::Left),
                "Center" => Some(Align::Center),
                _ => None,
            },
            Value::Integer(k) if *k >= 1 => Some(Align::Index(k.to_usize().unwrap_or(1))),
            _ => None,
        }
    }

    fn index(self, len: usize) -> usize {
        match self {
            Align::Right => len - 1,
            Align::Left => 0,
            Align::Center => len / 2,
            Align::Index(k) => (k - 1).min(len - 1),
        }
    }
}

// ── Threading / mapping ─────────────────────────────────────────────────────

/// TimeSeriesThread[f, {ts1, ts2, ...}] — combine series pointwise. Only times
/// present in every series are kept, and `f` is applied to the sequence of
/// values at each such time.
pub fn builtin_time_series_thread(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return uneval("TimeSeriesThread", args);
    }
    let Value::List(sers) = &args[1] else {
        return uneval("TimeSeriesThread", args);
    };
    if sers.is_empty() {
        return uneval("TimeSeriesThread", args);
    }
    let head = as_series(&sers[0])
        .map(|(h, _)| h.to_string())
        .unwrap_or_else(|| "TimeSeries".to_string());
    let mut all: Vec<Vec<Value>> = Vec::with_capacity(sers.len());
    for s in sers {
        match pairs_from_data(s) {
            Some(pairs) => all.push(pairs),
            None => return uneval("TimeSeriesThread", args),
        }
    }
    let mut out = Vec::new();
    for p in &all[0] {
        let Some(t) = pair_time(p).cloned() else {
            continue;
        };
        let mut vals = Vec::with_capacity(all.len());
        let mut present = true;
        for other in &all {
            match other.iter().find(|q| pair_time(q) == Some(&t)) {
                Some(q) => vals.push(pair_value(q).cloned().unwrap_or(Value::Null)),
                None => {
                    present = false;
                    break;
                }
            }
        }
        if present {
            let r = apply_function(&args[0], &vals, env)?;
            out.push(Value::List(vec![t, r]));
        }
    }
    out.sort_by(time_cmp);
    Ok(make_series(&head, out))
}

/// TimeSeriesMap[f, series] — apply `f` to each value, keeping the times.
pub fn builtin_time_series_map(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return uneval("TimeSeriesMap", args);
    }
    let head = as_series(&args[1])
        .map(|(h, _)| h.to_string())
        .unwrap_or_else(|| "TimeSeries".to_string());
    let Some(pairs) = pairs_from_data(&args[1]) else {
        return uneval("TimeSeriesMap", args);
    };
    let mut out = Vec::with_capacity(pairs.len());
    for p in &pairs {
        let Some(t) = pair_time(p).cloned() else {
            continue;
        };
        let v = pair_value(p).cloned().unwrap_or(Value::Null);
        let r = apply_function(&args[0], &[v], env)?;
        out.push(Value::List(vec![t, r]));
    }
    Ok(make_series(&head, out))
}

// ── Mutation-style edits ────────────────────────────────────────────────────

/// Order two pairs by their numeric time (non-numeric times compare equal).
fn time_cmp(a: &Value, b: &Value) -> std::cmp::Ordering {
    match (time_f64(a), time_f64(b)) {
        (Some(x), Some(y)) => x.total_cmp(&y),
        _ => std::cmp::Ordering::Equal,
    }
}

/// TimeSeriesInsert[series, {t, v}] — insert or overwrite the point at time `t`.
pub fn builtin_time_series_insert(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return uneval("TimeSeriesInsert", args);
    }
    let Some((head, pairs)) = as_series(&args[0]) else {
        return uneval("TimeSeriesInsert", args);
    };
    let Some(point) = pair_from(&args[1]) else {
        return uneval("TimeSeriesInsert", args);
    };
    let head = head.to_string();
    let Some(t) = pair_time(&point).cloned() else {
        return uneval("TimeSeriesInsert", args);
    };
    let mut out: Vec<Value> = pairs.clone();
    if let Some(pos) = out.iter().position(|p| pair_time(p) == Some(&t)) {
        out[pos] = point;
    } else {
        out.push(point);
        out.sort_by(time_cmp);
    }
    Ok(make_series(&head, out))
}

/// TimeSeriesDelete[series, t] (or a list of times) — drop points at time `t`.
pub fn builtin_time_series_delete(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return uneval("TimeSeriesDelete", args);
    }
    let Some((head, pairs)) = as_series(&args[0]) else {
        return uneval("TimeSeriesDelete", args);
    };
    let targets: Vec<&Value> = match &args[1] {
        Value::List(ts) => ts.iter().collect(),
        other => vec![other],
    };
    let head = head.to_string();
    let out = pairs
        .iter()
        .filter(|p| !targets.iter().any(|t| pair_time(p) == Some(*t)))
        .cloned()
        .collect();
    Ok(make_series(&head, out))
}

/// TimeSeriesShift[series, dt] — translate every time by `dt`.
pub fn builtin_time_series_shift(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return uneval("TimeSeriesShift", args);
    }
    let Some((head, pairs)) = as_series(&args[0]) else {
        return uneval("TimeSeriesShift", args);
    };
    let Some(dt) = super::to_f64(&args[1]) else {
        return uneval("TimeSeriesShift", args);
    };
    let times: Vec<f64> = pairs.iter().filter_map(time_f64).collect();
    if times.len() != pairs.len() {
        return uneval("TimeSeriesShift", args);
    }
    let head = head.to_string();
    let out = pairs
        .iter()
        .zip(times)
        .map(|(p, t)| {
            Value::List(vec![
                num(t + dt),
                pair_value(p).cloned().unwrap_or(Value::Null),
            ])
        })
        .collect();
    Ok(make_series(&head, out))
}

/// TimeSeriesRescale[series, {tmin, tmax}] — linearly map the time range onto
/// `[tmin, tmax]`.
pub fn builtin_time_series_rescale(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return uneval("TimeSeriesRescale", args);
    }
    let Some((head, pairs)) = as_series(&args[0]) else {
        return uneval("TimeSeriesRescale", args);
    };
    let Value::List(bounds) = &args[1] else {
        return uneval("TimeSeriesRescale", args);
    };
    if bounds.len() != 2 {
        return uneval("TimeSeriesRescale", args);
    }
    let (Some(lo), Some(hi)) = (super::to_f64(&bounds[0]), super::to_f64(&bounds[1])) else {
        return uneval("TimeSeriesRescale", args);
    };
    let times: Vec<f64> = pairs.iter().filter_map(time_f64).collect();
    if times.len() != pairs.len() || times.is_empty() {
        return uneval("TimeSeriesRescale", args);
    }
    let tmin = times.iter().copied().fold(f64::INFINITY, f64::min);
    let tmax = times.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let head = head.to_string();
    let out = pairs
        .iter()
        .zip(times)
        .map(|(p, t)| {
            let nt = if (tmax - tmin).abs() < f64::EPSILON {
                lo
            } else {
                lo + (t - tmin) / (tmax - tmin) * (hi - lo)
            };
            Value::List(vec![num(nt), pair_value(p).cloned().unwrap_or(Value::Null)])
        })
        .collect();
    Ok(make_series(&head, out))
}

// ── Correlation lists ───────────────────────────────────────────────────────

/// Autocorrelations of `values` for lags `0..=kmax`, normalized by the sum of
/// squared deviations (so lag 0 is 1).
fn autocorrelations(values: &[f64], kmax: usize) -> Vec<f64> {
    let n = values.len();
    let mean = values.iter().sum::<f64>() / n as f64;
    let denom: f64 = values.iter().map(|x| (x - mean) * (x - mean)).sum();
    (0..=kmax)
        .map(|l| {
            if l == 0 {
                1.0
            } else if denom == 0.0 {
                0.0
            } else {
                (0..n - l)
                    .map(|i| (values[i] - mean) * (values[i + l] - mean))
                    .sum::<f64>()
                    / denom
            }
        })
        .collect()
}

/// Partial autocorrelations via the Durbin–Levinson recursion.
fn partial_autocorrelations(values: &[f64], kmax: usize) -> Vec<f64> {
    let r = autocorrelations(values, kmax);
    let mut prev = vec![0.0f64; kmax + 1];
    let mut out = vec![1.0f64];
    for k in 1..=kmax {
        let mut numer = r[k];
        let mut denom = 1.0;
        for j in 1..k {
            numer -= prev[j] * r[k - j];
            denom -= prev[j] * r[j];
        }
        let phikk = if denom.abs() < f64::EPSILON {
            0.0
        } else {
            numer / denom
        };
        out.push(phikk);
        let mut phi = vec![0.0f64; kmax + 1];
        for j in 1..k {
            phi[j] = prev[j] - phikk * prev[k - j];
        }
        phi[k] = phikk;
        prev = phi;
    }
    out
}

/// Parse a series (or list) into its numeric value sequence.
fn series_values(v: &Value) -> Option<Vec<f64>> {
    let pairs = pairs_from_data(v)?;
    pairs
        .iter()
        .map(|p| pair_value(p).and_then(super::to_f64))
        .collect()
}

fn parse_kmax(v: Option<&Value>, n: usize) -> Option<usize> {
    match v {
        None => Some(n.saturating_sub(1)),
        Some(x) => x.to_integer().and_then(|k| {
            if k < 0 {
                None
            } else {
                Some((k as usize).min(n.saturating_sub(1)))
            }
        }),
    }
}

/// AutocorrelationList[series] / [series, kmax] — lag-`0..kmax` autocorrelations.
pub fn builtin_autocorrelation_list(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return uneval("AutocorrelationList", args);
    }
    let Some(values) = series_values(&args[0]) else {
        return uneval("AutocorrelationList", args);
    };
    if values.len() < 2 {
        return uneval("AutocorrelationList", args);
    }
    let Some(kmax) = parse_kmax(args.get(1), values.len()) else {
        return uneval("AutocorrelationList", args);
    };
    Ok(Value::List(
        autocorrelations(&values, kmax)
            .into_iter()
            .map(num)
            .collect(),
    ))
}

/// PartialCorrelationList[series] / [series, kmax] — partial autocorrelations.
pub fn builtin_partial_correlation_list(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return uneval("PartialCorrelationList", args);
    }
    let Some(values) = series_values(&args[0]) else {
        return uneval("PartialCorrelationList", args);
    };
    if values.len() < 2 {
        return uneval("PartialCorrelationList", args);
    }
    let Some(kmax) = parse_kmax(args.get(1), values.len()) else {
        return uneval("PartialCorrelationList", args);
    };
    Ok(Value::List(
        partial_autocorrelations(&values, kmax)
            .into_iter()
            .map(num)
            .collect(),
    ))
}

// ── Linear processes ────────────────────────────────────────────────────────

/// A coefficient argument is a list of numbers, or a single number.
fn coef_list(v: &Value) -> Option<Vec<f64>> {
    match v {
        Value::List(items) => items.iter().map(super::to_f64).collect(),
        other => super::to_f64(other).map(|x| vec![x]),
    }
}

/// Draw one standard-normal variate via the Box–Muller transform.
fn std_normal(rng: &mut fastrand::Rng) -> f64 {
    let u1 = rng.f64().max(f64::MIN_POSITIVE);
    let u2 = rng.f64();
    (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
}

/// Build a `TimeSeries` of `values` sampled at times `1..=n`.
fn sample_series(values: Vec<f64>) -> Value {
    let pairs = values
        .into_iter()
        .enumerate()
        .map(|(i, x)| Value::List(vec![int(i as i64 + 1), num(x)]))
        .collect();
    make_series("TimeSeries", pairs)
}

/// Parse the `(variance, n)` tail shared by the process samplers.
fn parse_process_tail(args: &[Value]) -> Option<(f64, usize)> {
    let v = super::to_f64(&args[args.len() - 2]).filter(|v| *v >= 0.0)?;
    let n = args[args.len() - 1]
        .to_integer()
        .filter(|n| *n > 0)
        .map(|n| n as usize)?;
    Some((v, n))
}

/// ARProcess[{a1, ..., ap}, variance, n] — sample an AR(p) process.
pub fn builtin_ar_process(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return uneval("ARProcess", args);
    }
    let Some(a) = coef_list(&args[0]) else {
        return uneval("ARProcess", args);
    };
    let Some((v, n)) = parse_process_tail(args) else {
        return uneval("ARProcess", args);
    };
    let sd = v.sqrt();
    let values = super::random::with_rng(|rng| {
        let mut xs = vec![0.0f64; n];
        for t in 0..n {
            let mut val = sd * std_normal(rng);
            for (j, &aj) in a.iter().enumerate() {
                if t > j {
                    val += aj * xs[t - 1 - j];
                }
            }
            xs[t] = val;
        }
        xs
    });
    Ok(sample_series(values))
}

/// MAProcess[{b1, ..., bq}, variance, n] — sample an MA(q) process.
pub fn builtin_ma_process(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return uneval("MAProcess", args);
    }
    let Some(b) = coef_list(&args[0]) else {
        return uneval("MAProcess", args);
    };
    let Some((v, n)) = parse_process_tail(args) else {
        return uneval("MAProcess", args);
    };
    let sd = v.sqrt();
    let values = super::random::with_rng(|rng| {
        let e: Vec<f64> = (0..n).map(|_| sd * std_normal(rng)).collect();
        let mut xs = vec![0.0f64; n];
        for t in 0..n {
            let mut val = e[t];
            for (j, &bj) in b.iter().enumerate() {
                if t > j {
                    val += bj * e[t - 1 - j];
                }
            }
            xs[t] = val;
        }
        xs
    });
    Ok(sample_series(values))
}

/// ARMAProcess[{a1, ...}, {b1, ...}, variance, n] — sample an ARMA(p, q).
pub fn builtin_arma_process(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 4 {
        return uneval("ARMAProcess", args);
    }
    let (Some(a), Some(b)) = (coef_list(&args[0]), coef_list(&args[1])) else {
        return uneval("ARMAProcess", args);
    };
    let Some((v, n)) = parse_process_tail(args) else {
        return uneval("ARMAProcess", args);
    };
    let sd = v.sqrt();
    let values = super::random::with_rng(|rng| {
        let mut es = vec![0.0f64; n];
        let mut xs = vec![0.0f64; n];
        for t in 0..n {
            let et = sd * std_normal(rng);
            es[t] = et;
            let mut val = et;
            for (j, &aj) in a.iter().enumerate() {
                if t > j {
                    val += aj * xs[t - 1 - j];
                }
            }
            for (j, &bj) in b.iter().enumerate() {
                if t > j {
                    val += bj * es[t - 1 - j];
                }
            }
            xs[t] = val;
        }
        xs
    });
    Ok(sample_series(values))
}

// ── Statistics / accessor wrappers ──────────────────────────────────────────

/// Mean[series] — arithmetic mean of the values; otherwise the list/distribution
/// implementation.
pub fn builtin_mean(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 1
        && is_series(&args[0])
        && let Some(pairs) = pairs_from_data(&args[0])
    {
        let vals: Vec<Value> = pairs.iter().filter_map(pair_value).cloned().collect();
        if vals.is_empty() {
            return uneval("Mean", args);
        }
        return super::statistics::builtin_mean(&[Value::List(vals)]);
    }
    super::statistics::builtin_mean(args)
}

/// Variance[series] — sample variance of the values; otherwise the
/// list/distribution implementation.
pub fn builtin_variance(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 1
        && is_series(&args[0])
        && let Some(pairs) = pairs_from_data(&args[0])
    {
        let vals: Vec<Value> = pairs.iter().filter_map(pair_value).cloned().collect();
        if vals.is_empty() {
            return uneval("Variance", args);
        }
        return super::statistics::builtin_variance(&[Value::List(vals)]);
    }
    super::statistics::builtin_variance(args)
}

/// DateList[series] — the times of a series; otherwise the calendar
/// implementation.
pub fn builtin_date_list(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 1
        && is_series(&args[0])
        && let Some(pairs) = pairs_from_data(&args[0])
    {
        return Ok(Value::List(
            pairs.iter().filter_map(pair_time).cloned().collect(),
        ));
    }
    super::calendar::builtin_date_list(args)
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    use super::{register_builtin, register_builtin_env};
    register_builtin(env, "TimeSeries", builtin_time_series);
    register_builtin(env, "TemporalData", builtin_temporal_data);
    register_builtin(env, "EventSeries", builtin_event_series);
    register_builtin(env, "MovingAverage", builtin_moving_average);
    register_builtin(env, "MovingMedian", builtin_moving_median);
    register_builtin(env, "TimeSeriesWindow", builtin_time_series_window);
    register_builtin(env, "TimeSeriesResample", builtin_time_series_resample);
    register_builtin(env, "TimeSeriesInsert", builtin_time_series_insert);
    register_builtin(env, "TimeSeriesDelete", builtin_time_series_delete);
    register_builtin(env, "TimeSeriesShift", builtin_time_series_shift);
    register_builtin(env, "TimeSeriesRescale", builtin_time_series_rescale);
    register_builtin(env, "AutocorrelationList", builtin_autocorrelation_list);
    register_builtin(
        env,
        "PartialCorrelationList",
        builtin_partial_correlation_list,
    );
    register_builtin(env, "ARProcess", builtin_ar_process);
    register_builtin(env, "MAProcess", builtin_ma_process);
    register_builtin(env, "ARMAProcess", builtin_arma_process);
    // `Normal`/`Values`/`Mean`/`Variance`/`DateList` overlap other modules;
    // these registrations run last so the series-aware wrappers take
    // precedence, while still delegating non-series arguments.
    register_builtin(env, "Normal", builtin_normal);
    register_builtin(env, "Values", builtin_values);
    register_builtin(env, "Mean", builtin_mean);
    register_builtin(env, "Variance", builtin_variance);
    register_builtin(env, "DateList", builtin_date_list);
    register_builtin_env(env, "MovingMap", builtin_moving_map);
    register_builtin_env(env, "TimeSeriesThread", builtin_time_series_thread);
    register_builtin_env(env, "TimeSeriesMap", builtin_time_series_map);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::builtins;
    use crate::lexer;
    use crate::parser;

    fn eval_str(input: &str) -> Value {
        std::thread::Builder::new()
            .stack_size(16 * 1024 * 1024)
            .spawn({
                let input = input.to_string();
                move || {
                    let env = Env::new();
                    builtins::register_builtins(&env);
                    let tokens = lexer::tokenize(&input).unwrap();
                    let ast = parser::parse(tokens).unwrap();
                    crate::eval::eval_program(&ast, &env).unwrap()
                }
            })
            .unwrap()
            .join()
            .unwrap()
    }

    fn iv(n: i64) -> Value {
        int(n)
    }

    fn lst(items: Vec<Value>) -> Value {
        Value::List(items)
    }

    fn pair(t: i64, v: i64) -> Value {
        lst(vec![iv(t), iv(v)])
    }

    fn series(pairs: Vec<(i64, i64)>) -> Value {
        make_series(
            "TimeSeries",
            pairs.into_iter().map(|(t, v)| pair(t, v)).collect(),
        )
    }

    fn as_f(v: &Value) -> f64 {
        super::super::to_f64(v).unwrap()
    }

    fn pairs_of(v: &Value) -> Vec<(f64, f64)> {
        match v {
            Value::Call { head, args } if head == "TimeSeries" => match &args[0] {
                Value::List(ps) => ps
                    .iter()
                    .map(|p| match pair_fields(p) {
                        Some((t, val)) => (as_f(t), as_f(val)),
                        None => panic!("malformed pair: {p}"),
                    })
                    .collect(),
                other => panic!("expected pair list, got {other}"),
            },
            other => panic!("expected TimeSeries, got {other}"),
        }
    }

    fn approx(a: f64, b: f64) {
        assert!((a - b).abs() < 1e-9, "expected {b}, got {a}");
    }

    // ── Representation / constructors ──

    #[test]
    fn test_time_series_automatic_times() {
        let v = eval_str("TimeSeries[{10, 20, 30}]");
        assert!(matches!(&v, Value::Call { head, .. } if head == "TimeSeries"));
        assert_eq!(
            pairs_of(&v)
                .into_iter()
                .map(|(_, val)| val)
                .collect::<Vec<_>>(),
            vec![10.0, 20.0, 30.0]
        );
        // Automatic times are 1, 2, 3.
        assert_eq!(
            pairs_of(&v).into_iter().map(|(t, _)| t).collect::<Vec<_>>(),
            vec![1.0, 2.0, 3.0]
        );
    }

    #[test]
    fn test_time_series_explicit_pairs() {
        let v = eval_str("TimeSeries[{{1, 10}, {2, 20}}]");
        assert_eq!(pairs_of(&v), vec![(1.0, 10.0), (2.0, 20.0)]);
    }

    #[test]
    fn test_time_series_from_rules() {
        let v = builtin_time_series(&[lst(vec![
            Value::Rule {
                lhs: Box::new(iv(1)),
                rhs: Box::new(iv(7)),
                delayed: false,
            },
            Value::Rule {
                lhs: Box::new(iv(2)),
                rhs: Box::new(iv(8)),
                delayed: false,
            },
        ])])
        .unwrap();
        assert_eq!(pairs_of(&v), vec![(1.0, 7.0), (2.0, 8.0)]);
    }

    #[test]
    fn test_time_series_idempotent() {
        let v = eval_str("TimeSeries[TimeSeries[{{1, 5}, {2, 6}}]]");
        assert_eq!(pairs_of(&v), vec![(1.0, 5.0), (2.0, 6.0)]);
    }

    #[test]
    fn test_temporal_data_head() {
        let v = eval_str("TemporalData[{3, 4}]");
        assert!(matches!(&v, Value::Call { head, .. } if head == "TemporalData"));
    }

    #[test]
    fn test_event_series_head() {
        let v = eval_str("EventSeries[{0, 1, 1, 0}]");
        assert!(matches!(&v, Value::Call { head, .. } if head == "EventSeries"));
    }

    #[test]
    fn test_constructor_inapplicable() {
        let v = eval_str("TimeSeries[z]");
        assert!(
            matches!(&v, Value::Call { head, args } if head == "TimeSeries" && args.len() == 1)
        );
    }

    // ── Accessors ──

    #[test]
    fn test_normal_accessor() {
        let v = eval_str("Normal[TimeSeries[{{1, 10}, {2, 20}}]]");
        match v {
            Value::List(items) => {
                assert_eq!(items.len(), 2);
                assert_eq!(items[0], pair(1, 10));
            }
            other => panic!("expected list, got {other}"),
        }
    }

    #[test]
    fn test_values_accessor() {
        let v = eval_str("Values[TimeSeries[{{1, 10}, {2, 20}}]]");
        match v {
            Value::List(items) => {
                assert_eq!(items, vec![iv(10), iv(20)]);
            }
            other => panic!("expected list, got {other}"),
        }
    }

    #[test]
    fn test_normal_still_handles_assoc() {
        let v = eval_str("Normal[<|\"a\" -> 1, \"b\" -> 2|>]");
        match v {
            Value::List(rules) => assert_eq!(rules.len(), 2),
            other => panic!("expected rule list, got {other}"),
        }
    }

    #[test]
    fn test_values_still_handles_assoc() {
        let v = eval_str("Values[<|\"a\" -> 1, \"b\" -> 2|>]");
        match v {
            Value::List(items) => {
                let mut nums: Vec<i64> = items.iter().filter_map(|x| x.to_integer()).collect();
                nums.sort();
                assert_eq!(nums, vec![1, 2]);
            }
            other => panic!("expected list, got {other}"),
        }
    }

    // ── Moving statistics ──

    #[test]
    fn test_moving_average_list_unchanged() {
        let v = eval_str("MovingAverage[{1, 2, 3, 4}, 2]");
        match v {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
                approx(as_f(&items[0]), 1.5);
                approx(as_f(&items[2]), 3.5);
            }
            other => panic!("expected list, got {other}"),
        }
    }

    #[test]
    fn test_moving_average_series() {
        let v = eval_str("MovingAverage[TimeSeries[{1, 2, 3, 4}], 2]");
        let ps = pairs_of(&v);
        assert_eq!(ps.len(), 3);
        approx(ps[0].0, 2.0);
        approx(ps[0].1, 1.5);
        approx(ps[2].0, 4.0);
        approx(ps[2].1, 3.5);
    }

    #[test]
    fn test_moving_median_list() {
        let v = eval_str("MovingMedian[{5, 1, 3, 2, 4}, 3]");
        match v {
            Value::List(items) => {
                assert_eq!(items.len(), 3);
                approx(as_f(&items[0]), 3.0); // median{5,1,3}
                approx(as_f(&items[2]), 3.0); // median{3,2,4}
            }
            other => panic!("expected list, got {other}"),
        }
    }

    #[test]
    fn test_moving_median_series() {
        let v = builtin_moving_median(&[series(vec![(1, 5), (2, 1), (3, 3)]), iv(3)]).unwrap();
        let ps = pairs_of(&v);
        assert_eq!(ps.len(), 1);
        approx(ps[0].0, 3.0);
        approx(ps[0].1, 3.0);
    }

    #[test]
    fn test_moving_map_mean() {
        let v = eval_str("MovingMap[Mean, TimeSeries[{1, 2, 3, 4}], 2]");
        let ps = pairs_of(&v);
        assert_eq!(ps.len(), 3);
        approx(ps[0].0, 2.0);
        approx(ps[0].1, 1.5);
        approx(ps[1].1, 2.5);
        approx(ps[2].0, 4.0);
        approx(ps[2].1, 3.5);
    }

    #[test]
    fn test_moving_map_pure_function() {
        let v = eval_str("MovingMap[Total, TimeSeries[{1, 2, 3}], 2]");
        let ps = pairs_of(&v);
        assert_eq!(ps.len(), 2);
        approx(ps[0].1, 3.0);
        approx(ps[1].1, 5.0);
    }

    #[test]
    fn test_moving_map_on_list_gives_series() {
        let v = builtin_moving_map(
            &[
                Value::Symbol("Total".to_string()),
                lst(vec![iv(1), iv(2), iv(3)]),
                iv(2),
            ],
            &Env::new(),
        )
        .unwrap();
        // No builtins registered in this bare env, so applying `Total` yields
        // symbolic calls, but the container is still a TimeSeries.
        assert!(matches!(&v, Value::Call { head, .. } if head == "TimeSeries"));
    }

    #[test]
    fn test_moving_map_window_too_large() {
        let v = builtin_moving_map(
            &[
                Value::Symbol("Total".to_string()),
                series(vec![(1, 1), (2, 2)]),
                iv(5),
            ],
            &Env::new(),
        )
        .unwrap();
        assert_eq!(pairs_of(&v), Vec::<(f64, f64)>::new());
    }

    // ── Windowing / resampling ──

    #[test]
    fn test_time_series_window() {
        let v = eval_str("TimeSeriesWindow[TimeSeries[{10, 20, 30, 40, 50}], {2, 4}]");
        let ps = pairs_of(&v);
        assert_eq!(ps, vec![(2.0, 20.0), (3.0, 30.0), (4.0, 40.0)]);
    }

    #[test]
    fn test_time_series_window_empty() {
        let v = eval_str("TimeSeriesWindow[TimeSeries[{1, 2, 3}], {10, 20}]");
        assert_eq!(pairs_of(&v), Vec::<(f64, f64)>::new());
    }

    #[test]
    fn test_time_series_resample() {
        let v = eval_str("TimeSeriesResample[TimeSeries[{{1, 10}, {2, 20}, {3, 30}}], 2]");
        let ps = pairs_of(&v);
        assert_eq!(ps, vec![(1.0, 10.0), (3.0, 30.0)]);
    }

    #[test]
    fn test_resample_inapplicable() {
        let v = eval_str("TimeSeriesResample[TimeSeries[{1, 2}], 0]");
        assert!(matches!(&v, Value::Call { head, .. } if head == "TimeSeriesResample"));
    }

    // ── Inapplicable inputs ──

    #[test]
    fn test_window_inapplicable_non_series() {
        let v = builtin_time_series_window(&[iv(5), lst(vec![iv(1), iv(2)])]).unwrap();
        assert!(matches!(&v, Value::Call { head, .. } if head == "TimeSeriesWindow"));
    }

    #[test]
    fn test_moving_median_inapplicable() {
        let v = builtin_moving_median(&[Value::Symbol("z".to_string()), iv(2)]).unwrap();
        assert!(matches!(&v, Value::Call { head, .. } if head == "MovingMedian"));
    }

    // ── Threading / mapping ──

    fn head_of(v: &Value) -> &str {
        match v {
            Value::Call { head, .. } => head.as_str(),
            _ => "<atom>",
        }
    }

    #[test]
    fn test_thread_combines_shared_times() {
        let v = eval_str(
            "TimeSeriesThread[Plus, {TimeSeries[{{1, 1}, {2, 2}, {3, 3}}], \
             TimeSeries[{{2, 10}, {3, 20}}]}]",
        );
        assert_eq!(pairs_of(&v), vec![(2.0, 12.0), (3.0, 23.0)]);
    }

    #[test]
    fn test_thread_single_series_applies_f() {
        let v = eval_str("TimeSeriesThread[Sin, {TimeSeries[{{1, 0}, {2, 0}}]}]");
        let ps = pairs_of(&v);
        assert_eq!(ps.len(), 2);
        approx(ps[0].1, 0.0);
    }

    #[test]
    fn test_thread_inapplicable() {
        let v = eval_str("TimeSeriesThread[Plus, 5]");
        assert_eq!(head_of(&v), "TimeSeriesThread");
    }

    #[test]
    fn test_map_applies_to_values() {
        let v = eval_str("TimeSeriesMap[Sqrt, TimeSeries[{1, 4, 9}]]");
        let ps = pairs_of(&v);
        assert_eq!(ps, vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0)]);
    }

    #[test]
    fn test_map_inapplicable() {
        let v = eval_str("TimeSeriesMap[Sqrt, 5]");
        assert_eq!(head_of(&v), "TimeSeriesMap");
    }

    // ── Mutation-style edits ──

    #[test]
    fn test_insert_new_point_keeps_order() {
        let v = eval_str("TimeSeriesInsert[TimeSeries[{{1, 1}, {3, 3}}], {2, 2}]");
        assert_eq!(pairs_of(&v), vec![(1.0, 1.0), (2.0, 2.0), (3.0, 3.0)]);
    }

    #[test]
    fn test_insert_overwrites_existing() {
        let v = eval_str("TimeSeriesInsert[TimeSeries[{{1, 1}, {2, 2}}], {2, 9}]");
        assert_eq!(pairs_of(&v), vec![(1.0, 1.0), (2.0, 9.0)]);
    }

    #[test]
    fn test_insert_inapplicable() {
        let v = builtin_time_series_insert(&[iv(5), pair(1, 1)]).unwrap();
        assert_eq!(head_of(&v), "TimeSeriesInsert");
    }

    #[test]
    fn test_delete_point() {
        let v = eval_str("TimeSeriesDelete[TimeSeries[{{1, 1}, {2, 2}, {3, 3}}], 2]");
        assert_eq!(pairs_of(&v), vec![(1.0, 1.0), (3.0, 3.0)]);
    }

    #[test]
    fn test_delete_list_of_times() {
        let v = eval_str("TimeSeriesDelete[TimeSeries[{{1, 1}, {2, 2}, {3, 3}}], {1, 3}]");
        assert_eq!(pairs_of(&v), vec![(2.0, 2.0)]);
    }

    #[test]
    fn test_shift_times() {
        let v = eval_str("TimeSeriesShift[TimeSeries[{{1, 10}, {2, 20}}], 5]");
        assert_eq!(pairs_of(&v), vec![(6.0, 10.0), (7.0, 20.0)]);
    }

    #[test]
    fn test_rescale_times() {
        let v = eval_str("TimeSeriesRescale[TimeSeries[{{1, 10}, {3, 30}}], {0, 100}]");
        assert_eq!(pairs_of(&v), vec![(0.0, 10.0), (100.0, 30.0)]);
    }

    #[test]
    fn test_rescale_constant_times() {
        let v = eval_str("TimeSeriesRescale[TimeSeries[{{2, 10}, {2, 20}}], {0, 100}]");
        assert_eq!(pairs_of(&v), vec![(0.0, 10.0), (0.0, 20.0)]);
    }

    // ── MovingMap alignment ──

    #[test]
    fn test_moving_map_align_variants() {
        let r = eval_str("MovingMap[Mean, TimeSeries[{{1, 1}, {2, 2}, {3, 3}}], 2, Right]");
        assert_eq!(
            pairs_of(&r).iter().map(|p| p.0).collect::<Vec<_>>(),
            vec![2.0, 3.0]
        );
        let l = eval_str("MovingMap[Mean, TimeSeries[{{1, 1}, {2, 2}, {3, 3}}], 2, Left]");
        assert_eq!(
            pairs_of(&l).iter().map(|p| p.0).collect::<Vec<_>>(),
            vec![1.0, 2.0]
        );
        let c =
            eval_str("MovingMap[Mean, TimeSeries[{{1, 1}, {2, 2}, {3, 3}, {4, 4}}], 3, Center]");
        assert_eq!(
            pairs_of(&c).iter().map(|p| p.0).collect::<Vec<_>>(),
            vec![2.0, 3.0]
        );
    }

    #[test]
    fn test_moving_map_alignment_default_right() {
        let v = eval_str("MovingMap[Mean, TimeSeries[{{1, 1}, {2, 2}, {3, 3}}], 2]");
        assert_eq!(
            pairs_of(&v).iter().map(|p| p.0).collect::<Vec<_>>(),
            vec![2.0, 3.0]
        );
    }

    #[test]
    fn test_moving_map_bad_alignment() {
        let v = eval_str("MovingMap[Mean, TimeSeries[{1, 2, 3}], 2, Middle]");
        assert_eq!(head_of(&v), "MovingMap");
    }

    // ── Correlation lists ──

    #[test]
    fn test_autocorrelation_list() {
        let v = eval_str("AutocorrelationList[TimeSeries[{1, 2, 3, 4, 5}], 2]");
        let Value::List(items) = v else {
            panic!("expected list");
        };
        approx(as_f(&items[0]), 1.0);
        approx(as_f(&items[1]), 0.4);
        approx(as_f(&items[2]), -0.1);
    }

    #[test]
    fn test_autocorrelation_list_default_lag() {
        let v = eval_str("AutocorrelationList[TimeSeries[{1, 2, 3, 4, 5}]]");
        let Value::List(items) = v else {
            panic!("expected list");
        };
        assert_eq!(items.len(), 5);
        approx(as_f(&items[4]), -0.4);
    }

    #[test]
    fn test_partial_correlation_list() {
        let v = eval_str("PartialCorrelationList[TimeSeries[{1, 2, 3, 4, 5}], 2]");
        let Value::List(items) = v else {
            panic!("expected list");
        };
        approx(as_f(&items[0]), 1.0);
        approx(as_f(&items[1]), 0.4);
        approx(as_f(&items[2]), -0.30952380952380953);
    }

    #[test]
    fn test_correlation_inapplicable() {
        let v = builtin_autocorrelation_list(&[iv(5)]).unwrap();
        assert_eq!(head_of(&v), "AutocorrelationList");
    }

    // ── Linear processes ──

    #[test]
    fn test_ar_process_zero_noise() {
        let v = eval_str("ARProcess[{1}, 0, 3]");
        let ps = pairs_of(&v);
        assert_eq!(ps, vec![(1.0, 0.0), (2.0, 0.0), (3.0, 0.0)]);
    }

    #[test]
    fn test_ar_process_deterministic() {
        let a = eval_str("SeedRandom[42]; ARProcess[{0.5}, 1, 5]");
        let b = eval_str("SeedRandom[42]; ARProcess[{0.5}, 1, 5]");
        assert_eq!(pairs_of(&a), pairs_of(&b));
    }

    #[test]
    fn test_ma_matches_ar_with_zero_coeff() {
        let a = eval_str("MAProcess[{0}, 1, 4]");
        let b = eval_str("ARProcess[{0}, 1, 4]");
        assert_eq!(pairs_of(&a), pairs_of(&b));
    }

    #[test]
    fn test_arma_matches_ma_with_zero_ar() {
        let a = eval_str("ARMAProcess[{0}, {0}, 1, 4]");
        let b = eval_str("MAProcess[{0}, 1, 4]");
        assert_eq!(pairs_of(&a), pairs_of(&b));
    }

    #[test]
    fn test_process_unevaluated_without_n() {
        assert_eq!(head_of(&eval_str("ARProcess[{0.5}, 1]")), "ARProcess");
        assert_eq!(head_of(&eval_str("MAProcess[{0.5}, 1]")), "MAProcess");
        assert_eq!(
            head_of(&eval_str("ARMAProcess[{0.5}, {0.2}, 1]")),
            "ARMAProcess"
        );
    }

    // ── Statistics / accessors ──

    #[test]
    fn test_mean_on_series() {
        let v = eval_str("Mean[TimeSeries[{1, 2, 3, 4, 5}]]");
        approx(as_f(&v), 3.0);
    }

    #[test]
    fn test_variance_on_series() {
        let v = eval_str("Variance[TimeSeries[{1, 2, 3, 4, 5}]]");
        approx(as_f(&v), 2.5);
    }

    #[test]
    fn test_mean_variance_still_lists() {
        approx(as_f(&eval_str("Mean[{1, 2, 3, 4}]")), 2.5);
        approx(as_f(&eval_str("Variance[{1, 2, 3}]")), 1.0);
    }

    #[test]
    fn test_datelist_accessor() {
        let v = eval_str("DateList[TimeSeries[{{1, 10}, {2, 20}}]]");
        match v {
            Value::List(items) => assert_eq!(items, vec![iv(1), iv(2)]),
            other => panic!("expected list, got {other}"),
        }
    }

    #[test]
    fn test_mean_inapplicable_empty_series() {
        let v = builtin_mean(&[make_series("TimeSeries", vec![])]).unwrap();
        assert_eq!(head_of(&v), "Mean");
    }
}
