//! Statistics package builtins.
//!
//! Provides core statistical functions: Mean, Median, Variance,
//! StandardDeviation, Quantile, Covariance, Correlation,
//! RandomVariate, NormalDistribution, UniformDistribution.

use crate::value::{EvalError, Value};
use rug::Float;
use rug::Integer;

// ── Helpers ─────────────────────────────────────────────────────────────────

/// Extract a list of values, or error.
fn as_list(v: &Value) -> Result<&Vec<Value>, EvalError> {
    match v {
        Value::List(items) => Ok(items),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: v.type_name().to_string(),
        }),
    }
}

/// Convert a Value to f64.
fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        _ => None,
    }
}

/// Create a Real value from f64.
fn real(v: f64) -> Value {
    Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, v))
}

/// Create an Integer value.
fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

/// Extract all numeric values from a list as f64, with error on non-numeric.
fn extract_numbers(items: &[Value]) -> Result<Vec<f64>, EvalError> {
    items
        .iter()
        .map(|v| {
            to_f64(v).ok_or_else(|| EvalError::TypeError {
                expected: "Number".to_string(),
                got: v.type_name().to_string(),
            })
        })
        .collect()
}

// ── Builtins ────────────────────────────────────────────────────────────────

/// Mean[list] — arithmetic mean.
pub fn builtin_mean(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Mean requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error("Mean: list must not be empty".to_string()));
    }
    let nums = extract_numbers(items)?;
    let sum: f64 = nums.iter().sum();
    let mean = sum / nums.len() as f64;
    // Return integer if exact
    if mean.fract() == 0.0 && mean.abs() < i64::MAX as f64 {
        Ok(int(mean as i64))
    } else {
        Ok(real(mean))
    }
}

/// Median[list] — median value.
pub fn builtin_median(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Median requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "Median: list must not be empty".to_string(),
        ));
    }
    let mut nums = extract_numbers(items)?;
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = nums.len();
    let med = if n % 2 == 1 {
        nums[n / 2]
    } else {
        (nums[n / 2 - 1] + nums[n / 2]) / 2.0
    };
    if med.fract() == 0.0 && med.abs() < i64::MAX as f64 {
        Ok(int(med as i64))
    } else {
        Ok(real(med))
    }
}

/// Variance[list] — sample variance (Bessel's correction, n-1 denominator).
pub fn builtin_variance(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Variance requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    let n = items.len();
    if n < 2 {
        return Err(EvalError::Error(
            "Variance: need at least 2 data points".to_string(),
        ));
    }
    let nums = extract_numbers(items)?;
    let mean: f64 = nums.iter().sum::<f64>() / n as f64;
    let ss: f64 = nums.iter().map(|x| (x - mean).powi(2)).sum();
    let var = ss / (n - 1) as f64;
    Ok(real(var))
}

/// StandardDeviation[list] — sample standard deviation.
pub fn builtin_standard_deviation(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "StandardDeviation requires exactly 1 argument".to_string(),
        ));
    }
    let var = builtin_variance(args)?;
    match var {
        Value::Real(r) => Ok(Value::Real(r.sqrt())),
        _ => Err(EvalError::Error(
            "StandardDeviation: unexpected variance result".to_string(),
        )),
    }
}

/// Quantile[list, q] — q-th quantile (0 ≤ q ≤ 1).
pub fn builtin_quantile(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Quantile requires exactly 2 arguments".to_string(),
        ));
    }
    let items = as_list(&args[0])?;
    if items.is_empty() {
        return Err(EvalError::Error(
            "Quantile: list must not be empty".to_string(),
        ));
    }
    let q = to_f64(&args[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if !(0.0..=1.0).contains(&q) {
        return Err(EvalError::Error(
            "Quantile: q must be between 0 and 1".to_string(),
        ));
    }
    let mut nums = extract_numbers(items)?;
    nums.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = nums.len();
    if n == 1 {
        return Ok(real(nums[0]));
    }
    // Linear interpolation
    let pos = q * (n - 1) as f64;
    let lo = pos.floor() as usize;
    let hi = (lo + 1).min(n - 1);
    let frac = pos - lo as f64;
    let val = nums[lo] * (1.0 - frac) + nums[hi] * frac;
    Ok(real(val))
}

/// Covariance[list1, list2] — sample covariance.
pub fn builtin_covariance(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Covariance requires exactly 2 arguments".to_string(),
        ));
    }
    let items1 = as_list(&args[0])?;
    let items2 = as_list(&args[1])?;
    if items1.len() != items2.len() {
        return Err(EvalError::Error(format!(
            "Covariance: lists must have same length (got {} and {})",
            items1.len(),
            items2.len()
        )));
    }
    let n = items1.len();
    if n < 2 {
        return Err(EvalError::Error(
            "Covariance: need at least 2 data points".to_string(),
        ));
    }
    let nums1 = extract_numbers(items1)?;
    let nums2 = extract_numbers(items2)?;
    let mean1: f64 = nums1.iter().sum::<f64>() / n as f64;
    let mean2: f64 = nums2.iter().sum::<f64>() / n as f64;
    let cov: f64 = nums1
        .iter()
        .zip(nums2.iter())
        .map(|(x, y)| (x - mean1) * (y - mean2))
        .sum::<f64>()
        / (n - 1) as f64;
    Ok(real(cov))
}

