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

/// MovingMap[f, series, n] — map `f` over every length-`n` window.
pub fn builtin_moving_map(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return uneval("MovingMap", args);
    }
    let n = match args[2].to_integer() {
        Some(n) if n > 0 => n as usize,
        _ => return uneval("MovingMap", args),
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
        let t = window
            .last()
            .and_then(pair_time)
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
    // `Normal`/`Values` are also association accessors; these registrations
    // run last so the series-aware wrappers take precedence, while still
    // delegating non-series arguments.
    register_builtin(env, "Normal", builtin_normal);
    register_builtin(env, "Values", builtin_values);
    register_builtin_env(env, "MovingMap", builtin_moving_map);
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
}