/// Correlation[list1, list2] — Pearson correlation coefficient.
pub fn builtin_correlation(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Correlation requires exactly 2 arguments".to_string(),
        ));
    }
    let cov = builtin_covariance(args)?;
    let sd1 = builtin_standard_deviation(&[args[0].clone()])?;
    let sd2 = builtin_standard_deviation(&[args[1].clone()])?;
    match (&cov, &sd1, &sd2) {
        (Value::Real(c), Value::Real(s1), Value::Real(s2)) => {
            if s1.is_zero() || s2.is_zero() {
                return Err(EvalError::Error(
                    "Correlation: standard deviation is zero".to_string(),
                ));
            }
            let prec = c.prec().max(s1.prec()).max(s2.prec());
            let result =
                Float::with_val(prec, c) / (Float::with_val(prec, s1) * Float::with_val(prec, s2));
            Ok(Value::Real(result))
        }
        _ => Err(EvalError::Error(
            "Correlation: unexpected result types".to_string(),
        )),
    }
}

/// RandomVariate[dist, n] — generate n random samples from a distribution.
pub fn builtin_random_variate(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "RandomVariate requires exactly 2 arguments".to_string(),
        ));
    }
    let dist = match &args[0] {
        Value::Assoc(map) => map,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Association (distribution)".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let n = match &args[1] {
        Value::Integer(i) => i.to_usize().ok_or_else(|| {
            EvalError::Error("RandomVariate: count must be a non-negative integer".to_string())
        })?,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };

    let dist_type = dist
        .get("Distribution")
        .and_then(|v| match v {
            Value::Str(s) => Some(s.as_str()),
            _ => None,
        })
        .ok_or_else(|| {
            EvalError::Error(
                "RandomVariate: invalid distribution (missing 'Distribution' key)".to_string(),
            )
        })?;

    let mut rng = fastrand::Rng::new();
    let samples: Vec<Value> = match dist_type {
        "Normal" => {
            let mean = dist.get("Mean").and_then(to_f64).unwrap_or(0.0);
            let sd = dist.get("SD").and_then(to_f64).unwrap_or(1.0);
            (0..n)
                .map(|_| {
                    // Box-Muller transform
                    let u1 = rng.f64().max(1e-15);
                    let u2 = rng.f64();
                    let z = (-2.0_f64 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos();
                    real(mean + sd * z)
                })
                .collect()
        }
        "Uniform" => {
            let lo = dist.get("Min").and_then(to_f64).unwrap_or(0.0);
            let hi = dist.get("Max").and_then(to_f64).unwrap_or(1.0);
            (0..n).map(|_| real(lo + (hi - lo) * rng.f64())).collect()
        }
        "Poisson" => {
            let lambda = dist.get("Lambda").and_then(to_f64).unwrap_or(1.0);
            (0..n)
                .map(|_| {
                    // Knuth's algorithm
                    let l = (-lambda).exp();
                    let mut k = 0u64;
                    let mut p = 1.0f64;
                    loop {
                        k += 1;
                        p *= rng.f64();
                        if p < l {
                            break;
                        }
                    }
                    int((k - 1) as i64)
                })
                .collect()
        }
        other => {
            return Err(EvalError::Error(format!(
                "RandomVariate: unknown distribution type '{}'",
                other
            )));
        }
    };

    Ok(Value::List(samples))
}

/// NormalDistribution[mu, sigma] — create a normal distribution object.
pub fn builtin_normal_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "NormalDistribution requires exactly 2 arguments".to_string(),
        ));
    }
    let mu = to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    let sigma = to_f64(&args[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if sigma <= 0.0 {
        return Err(EvalError::Error(
            "NormalDistribution: standard deviation must be positive".to_string(),
        ));
    }
    let mut map = std::collections::HashMap::new();
    map.insert("Distribution".to_string(), Value::Str("Normal".to_string()));
    map.insert("Mean".to_string(), real(mu));
    map.insert("SD".to_string(), real(sigma));
    Ok(Value::Assoc(map))
}

/// UniformDistribution[min, max] — create a uniform distribution object.
pub fn builtin_uniform_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "UniformDistribution requires exactly 2 arguments".to_string(),
        ));
    }
    let lo = to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    let hi = to_f64(&args[1]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    if lo >= hi {
        return Err(EvalError::Error(
            "UniformDistribution: min must be less than max".to_string(),
        ));
    }
    let mut map = std::collections::HashMap::new();
    map.insert(
        "Distribution".to_string(),
        Value::Str("Uniform".to_string()),
    );
    map.insert("Min".to_string(), real(lo));
    map.insert("Max".to_string(), real(hi));
    Ok(Value::Assoc(map))
}

/// PoissonDistribution[lambda] — create a Poisson distribution object.
pub fn builtin_poisson_distribution(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PoissonDistribution requires exactly 1 argument".to_string(),
        ));
    }
    let lambda = to_f64(&args[0]).ok_or_else(|| EvalError::TypeError {
        expected: "Number".to_string(),
        got: args[0].type_name().to_string(),
    })?;
    if lambda <= 0.0 {
        return Err(EvalError::Error(
            "PoissonDistribution: lambda must be positive".to_string(),
        ));
    }
    let mut map = std::collections::HashMap::new();
    map.insert(
        "Distribution".to_string(),
        Value::Str("Poisson".to_string()),
    );
    map.insert("Lambda".to_string(), real(lambda));
    Ok(Value::Assoc(map))
}

// ── Registration ────────────────────────────────────────────────────────────

/// Register all Statistics builtins in the environment.
pub fn register(env: &crate::env::Env) {
    use super::register_builtin;
    register_builtin(env, "Mean", builtin_mean);
    register_builtin(env, "Median", builtin_median);
    register_builtin(env, "Variance", builtin_variance);
    register_builtin(env, "StandardDeviation", builtin_standard_deviation);
    register_builtin(env, "Quantile", builtin_quantile);
    register_builtin(env, "Covariance", builtin_covariance);
    register_builtin(env, "Correlation", builtin_correlation);
    register_builtin(env, "RandomVariate", builtin_random_variate);
    register_builtin(env, "NormalDistribution", builtin_normal_distribution);
    register_builtin(env, "UniformDistribution", builtin_uniform_distribution);
    register_builtin(env, "PoissonDistribution", builtin_poisson_distribution);
}

/// Symbol names exported by the Statistics package.
pub const SYMBOLS: &[&str] = &[
    "Mean",
    "Median",
    "Variance",
    "StandardDeviation",
    "Quantile",
    "Covariance",
    "Correlation",
    "RandomVariate",
    "NormalDistribution",
    "UniformDistribution",
    "PoissonDistribution",
    // Syma-side wrappers (loaded from .syma file):
    "GeometricMean",
    "HarmonicMean",
    "Skewness",
    "Kurtosis",
    "BinCounts",
    "HistogramList",
];

#[cfg(test)]
mod tests {
    use super::*;
    use rug::Integer;

    fn int_val(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn real_val(v: f64) -> Value {
        Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, v))
    }

    fn list(vals: Vec<Value>) -> Value {
        Value::List(vals)
    }

    #[test]
    fn test_mean_integers() {
        let data = list(vec![
            int_val(1),
            int_val(2),
            int_val(3),
            int_val(4),
            int_val(5),
        ]);
        let result = builtin_mean(&[data]).unwrap();
        assert_eq!(result, int_val(3));
    }

    #[test]
    fn test_mean_real() {
        let data = list(vec![int_val(1), int_val(2), int_val(3)]);
        let result = builtin_mean(&[data]).unwrap();
        // Mean = 2.0 → integer
        assert_eq!(result, int_val(2));
    }

    #[test]
    fn test_mean_non_integer() {
        let data = list(vec![int_val(1), int_val(2)]);
        let result = builtin_mean(&[data]).unwrap();
        // Mean = 1.5
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 1.5).abs() < 1e-10);
        } else {
            panic!("Expected Real, got {:?}", result);
        }
    }

    #[test]
    fn test_median_odd() {
        let data = list(vec![int_val(3), int_val(1), int_val(2)]);
        let result = builtin_median(&[data]).unwrap();
        assert_eq!(result, int_val(2));
    }

    #[test]
    fn test_median_even() {
        let data = list(vec![int_val(4), int_val(1), int_val(3), int_val(2)]);
        let result = builtin_median(&[data]).unwrap();
        // Median = (2 + 3) / 2 = 2.5
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 2.5).abs() < 1e-10);
        } else {
            panic!("Expected Real, got {:?}", result);
        }
    }

    #[test]
    fn test_variance() {
        let data = list(vec![
            int_val(2),
            int_val(4),
            int_val(4),
            int_val(4),
            int_val(5),
            int_val(5),
            int_val(7),
            int_val(9),
        ]);
        let result = builtin_variance(&[data]).unwrap();
        if let Value::Real(r) = result {
            // Sample variance of {2,4,4,4,5,5,7,9} = 4.5714...
            assert!((r.to_f64() - 32.0 / 7.0).abs() < 1e-10);
        } else {
            panic!("Expected Real, got {:?}", result);
        }
    }

    #[test]
    fn test_standard_deviation() {
        let data = list(vec![
            int_val(2),
            int_val(4),
            int_val(4),
            int_val(4),
            int_val(5),
            int_val(5),
            int_val(7),
            int_val(9),
        ]);
        let result = builtin_standard_deviation(&[data]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - (32.0f64 / 7.0).sqrt()).abs() < 1e-10);
        } else {
            panic!("Expected Real, got {:?}", result);
        }
    }

    #[test]
    fn test_quantile() {
        let data = list(vec![
            int_val(1),
            int_val(2),
            int_val(3),
            int_val(4),
            int_val(5),
        ]);
        // 0.5 quantile = median = 3
        let result = builtin_quantile(&[data, real_val(0.5)]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 3.0).abs() < 1e-10);
        } else {
            panic!("Expected Real, got {:?}", result);
        }
    }

    #[test]
    fn test_quantile_endpoints() {
        let data = list(vec![int_val(10), int_val(20), int_val(30)]);
        // q=0 → min = 10
        let r0 = builtin_quantile(&[data.clone(), real_val(0.0)]).unwrap();
        if let Value::Real(r) = r0 {
            assert!((r.to_f64() - 10.0).abs() < 1e-10);
        }
        // q=1 → max = 30
        let r1 = builtin_quantile(&[data, real_val(1.0)]).unwrap();
        if let Value::Real(r) = r1 {
            assert!((r.to_f64() - 30.0).abs() < 1e-10);
        }
    }

    #[test]
    fn test_covariance() {
        let x = list(vec![int_val(1), int_val(2), int_val(3)]);
        let y = list(vec![int_val(4), int_val(5), int_val(6)]);
        let result = builtin_covariance(&[x, y]).unwrap();
        if let Value::Real(r) = result {
            // Cov({1,2,3},{4,5,6}) = 1.0
            assert!((r.to_f64() - 1.0).abs() < 1e-10);
        } else {
            panic!("Expected Real, got {:?}", result);
        }
    }

    #[test]
    fn test_correlation_perfect() {
        let x = list(vec![int_val(1), int_val(2), int_val(3)]);
        let y = list(vec![int_val(2), int_val(4), int_val(6)]);
        let result = builtin_correlation(&[x, y]).unwrap();
        if let Value::Real(r) = result {
            assert!((r.to_f64() - 1.0).abs() < 1e-10);
        } else {
            panic!("Expected Real, got {:?}", result);
        }
    }

    #[test]
    fn test_normal_distribution() {
        let dist = builtin_normal_distribution(&[real_val(0.0), real_val(1.0)]).unwrap();
        if let Value::Assoc(map) = dist {
            assert_eq!(
                map.get("Distribution"),
                Some(&Value::Str("Normal".to_string()))
            );
        } else {
            panic!("Expected Assoc");
        }
    }

    #[test]
    fn test_uniform_distribution() {
        let dist = builtin_uniform_distribution(&[real_val(0.0), real_val(1.0)]).unwrap();
        if let Value::Assoc(map) = dist {
            assert_eq!(
                map.get("Distribution"),
                Some(&Value::Str("Uniform".to_string()))
            );
        } else {
            panic!("Expected Assoc");
        }
    }

    #[test]
    fn test_random_variate_normal() {
        let dist = builtin_normal_distribution(&[real_val(0.0), real_val(1.0)]).unwrap();
        let samples = builtin_random_variate(&[dist, int_val(100)]).unwrap();
        if let Value::List(items) = samples {
            assert_eq!(items.len(), 100);
            // All should be Real numbers
            for s in &items {
                assert!(matches!(s, Value::Real(_)));
            }
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_random_variate_uniform() {
        let dist = builtin_uniform_distribution(&[real_val(0.0), real_val(10.0)]).unwrap();
        let samples = builtin_random_variate(&[dist, int_val(50)]).unwrap();
        if let Value::List(items) = samples {
            assert_eq!(items.len(), 50);
        } else {
            panic!("Expected List");
        }
    }
}
