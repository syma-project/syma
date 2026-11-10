use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::{Float, Integer};

// ── Extractors ────────────────────────────────────────────────────────────────

/// Extract a list from a Value, returning TypeError for non-lists.
fn get_list(val: &Value) -> Result<&[Value], EvalError> {
    match val {
        Value::List(items) => Ok(items.as_slice()),
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: val.type_name().to_string(),
        }),
    }
}

/// Extract a non-negative integer from a Value.
fn non_neg_int(val: &Value) -> Result<Integer, EvalError> {
    match val {
        Value::Integer(n) if !n.is_negative() => Ok(n.clone()),
        Value::Integer(_) => Err(EvalError::TypeError {
            expected: "non-negative Integer".to_string(),
            got: "negative Integer".to_string(),
        }),
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: val.type_name().to_string(),
        }),
    }
}

/// Extract a non-negative integer index, accepting also integral `Real`s.
///
/// `N[...]` numericizes integer arguments to reals, so combinatorial functions
/// such as `BellNumber`/`Derangements`/`EulerE` must still evaluate under it.
fn non_neg_usize(v: &Value) -> Option<usize> {
    match v {
        Value::Integer(i) if !i.is_negative() => i.to_usize(),
        Value::Real(r) if r.is_integer() && !r.is_sign_negative() => {
            r.to_integer().and_then(|i| i.to_usize())
        }
        _ => None,
    }
}

/// Extract any integer from a Value.
fn to_int(val: &Value) -> Result<Integer, EvalError> {
    match val {
        Value::Integer(n) => Ok(n.clone()),
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: val.type_name().to_string(),
        }),
    }
}

// ── GCD helper ────────────────────────────────────────────────────────────────

fn gcd_int(mut a: Integer, mut b: Integer) -> Integer {
    while !b.is_zero() {
        let t = b.clone();
        b = a % b;
        a = t;
    }
    a
}

// ── Binomial ──────────────────────────────────────────────────────────────────

/// Binomial[n, k] — the binomial coefficient "n choose k".
/// Uses GCD reduction to avoid intermediate growth.
pub fn builtin_binomial(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Binomial requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(n), Value::Integer(k)) => {
            // Generalized binomial coefficient: valid for negative `n` too
            // (C(n, k) = n (n-1) ... (n-k+1) / k!).
            if k.is_negative() {
                return Ok(Value::Integer(Integer::from(0)));
            }
            if k.is_zero() {
                return Ok(Value::Integer(Integer::from(1)));
            }
            if !n.is_negative() && *k > *n {
                return Ok(Value::Integer(Integer::from(0)));
            }
            if !n.is_negative() && *k == *n {
                return Ok(Value::Integer(Integer::from(1)));
            }
            let k_usize = k
                .to_usize()
                .ok_or_else(|| EvalError::Error("Binomial: k too large".to_string()))?;
            // Use symmetry: C(n, k) = C(n, n-k) to minimize work. Only applies
            // to non-negative `n`; for negative `n` the generalized product is
            // used directly.
            let k_eff = if n.is_negative() {
                k_usize
            } else {
                match (n.clone() - k.clone()).to_usize() {
                    Some(n_minus_k) => k_usize.min(n_minus_k),
                    None => k_usize,
                }
            };
            let mut result = Integer::from(1);
            for i in 0..k_eff {
                // Multiply by (n - i), then divide by (i + 1), reducing by GCD each step.
                let mut numerator = n.clone() - Integer::from(i);
                let mut denominator = Integer::from(i + 1);
                let g = gcd_int(result.clone(), denominator.clone());
                result /= g.clone();
                denominator /= g;
                let g2 = gcd_int(numerator.clone(), denominator.clone());
                numerator /= g2.clone();
                denominator /= g2;
                result *= numerator;
                result /= denominator;
            }
            Ok(Value::Integer(result))
        }
        _ => Ok(Value::Call {
            head: "Binomial".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── Multinomial ──────────────────────────────────────────────────────────────

/// Multinomial[n1, n2, ...] and Multinomial[{n1, n2, ...}] — the multinomial coefficient.
/// Returns (n1+n2+...)! / (n1! * n2! * ...). Multinomial[] = Multinomial[{}] = 1.
pub fn builtin_multinomial(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Integer(Integer::from(1)));
    }
    // Handle Multinomial[{n1, n2, ...}] (single list argument); a single bare
    // integer is a one-element sequence, not a list.
    let ints: Vec<Integer> = if args.len() == 1 {
        match &args[0] {
            Value::List(items) => items.iter().map(to_int).collect::<Result<Vec<_>, _>>()?,
            other => vec![to_int(other)?],
        }
    } else {
        args.iter().map(to_int).collect::<Result<Vec<_>, _>>()?
    };
    if ints.is_empty() {
        return Ok(Value::Integer(Integer::from(1)));
    }
    let sum: Integer = ints.iter().cloned().fold(Integer::from(0), |a, b| a + b);
    let mut result = integer_factorial(&sum);
    for k in &ints {
        result /= integer_factorial(k);
    }
    Ok(Value::Integer(result))
}

// ── Factorial2 ────────────────────────────────────────────────────────────────

/// Factorial2[n] — the double factorial n!!.
/// n!! = n * (n-2) * (n-4) * ... stopping at 1 or 2.
/// Negative odd n: 1 / ((-n-2)!!).
/// n == 0 or n == -1: 1.
pub fn builtin_factorial2(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Factorial2 requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if *n <= -2 {
                // Negative odd n: 1 / ((-n - 2)!!)
                let neg = -n.clone();
                let adjusted = neg - Integer::from(2);
                let denom = integer_double_factorial(&adjusted);
                Ok(Value::Call {
                    head: "Divide".to_string(),
                    args: vec![Value::Integer(Integer::from(1)), Value::Integer(denom)],
                })
            } else if *n == 0 || *n == -1 {
                Ok(Value::Integer(Integer::from(1)))
            } else if !n.is_negative() {
                Ok(Value::Integer(integer_double_factorial(n)))
            } else {
                Ok(Value::Call {
                    head: "Factorial2".to_string(),
                    args: args.to_vec(),
                })
            }
        }
        _ => Ok(Value::Call {
            head: "Factorial2".to_string(),
            args: args.to_vec(),
        }),
    }
}

/// Compute n!! for non-negative n.
fn integer_double_factorial(n: &Integer) -> Integer {
    let mut result = Integer::from(1);
    let mut i = n.clone();
    while i > 0 {
        result *= i.clone();
        i -= 2;
    }
    result
}

// ── AlternatingFactorial ─────────────────────────────────────────────────────

/// AlternatingFactorial[n] — 1! - 2! + 3! - 4! + ... + (-1)^(n+1)*n!.
pub fn builtin_alternating_factorial(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "AlternatingFactorial requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if !n.is_negative() => {
            let n_val = n
                .to_usize()
                .ok_or_else(|| EvalError::Error("AlternatingFactorial: n too large".to_string()))?;
            if n_val == 0 {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let mut result = Integer::from(0);
            let mut fact = Integer::from(1);
            for i in 1..=n_val {
                fact *= Integer::from(i);
                if i % 2 == 1 {
                    result += fact.clone();
                } else {
                    result -= fact.clone();
                }
            }
            Ok(Value::Integer(result))
        }
        _ => Ok(Value::Call {
            head: "AlternatingFactorial".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── Subfactorial ──────────────────────────────────────────────────────────────

/// Subfactorial[n] — the number of derangements of n elements (!n).
/// !0 = 1, !1 = 0, !n = n * !(n-1) + (-1)^n.
pub fn builtin_subfactorial(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Subfactorial requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if !n.is_negative() => {
            let n_val = n
                .to_usize()
                .ok_or_else(|| EvalError::Error("Subfactorial: n too large".to_string()))?;
            if n_val == 0 {
                return Ok(Value::Integer(Integer::from(1)));
            }
            if n_val == 1 {
                return Ok(Value::Integer(Integer::from(0)));
            }
            // Iterative: !n = n * !(n-1) + (-1)^n, seeded with !1 = 0.
            let mut curr = Integer::from(0); // !1
            for i in 2..=n_val {
                let sign = if i % 2 == 0 {
                    Integer::from(1)
                } else {
                    Integer::from(-1)
                };
                curr = Integer::from(i) * curr + sign;
            }
            Ok(Value::Integer(curr))
        }
        _ => Ok(Value::Call {
            head: "Subfactorial".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── Permutations ──────────────────────────────────────────────────────────────

/// Permutations[list] — all permutations of the elements.
/// Permutations[list, k] — permutations of length k.
/// Permutations[list, {kmin, kmax}] — all permutations from length kmin to kmax.
pub fn builtin_permutations(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Permutations requires at least 1 argument".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let len = items.len();

    if args.len() == 1 {
        // All permutations of length len, with duplicates removed (so that
        // Permutations[{1, 1}] == {{1, 1}}, matching Mathematica).
        let mut result = Vec::new();
        permute(items, len, &mut result);
        Ok(Value::List(dedup_values(result)))
    } else if args.len() == 2 {
        // Parse spec.
        let perms = generate_permutations(items, &args[1])?;
        Ok(Value::List(dedup_values(perms)))
    } else {
        Err(EvalError::Error(
            "Permutations: usage is Permutations[list], Permutations[list, k], or Permutations[list, {kmin, kmax}]"
                .to_string(),
        ))
    }
}

/// Generate permutations with a specification (k or {kmin, kmax}).
fn generate_permutations(items: &[Value], spec: &Value) -> Result<Vec<Value>, EvalError> {
    match spec {
        Value::Integer(k) => {
            let k = k.clone();
            if k <= 0 {
                return Ok(vec![Value::List(vec![])]);
            }
            let k_usize = k
                .to_usize()
                .ok_or_else(|| EvalError::Error("Permutations: k too large".to_string()))?;
            let len = items.len();
            if k_usize > len {
                return Ok(Vec::new());
            }
            // Generate all permutations of length k from the list.
            // Use He's algorithm for combinations + internal permuting,
            // or simpler: generate all permutations of size k from the n items.
            let mut result = Vec::new();
            let used = vec![false; len];
            let current = Vec::with_capacity(k_usize);
            partial_permutations(items, &used, &current, k_usize, &mut result);
            Ok(result)
        }
        Value::List(spec_list) if spec_list.len() == 2 => {
            let kmin = to_int(&spec_list[0])?;
            let kmax = to_int(&spec_list[1])?;
            let kmin_usize = kmin
                .to_usize()
                .ok_or_else(|| EvalError::Error("Permutations: kmin too large".to_string()))?;
            let kmax_usize = kmax
                .to_usize()
                .ok_or_else(|| EvalError::Error("Permutations: kmax too large".to_string()))?;
            let mut all: Vec<Value> = Vec::new();
            for k in kmin_usize..=kmax_usize {
                let k_int = Integer::from(k);
                let perms = generate_permutations(items, &Value::Integer(k_int))?;
                all.extend(perms);
            }
            Ok(all)
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer or {kmin, kmax}".to_string(),
            got: spec.type_name().to_string(),
        }),
    }
}

/// Remove duplicate values while preserving first-occurrence order.
fn dedup_values(values: Vec<Value>) -> Vec<Value> {
    let mut out: Vec<Value> = Vec::with_capacity(values.len());
    for v in values {
        if !out.contains(&v) {
            out.push(v);
        }
    }
    out
}

/// Recursive backtracking for partial permutations of length k.
fn partial_permutations(
    items: &[Value],
    used: &[bool],
    current: &[Value],
    k: usize,
    result: &mut Vec<Value>,
) {
    if current.len() == k {
        result.push(Value::List(current.to_vec()));
        return;
    }
    for i in 0..items.len() {
        if !used[i] {
            let mut used_copy = used.to_vec();
            used_copy[i] = true;
            {
                let mut current_copy = current.to_vec();
                current_copy.push(items[i].clone());
                partial_permutations(items, &used_copy, &current_copy, k, result);
            }
        }
    }
}

/// Generate all permutations of the given items in-place into result.
fn permute(items: &[Value], n: usize, result: &mut Vec<Value>) {
    if n == 0 {
        result.push(Value::List(vec![]));
        return;
    }
    let mut buf = items[..n.min(items.len())].to_vec();
    permute_backtrack(&mut buf, 0, result);
}

/// Classic swap-based backtracking permutation generator.
fn permute_backtrack(buf: &mut [Value], start: usize, result: &mut Vec<Value>) {
    if start == buf.len() {
        result.push(Value::List(buf.to_vec()));
        return;
    }
    for i in start..buf.len() {
        buf.swap(start, i);
        permute_backtrack(buf, start + 1, result);
        buf.swap(start, i);
    }
}

// ── Subsets ───────────────────────────────────────────────────────────────────

/// Subsets[list] — all subsets of the list.
/// Subsets[list, k] — subsets of length k.
/// Subsets[list, {kmin, kmax}] — subsets from length kmin to kmax.
/// Subsets are generated in lexicographic (increasing length) order.
pub fn builtin_subsets(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Subsets requires at least 1 argument".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let len = items.len();

    if args.len() == 1 {
        // All subsets in increasing order of length.
        let mut result = Vec::new();
        for k in 0..=len {
            let subsets = generate_subsets_of_size(items, len, k);
            result.extend(subsets);
        }
        Ok(Value::List(result))
    } else if args.len() == 2 {
        let subs = generate_subsets_specified(items, len, &args[1])?;
        Ok(Value::List(subs))
    } else {
        Err(EvalError::Error(
            "Subsets: usage is Subsets[list], Subsets[list, k], or Subsets[list, {kmin, kmax}]"
                .to_string(),
        ))
    }
}

fn generate_subsets_of_size(items: &[Value], n: usize, k: usize) -> Vec<Value> {
    if k == 0 {
        return vec![Value::List(vec![])];
    }
    if k > n {
        return Vec::new();
    }
    let mut result = Vec::new();
    let mut indices: Vec<usize> = (0..k).collect();
    loop {
        let subset: Vec<Value> = indices.iter().map(|&i| items[i].clone()).collect();
        result.push(Value::List(subset));
        // Next combination using the standard odometer algorithm.
        let mut j = k as isize - 1;
        while j >= 0 && indices[j as usize] == n - k + j as usize {
            j -= 1;
        }
        if j < 0 {
            break;
        }
        let j = j as usize;
        indices[j] += 1;
        for m in (j + 1)..k {
            indices[m] = indices[m - 1] + 1;
        }
    }
    result
}

fn generate_subsets_specified(
    items: &[Value],
    n: usize,
    spec: &Value,
) -> Result<Vec<Value>, EvalError> {
    match spec {
        Value::Integer(k) => {
            let k = k.clone();
            if k < 0 {
                return Ok(Vec::new());
            }
            let k_usize = k
                .to_usize()
                .ok_or_else(|| EvalError::Error("Subsets: k too large".to_string()))?;
            Ok(generate_subsets_of_size(items, n, k_usize))
        }
        Value::List(spec_list) if spec_list.len() == 2 => {
            let kmin = to_int(&spec_list[0])?
                .to_usize()
                .ok_or_else(|| EvalError::Error("Subsets: kmin too large".to_string()))?;
            let kmax = to_int(&spec_list[1])?
                .to_usize()
                .ok_or_else(|| EvalError::Error("Subsets: kmax too large".to_string()))?;
            let mut all: Vec<Value> = Vec::new();
            for k in kmin..=kmax {
                let subs = generate_subsets_of_size(items, n, k);
                all.extend(subs);
            }
            Ok(all)
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer or {kmin, kmax}".to_string(),
            got: spec.type_name().to_string(),
        }),
    }
}

// ── Tuples ────────────────────────────────────────────────────────────────────

/// Tuples[elements, n] — all n-length tuples from elements.
/// Tuples[{a, b}, 3] -> {{a,a,a}, {a,a,b}, {a,b,a}, {a,b,b}, {b,a,a}, ...}
pub fn builtin_tuples(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Tuples requires exactly 2 arguments: Tuples[elements, n]".to_string(),
        ));
    }
    let elements = get_list(&args[0])?;
    let n = non_neg_int(&args[1])?;
    let n_usize = n
        .to_usize()
        .ok_or_else(|| EvalError::Error("Tuples: n too large".to_string()))?;

    if n_usize == 0 {
        return Ok(Value::List(vec![Value::List(vec![])]));
    }

    let elem_count = elements.len();
    if elem_count == 0 {
        return Ok(Value::List(vec![]));
    }

    // Total tuples = elem_count ^ n.
    let total = elem_count.pow(n_usize as u32);
    let mut result = Vec::with_capacity(total);
    // Each tuple is a base-element_count number in [0..n_usize) positions.
    for t in 0..total {
        let mut indices = Vec::with_capacity(n_usize);
        let mut remaining = t;
        for _ in 0..n_usize {
            indices.push(remaining % elem_count);
            remaining /= elem_count;
        }
        indices.reverse();
        let tuple: Vec<Value> = indices.iter().map(|&i| elements[i].clone()).collect();
        result.push(Value::List(tuple));
    }
    Ok(Value::List(result))
}

// ── Arrangements ──────────────────────────────────────────────────────────────

/// Arrangements[list, k] — all permutations of length k from list.
/// Same as Permutations[list, k].
pub fn builtin_arrangements(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Arrangements requires exactly 2 arguments: Arrangements[list, k]".to_string(),
        ));
    }
    let items = get_list(&args[0])?;
    let perms = generate_permutations(items, &args[1])?;
    Ok(Value::List(perms))
}

// ── StirlingS2 ────────────────────────────────────────────────────────────────

/// StirlingS2[n, k] — Stirling numbers of the second kind.
/// The number of ways to partition n labeled elements into k non-empty subsets.
/// S(n, k) = k * S(n-1, k) + S(n-1, k-1), S(0,0) = 1, S(n,0) = 0 for n > 0.
pub fn builtin_stirling_s2(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StirlingS2 requires exactly 2 arguments: StirlingS2[n, k]".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(n), Value::Integer(k)) => {
            if n.is_negative() || k.is_negative() {
                return Ok(Value::Call {
                    head: "StirlingS2".to_string(),
                    args: args.to_vec(),
                });
            }
            let n_val = n
                .to_usize()
                .ok_or_else(|| EvalError::Error("StirlingS2: n too large".to_string()))?;
            let k_val = k
                .to_usize()
                .ok_or_else(|| EvalError::Error("StirlingS2: k too large".to_string()))?;
            if k_val == 0 {
                return Ok(Value::Integer(Integer::from(if n_val == 0 {
                    1
                } else {
                    0
                })));
            }
            if n_val == 0 {
                return Ok(Value::Integer(Integer::from(0)));
            }
            if k_val > n_val {
                return Ok(Value::Integer(Integer::from(0)));
            }
            // Compute using iterative 2D DP with a single row.
            let mut dp = vec![Integer::from(0); k_val + 1];
            dp[0] = Integer::from(1); // S(0, 0) = 1
            for row in 1..=n_val {
                for col in (1..=k_val.min(row)).rev() {
                    dp[col] = Integer::from(col) * dp[col].clone() + dp[col - 1].clone();
                }
                dp[0] = Integer::from(0); // S(n, 0) = 0 for n >= 1
            }
            Ok(Value::Integer(dp[k_val].clone()))
        }
        _ => Ok(Value::Call {
            head: "StirlingS2".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── StirlingS1 ────────────────────────────────────────────────────────────────

/// StirlingS1[n, k] — signed Stirling numbers of the first kind.
/// Coefficients of the rising factorial x*(x+1)*...*(x+n-1).
/// s(n, k) = s(n-1, k-1) + (n-1)*s(n-1, k), s(0,0) = 1, s(n,0) = 0 for n > 0.
pub fn builtin_stirling_s1(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "StirlingS1 requires exactly 2 arguments: StirlingS1[n, k]".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(n), Value::Integer(k)) => {
            if n.is_negative() || k.is_negative() {
                return Ok(Value::Call {
                    head: "StirlingS1".to_string(),
                    args: args.to_vec(),
                });
            }
            let n_val = n
                .to_usize()
                .ok_or_else(|| EvalError::Error("StirlingS1: n too large".to_string()))?;
            let k_val = k
                .to_usize()
                .ok_or_else(|| EvalError::Error("StirlingS1: k too large".to_string()))?;
            if k_val == 0 {
                return Ok(Value::Integer(Integer::from(if n_val == 0 {
                    1
                } else {
                    0
                })));
            }
            if n_val == 0 {
                return Ok(Value::Integer(Integer::from(0)));
            }
            if k_val > n_val {
                return Ok(Value::Integer(Integer::from(0)));
            }
            // Compute using iterative 2D DP with a single row.
            let mut dp = vec![Integer::from(0); k_val + 1];
            dp[0] = Integer::from(1); // s(0, 0) = 1
            for row in 1..=n_val {
                for col in (1..=k_val.min(row)).rev() {
                    dp[col] = dp[col - 1].clone() + Integer::from(row - 1) * dp[col].clone();
                }
                dp[0] = Integer::from(0); // s(n, 0) = 0 for n >= 1
            }
            Ok(Value::Integer(dp[k_val].clone()))
        }
        _ => Ok(Value::Call {
            head: "StirlingS1".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── LucasL ────────────────────────────────────────────────────────────────────

/// LucasL[n] — the n-th Lucas number.
/// L(0) = 2, L(1) = 1, L(n) = L(n-1) + L(n-2).
pub fn builtin_lucas_l(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "LucasL requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if !n.is_negative() => {
            let n_val = n
                .to_usize()
                .ok_or_else(|| EvalError::Error("LucasL: n too large".to_string()))?;
            match n_val {
                0 => Ok(Value::Integer(Integer::from(2))),
                1 => Ok(Value::Integer(Integer::from(1))),
                _ => {
                    let mut prev = Integer::from(2); // L(0)
                    let mut curr = Integer::from(1); // L(1)
                    for _ in 2..=n_val {
                        let next = prev + curr.clone();
                        prev = curr;
                        curr = next;
                    }
                    Ok(Value::Integer(curr))
                }
            }
        }
        _ => Ok(Value::Call {
            head: "LucasL".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── Fibonacci ────────────────────────────────────────────────────────────────

/// Fast doubling Fibonacci algorithm. Returns (F(n), F(n+1)).
fn fib_pair(n: usize) -> (Integer, Integer) {
    if n == 0 {
        return (Integer::from(0), Integer::from(1));
    }
    let (a, b) = fib_pair(n >> 1);
    // c = a * (2*b - a), d = a^2 + b^2
    let a2 = a.clone() * &a;
    let b2 = b.clone() * &b;
    let c = a.clone() * (Integer::from(2) * &b - &a);
    let d = a2 + b2;
    if n & 1 == 0 {
        (c, d)
    } else {
        (d.clone(), c + d)
    }
}

/// Evaluate the Fibonacci polynomial F_n(x) at the given x value.
/// Uses the recurrence F_0(x)=0, F_1(x)=1, F_n(x) = x*F_{n-1}(x) + F_{n-2}(x).
fn fibonacci_polynomial(n: usize, x: &Value) -> Result<Value, EvalError> {
    if n == 0 {
        return Ok(Value::Integer(Integer::from(0)));
    }
    if n == 1 {
        return Ok(Value::Integer(Integer::from(1)));
    }
    let mut prev = Value::Integer(Integer::from(0));
    let mut curr = Value::Integer(Integer::from(1));
    for _ in 2..=n {
        // next = x * curr + prev
        let x_times_curr = mul_val(x, &curr)?;
        let next = add_val(&x_times_curr, &prev)?;
        prev = curr;
        curr = next;
    }
    Ok(curr)
}

/// Add two Values (Integer + Integer → Integer, mixed → symbolic).
fn add_val(a: &Value, b: &Value) -> Result<Value, EvalError> {
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Ok(Value::Integer(x.clone() + y)),
        (Value::Real(x), Value::Real(y)) => Ok(Value::Real(x.clone() + y.clone())),
        (Value::Integer(x), Value::Real(y)) => {
            let xf = Float::with_val(crate::value::DEFAULT_PRECISION, x);
            Ok(Value::Real(xf + y.clone()))
        }
        (Value::Real(x), Value::Integer(y)) => {
            let yf = Float::with_val(crate::value::DEFAULT_PRECISION, y);
            Ok(Value::Real(x.clone() + yf))
        }
        _ => Ok(Value::Call {
            head: "Plus".to_string(),
            args: vec![a.clone(), b.clone()],
        }),
    }
}

/// Multiply two Values (Integer * Integer → Integer, mixed → symbolic).
fn mul_val(a: &Value, b: &Value) -> Result<Value, EvalError> {
    match (a, b) {
        (Value::Integer(x), Value::Integer(y)) => Ok(Value::Integer(x.clone() * y)),
        (Value::Real(x), Value::Real(y)) => Ok(Value::Real(x.clone() * y.clone())),
        (Value::Integer(x), Value::Real(y)) => {
            let xf = Float::with_val(crate::value::DEFAULT_PRECISION, x);
            Ok(Value::Real(xf * y.clone()))
        }
        (Value::Real(x), Value::Integer(y)) => {
            let yf = Float::with_val(crate::value::DEFAULT_PRECISION, y);
            Ok(Value::Real(x.clone() * yf))
        }
        _ => Ok(Value::Call {
            head: "Times".to_string(),
            args: vec![a.clone(), b.clone()],
        }),
    }
}

/// Fibonacci[n] — the n-th Fibonacci number using fast doubling.
/// Fibonacci[n, x] — the Fibonacci polynomial F_n(x).
/// Negative n uses F_{-n} = (-1)^(n+1) F_n.
pub fn builtin_fibonacci(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Fibonacci requires 1 or 2 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            let negative = n.is_negative();
            let m = if negative {
                (-n.clone())
                    .to_usize()
                    .ok_or_else(|| EvalError::Error("Fibonacci: n too large".to_string()))?
            } else {
                n.to_usize()
                    .ok_or_else(|| EvalError::Error("Fibonacci: n too large".to_string()))?
            };
            let sign = if negative && m % 2 == 0 { -1 } else { 1 };
            if args.len() == 1 {
                // Fast doubling
                let (f_n, _) = fib_pair(m);
                Ok(Value::Integer(Integer::from(sign) * f_n))
            } else {
                // Fibonacci polynomial, with the sign correction for negative n.
                let val = fibonacci_polynomial(m, &args[1])?;
                if sign < 0 { negate_val(&val) } else { Ok(val) }
            }
        }
        _ => Ok(Value::Call {
            head: "Fibonacci".to_string(),
            args: args.to_vec(),
        }),
    }
}

/// Negate a numeric Value, falling back to a symbolic `-1 * x`.
fn negate_val(v: &Value) -> Result<Value, EvalError> {
    match v {
        Value::Integer(x) => Ok(Value::Integer(-x.clone())),
        Value::Real(x) => Ok(Value::Real(-x.clone())),
        Value::Rational(x) => Ok(Value::Rational(Box::new(-(**x).clone()))),
        _ => Ok(Value::Call {
            head: "Times".to_string(),
            args: vec![Value::Integer(Integer::from(-1)), v.clone()],
        }),
    }
}

// ── CatalanNumber ───────────────────────────────────────────────────────────

/// CatalanNumber[n] — the n-th Catalan number = Binomial[2n, n] / (n + 1).
pub fn builtin_catalan_number(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "CatalanNumber requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if !n.is_negative() => {
            let n_val = n
                .to_usize()
                .ok_or_else(|| EvalError::Error("CatalanNumber: n too large".to_string()))?;
            // C_n = Binomial(2n, n) / (n+1)
            // Use GCD-safe computation to avoid intermediate growth.
            let mut result = Integer::from(1);
            for i in 0..n_val {
                let mut numerator = Integer::from(2 * n_val - i);
                let mut denominator = Integer::from(i + 1);
                let g = gcd_int(result.clone(), denominator.clone());
                result /= g.clone();
                denominator /= g;
                let g2 = gcd_int(numerator.clone(), denominator.clone());
                numerator /= g2.clone();
                denominator /= g2;
                result *= numerator;
                result /= denominator;
            }
            // Now divide by (n+1)
            let n_plus_1 = Integer::from(n_val + 1);
            result /= n_plus_1;
            Ok(Value::Integer(result))
        }
        _ => Ok(Value::Call {
            head: "CatalanNumber".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── HarmonicNumber ──────────────────────────────────────────────────────────

/// HarmonicNumber[n] — the n-th harmonic number = sum(1/k, k=1..n).
/// HarmonicNumber[n, r] = sum(1/k^r, k=1..n).
/// Returns Rational for exact integer results, Real for approximate.
pub fn builtin_harmonic_number(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "HarmonicNumber requires 1 or 2 arguments".to_string(),
        ));
    }
    let r = if args.len() == 2 {
        match &args[1] {
            Value::Integer(r) if r.is_positive() => r
                .to_u32()
                .ok_or_else(|| EvalError::Error("HarmonicNumber: r too large".to_string()))?,
            _ => {
                return Ok(Value::Call {
                    head: "HarmonicNumber".to_string(),
                    args: args.to_vec(),
                });
            }
        }
    } else {
        1
    };
    match &args[0] {
        Value::Integer(n) if !n.is_negative() => {
            let n_val = n
                .to_usize()
                .ok_or_else(|| EvalError::Error("HarmonicNumber: n too large".to_string()))?;
            if n_val == 0 {
                return Ok(Value::Integer(Integer::from(0)));
            }
            if r == 1 {
                // H_n = sum(1/k, k=1..n). Compute as exact rational.
                // Use rug::Rational for exact computation.
                let mut sum = rug::Rational::from(0);
                for k in 1..=n_val {
                    sum += rug::Rational::from((1, k as u64));
                }
                Ok(Value::Rational(Box::new(sum)))
            } else {
                // sum(1/k^r, k=1..n)
                let mut sum = rug::Rational::from(0);
                for k in 1..=n_val {
                    let kr = (k as u64).pow(r);
                    sum += rug::Rational::from((1, kr));
                }
                Ok(Value::Rational(Box::new(sum)))
            }
        }
        _ => Ok(Value::Call {
            head: "HarmonicNumber".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── PartitionsP ─────────────────────────────────────────────────────────────

/// PartitionsP[n] — the number of unrestricted integer partitions of n.
/// Uses the pentagonal number theorem for efficient computation.
pub fn builtin_partitions_p(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PartitionsP requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if !n.is_negative() => {
            let n_val = n
                .to_usize()
                .ok_or_else(|| EvalError::Error("PartitionsP: n too large".to_string()))?;
            Ok(Value::Integer(partitions_p(n_val)))
        }
        _ => Ok(Value::Call {
            head: "PartitionsP".to_string(),
            args: args.to_vec(),
        }),
    }
}

/// Compute p(n) using the pentagonal number theorem recurrence:
/// p(0) = 1, p(n) = sum((-1)^(k-1) * p(n - g(k)), k in Z\{0})
/// where g(k) = k(3k-1)/2 (generalized pentagonal numbers).
/// Sign pattern for pairs (pent1 from k>0, pent2 from k<0):
/// k=1: pent1 -, pent2 -; k=2: pent1 +, pent2 +; k=3: pent1 -, pent2 -; ...
fn partitions_p(n: usize) -> Integer {
    if n == 0 {
        return Integer::from(1);
    }
    let mut p = vec![Integer::from(0); n + 1];
    p[0] = Integer::from(1);
    for i in 1..=n {
        let mut sum = Integer::from(0);
        for k in 1.. {
            let pent1 = k * (3 * k - 1) / 2;
            let pent2 = k * (3 * k + 1) / 2;
            if pent1 > i {
                break;
            }
            // k odd: both pentagonal numbers contribute positively
            // k even: both contribute negatively
            let sign = if k % 2 == 1 { 1 } else { -1 };
            sum += Integer::from(sign) * &p[i - pent1];
            if pent2 <= i {
                sum += Integer::from(sign) * &p[i - pent2];
            }
        }
        p[i] = sum;
    }
    p[n].clone()
}

// ── PartitionsQ ─────────────────────────────────────────────────────────────

/// PartitionsQ[n] — the number of partitions of n into distinct parts.
/// Uses DP: q(n) = q(n-1) + q(n-2) - q(n-5) - q(n-7) + ...
/// (same pentagonal recurrence but with alternating signs in pairs).
pub fn builtin_partitions_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PartitionsQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if !n.is_negative() => {
            let n_val = n
                .to_usize()
                .ok_or_else(|| EvalError::Error("PartitionsQ: n too large".to_string()))?;
            Ok(Value::Integer(partitions_q(n_val)))
        }
        _ => Ok(Value::Call {
            head: "PartitionsQ".to_string(),
            args: args.to_vec(),
        }),
    }
}

/// Compute q(n) — partitions into distinct parts.
/// Uses DP: q(n) = q(n, n) where q(n, k) is the number of partitions of n
/// into distinct parts with each part ≤ k.
/// Recurrence: q(n, k) = q(n, k-1) + q(n-k, k-1), q(0, k) = 1, q(n, 0) = 0.
fn partitions_q(n: usize) -> Integer {
    if n == 0 {
        return Integer::from(1);
    }
    // 1D DP: q[j] = number of partitions of j into distinct parts ≤ current k
    let mut q = vec![Integer::from(0); n + 1];
    q[0] = Integer::from(1);
    for k in 1..=n {
        // Process in reverse to avoid using the same k twice
        for j in (k..=n).rev() {
            let add = q[j - k].clone();
            q[j] += add;
        }
    }
    q[n].clone()
}

// ── BellB ───────────────────────────────────────────────────────────────────

/// BellB[n] — the n-th Bell number using the Bell triangle.
/// BellB[n, k] — the partial Bell polynomial B_{n,k}.
pub fn builtin_bell_b(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "BellB requires 1 or 2 arguments".to_string(),
        ));
    }
    if let Some(n_val) = non_neg_usize(&args[0]) {
        if args.len() == 1 {
            // Bell number via Bell triangle
            return Ok(Value::Integer(bell_number(n_val)));
        }
        // BellB[n, k] — partial Bell polynomial (number of partitions into k blocks)
        if let Some(k_val) = non_neg_usize(&args[1]) {
            // BellB[n, k] in Wolfram Language = StirlingS2[n, k]
            // (the number of partitions of n elements into exactly k non-empty subsets)
            return Ok(Value::Integer(stirling_s2_val(n_val, k_val)));
        }
    }
    Ok(Value::Call {
        head: "BellB".to_string(),
        args: args.to_vec(),
    })
}

/// Compute StirlingS2(n, k) as a plain value.
fn stirling_s2_val(n: usize, k: usize) -> Integer {
    if k == 0 {
        return Integer::from(if n == 0 { 1 } else { 0 });
    }
    if n == 0 || k > n {
        return Integer::from(0);
    }
    let mut dp = vec![Integer::from(0); k + 1];
    dp[0] = Integer::from(1);
    for row in 1..=n {
        // Iterate backward to avoid overwriting values we still need
        for col in (1..=k.min(row)).rev() {
            dp[col] = Integer::from(col) * dp[col].clone() + dp[col - 1].clone();
        }
        if row >= 1 {
            dp[0] = Integer::from(0);
        }
    }
    dp[k].clone()
}

/// Compute the n-th Bell number using the Bell triangle.
fn bell_number(n: usize) -> Integer {
    if n == 0 {
        return Integer::from(1);
    }
    // Bell triangle: row 0 = [B(0)] = [1]
    // Each subsequent row starts with the last element of the previous row,
    // then each element is the sum of the element to its left and the element
    // above-left.
    let mut prev_row = vec![Integer::from(1)]; // B(0) = 1
    for _ in 1..=n {
        let mut new_row = Vec::with_capacity(prev_row.len() + 1);
        new_row.push(prev_row.last().unwrap().clone());
        for prev_val in &prev_row {
            let val = new_row.last().unwrap().clone() + prev_val;
            new_row.push(val);
        }
        prev_row = new_row;
    }
    prev_row[0].clone()
}

// ── BellNumber ───────────────────────────────────────────────────────────────

/// BellNumber[n] — the n-th Bell number (an alias for BellB[n]).
pub fn builtin_bell_number(args: &[Value]) -> Result<Value, EvalError> {
    builtin_bell_b(args)
}

// ── Derangements ─────────────────────────────────────────────────────────────

/// Derangements[n] — the number of derangements of n elements (!n).
/// Derangements[list] — all derangements (permutations with no fixed position) of list.
pub fn builtin_derangements(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Derangements requires exactly 1 argument".to_string(),
        ));
    }
    if let Some(n_val) = non_neg_usize(&args[0]) {
        return Ok(Value::Integer(subfactorial_val(n_val)));
    }
    match &args[0] {
        Value::List(items) => {
            let mut result = Vec::new();
            let used = vec![false; items.len()];
            let mut current = Vec::with_capacity(items.len());
            derange(items, &used, &mut current, &mut result);
            Ok(Value::List(result))
        }
        _ => Ok(Value::Call {
            head: "Derangements".to_string(),
            args: args.to_vec(),
        }),
    }
}

/// Subfactorial value: !n, the number of derangements of n objects.
fn subfactorial_val(n: usize) -> Integer {
    let mut curr = Integer::from(0); // !1
    match n {
        0 => Integer::from(1),
        1 => Integer::from(0),
        _ => {
            for i in 2..=n {
                let sign = if i % 2 == 0 { 1 } else { -1 };
                curr = Integer::from(i) * curr + sign;
            }
            curr
        }
    }
}

/// Backtracking generation of derangements of `items`: position `p` receives an
/// element from index `i` with `i != p`.
fn derange(items: &[Value], used: &[bool], current: &mut Vec<Value>, result: &mut Vec<Value>) {
    let pos = current.len();
    if pos == items.len() {
        result.push(Value::List(current.clone()));
        return;
    }
    for i in 0..items.len() {
        if !used[i] && i != pos {
            let mut used2 = used.to_vec();
            used2[i] = true;
            current.push(items[i].clone());
            derange(items, &used2, current, result);
            current.pop();
        }
    }
}

// ── EulerE ───────────────────────────────────────────────────────────────────

/// EulerE[n] — the n-th Euler (secant) number E_n:
/// E_0 = 1, E_n = 0 for odd n, and E_n = -sum_{even k<n} C(n,k) E_k.
pub fn builtin_euler_e(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "EulerE requires exactly 1 argument".to_string(),
        ));
    }
    if let Some(n_val) = non_neg_usize(&args[0]) {
        return Ok(Value::Integer(euler_number_e(n_val)));
    }
    Ok(Value::Call {
        head: "EulerE".to_string(),
        args: args.to_vec(),
    })
}

/// Euler (secant) number E_n; zero for odd n.
fn euler_number_e(n: usize) -> Integer {
    if n % 2 == 1 {
        return Integer::from(0);
    }
    let mut e = vec![Integer::from(0); n + 1];
    e[0] = Integer::from(1);
    for m in (2..=n).step_by(2) {
        let mut sum = Integer::from(0);
        for k in (0..m).step_by(2) {
            sum += binomial_val(m, k) * e[k].clone();
        }
        e[m] = -sum;
    }
    e[n].clone()
}

/// Exact binomial coefficient C(n, k) for plain `usize` inputs.
fn binomial_val(n: usize, k: usize) -> Integer {
    if k > n {
        return Integer::from(0);
    }
    let k = k.min(n - k);
    let mut result = Integer::from(1);
    for i in 0..k {
        result *= Integer::from(n - i);
        result /= Integer::from(i + 1);
    }
    result
}

// ── EulerNumber ──────────────────────────────────────────────────────────────

/// EulerNumber[n] — the n-th Euler zigzag (up/down) number A000111:
/// 1, 1, 1, 2, 5, 16, 61, 272, 1385, 7936, ...
pub fn builtin_euler_number(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "EulerNumber requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if !n.is_negative() => {
            let n_val = n
                .to_usize()
                .ok_or_else(|| EvalError::Error("EulerNumber: n too large".to_string()))?;
            Ok(Value::Integer(euler_zigzag(n_val)))
        }
        _ => Ok(Value::Call {
            head: "EulerNumber".to_string(),
            args: args.to_vec(),
        }),
    }
}

/// Euler zigzag (up/down) number A_n built from the Entringer triangle:
/// E(0,0) = 1, E(n,0) = 0, E(n,k) = E(n,k-1) + E(n-1,n-k); A_n = E(n,n).
fn euler_zigzag(n: usize) -> Integer {
    if n == 0 {
        return Integer::from(1);
    }
    let mut prev = vec![Integer::from(1)]; // E(0, 0)
    for m in 1..=n {
        let mut row = vec![Integer::from(0); m + 1];
        for k in 1..=m {
            row[k] = row[k - 1].clone() + prev[m - k].clone();
        }
        prev = row;
    }
    prev[n].clone()
}

// ── Permutations as cycles / signatures / orders ─────────────────────────────

/// Return the cycle list of a `Cycles[...]` value, if it is one.
fn cycles_arg(val: &Value) -> Option<&[Value]> {
    if let Value::Call { head, args } = val
        && head == "Cycles"
        && args.len() == 1
        && let Value::List(cycles) = &args[0]
    {
        return Some(cycles.as_slice());
    }
    None
}

/// Build a canonical `Cycles[{{...}, ...}]` value from plain cycles.
fn make_cycles(cycles: Vec<Vec<usize>>) -> Value {
    let cycle_values = cycles
        .into_iter()
        .map(|c| {
            Value::List(
                c.into_iter()
                    .map(|x| Value::Integer(Integer::from(x)))
                    .collect(),
            )
        })
        .collect();
    Value::Call {
        head: "Cycles".to_string(),
        args: vec![Value::List(cycle_values)],
    }
}

/// Parse a permutation image list (1-indexed positive integers) from a Value.
fn parse_perm_list(val: &Value) -> Result<Vec<usize>, EvalError> {
    let items = get_list(val)?;
    let mut perm = Vec::with_capacity(items.len());
    for it in items {
        let x = to_int(it)?;
        let xu = x.to_usize().filter(|&u| u >= 1).ok_or_else(|| {
            EvalError::Error("expected a permutation of positive integers".to_string())
        })?;
        perm.push(xu);
    }
    Ok(perm)
}

/// Convert a list of cycles to the corresponding image list.
fn cycles_to_perm(cycles: &[Value]) -> Result<Vec<usize>, EvalError> {
    let mut parsed: Vec<Vec<usize>> = Vec::with_capacity(cycles.len());
    let mut max = 0usize;
    for cyc in cycles {
        let items = get_list(cyc)?;
        let mut c = Vec::with_capacity(items.len());
        for it in items {
            let x = to_int(it)?;
            let xu = x
                .to_usize()
                .filter(|&u| u >= 1)
                .ok_or_else(|| EvalError::Error("Cycle: expected positive integers".to_string()))?;
            max = max.max(xu);
            c.push(xu);
        }
        parsed.push(c);
    }
    let mut perm: Vec<usize> = (1..=max).collect();
    for c in &parsed {
        if c.is_empty() {
            continue;
        }
        for i in 0..c.len() {
            perm[c[i] - 1] = c[(i + 1) % c.len()];
        }
    }
    Ok(perm)
}

/// Decompose a permutation image list into disjoint cycles (fixed points omitted).
fn perm_to_cycles(perm: &[usize]) -> Vec<Vec<usize>> {
    let n = perm.len();
    let mut visited = vec![false; n];
    let mut out = Vec::new();
    for start in 0..n {
        if visited[start] {
            continue;
        }
        if perm[start] == start + 1 {
            visited[start] = true;
            continue;
        }
        let mut cycle = Vec::new();
        let mut j = start;
        while !visited[j] {
            visited[j] = true;
            cycle.push(j + 1);
            j = perm[j] - 1;
        }
        out.push(cycle);
    }
    out
}

/// Whether `perm` is a permutation of `1..=perm.len()`.
fn valid_perm(perm: &[usize]) -> bool {
    let n = perm.len();
    let mut seen = vec![false; n];
    for &p in perm {
        if p == 0 || p > n || seen[p - 1] {
            return false;
        }
        seen[p - 1] = true;
    }
    true
}

/// Smallest degree that can host `perm`: at least its length and its largest image.
fn perm_degree(perm: &[usize]) -> usize {
    perm.iter().copied().max().unwrap_or(0).max(perm.len())
}

/// Pad `perm` with fixed points so it acts on at least `perm_degree(perm)` points.
fn pad_identity(mut perm: Vec<usize>) -> Vec<usize> {
    let n = perm_degree(&perm);
    while perm.len() < n {
        perm.push(perm.len() + 1);
    }
    perm
}

/// Parse a permutation from either an image list or a `Cycles[...]` value.
fn perm_from_value(val: &Value) -> Result<Vec<usize>, EvalError> {
    if let Some(cycles) = cycles_arg(val) {
        cycles_to_perm(cycles)
    } else {
        parse_perm_list(val)
    }
}

/// Sign of a permutation: (-1)^(n - number of cycles), or 0 if not a permutation.
fn perm_sign(perm: &[usize]) -> Integer {
    if !valid_perm(perm) {
        return Integer::from(0);
    }
    let n = perm.len();
    let cycles = perm_to_cycles(perm);
    let in_cycles: usize = cycles.iter().map(|c| c.len()).sum();
    let num_cycles = cycles.len() + (n - in_cycles);
    if (n - num_cycles).is_multiple_of(2) {
        Integer::from(1)
    } else {
        Integer::from(-1)
    }
}

/// Order of a permutation: LCM of its cycle lengths.
fn perm_order(perm: &[usize]) -> Integer {
    let mut l = Integer::from(1);
    for c in perm_to_cycles(perm) {
        l = lcm_int(l, Integer::from(c.len()));
    }
    l
}

/// Least common multiple of two non-negative integers.
fn lcm_int(a: Integer, b: Integer) -> Integer {
    if a.is_zero() || b.is_zero() {
        return Integer::from(0);
    }
    let g = gcd_int(a.clone(), b.clone());
    (a / g) * b
}

/// Compose two permutations: apply `a` then `b`.
fn compose_perms(a: &[usize], b: &[usize]) -> Vec<usize> {
    a.iter().map(|&x| b[x - 1]).collect()
}

/// Size of the permutation group generated by `gens` (closure enumeration).
fn group_order(gens: &[Value]) -> Result<Integer, EvalError> {
    let perms: Vec<Vec<usize>> = gens.iter().map(perm_from_value).collect::<Result<_, _>>()?;
    let n = perms.iter().map(|p| perm_degree(p)).max().unwrap_or(0);
    let mut generators: Vec<Vec<usize>> = Vec::with_capacity(perms.len());
    for mut p in perms {
        while p.len() < n {
            p.push(p.len() + 1);
        }
        generators.push(p);
    }
    let identity: Vec<usize> = (1..=n).collect();
    let mut seen = vec![identity.clone()];
    let mut queue = vec![identity];
    while let Some(p) = queue.pop() {
        for g in &generators {
            let q = compose_perms(&p, g);
            if !seen.contains(&q) {
                seen.push(q.clone());
                queue.push(q);
            }
        }
    }
    Ok(Integer::from(seen.len()))
}

/// Cycles[{{...}, ...}] — a permutation in disjoint-cycle notation.
pub fn builtin_cycles(args: &[Value]) -> Result<Value, EvalError> {
    let raw: Vec<Value> = if args.len() == 1 {
        match &args[0] {
            Value::List(items) if items.iter().all(|x| matches!(x, Value::List(_))) => {
                items.clone()
            }
            other => vec![other.clone()],
        }
    } else {
        args.to_vec()
    };
    let mut norm: Vec<Vec<usize>> = Vec::with_capacity(raw.len());
    for c in &raw {
        let items = get_list(c)?;
        let mut v = Vec::with_capacity(items.len());
        for it in items {
            let x = to_int(it)?;
            let xu = x.to_usize().filter(|&u| u >= 1).ok_or_else(|| {
                EvalError::Error("Cycles: expected positive integers".to_string())
            })?;
            v.push(xu);
        }
        if !v.is_empty() {
            norm.push(v);
        }
    }
    Ok(make_cycles(norm))
}

/// PermutationCycles[perm] — convert a permutation (image list or Cycles) to Cycles form.
pub fn builtin_permutation_cycles(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PermutationCycles requires exactly 1 argument".to_string(),
        ));
    }
    if cycles_arg(&args[0]).is_some() {
        return Ok(args[0].clone());
    }
    let perm = parse_perm_list(&args[0])?;
    if !valid_perm(&perm) {
        return Ok(Value::Call {
            head: "PermutationCycles".to_string(),
            args: args.to_vec(),
        });
    }
    Ok(make_cycles(perm_to_cycles(&perm)))
}

/// PermutationList[perm] / PermutationList[perm, n] — Cycles (or list) to image list.
pub fn builtin_permutation_list(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "PermutationList requires 1 or 2 arguments".to_string(),
        ));
    }
    let mut perm = perm_from_value(&args[0])?;
    if args.len() == 2 {
        let n = non_neg_int(&args[1])?
            .to_usize()
            .ok_or_else(|| EvalError::Error("PermutationList: length too large".to_string()))?;
        if n > perm.len() {
            perm.resize(n, 0);
            for (i, slot) in perm.iter_mut().enumerate() {
                if *slot == 0 {
                    *slot = i + 1;
                }
            }
        }
    }
    Ok(Value::List(
        perm.into_iter()
            .map(|x| Value::Integer(Integer::from(x)))
            .collect(),
    ))
}

/// Signature[perm] — the sign (+1/-1) of a permutation; 0 for a non-permutation list.
pub fn builtin_signature(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Signature requires exactly 1 argument".to_string(),
        ));
    }
    if let Value::List(_) = &args[0] {
        let perm = parse_perm_list(&args[0])?;
        return Ok(Value::Integer(perm_sign(&perm)));
    }
    if let Some(cycles) = cycles_arg(&args[0]) {
        let perm = cycles_to_perm(cycles)?;
        return Ok(Value::Integer(perm_sign(&perm)));
    }
    Ok(Value::Call {
        head: "Signature".to_string(),
        args: args.to_vec(),
    })
}

/// PermutationSignature[perm] — an alias for Signature.
pub fn builtin_permutation_signature(args: &[Value]) -> Result<Value, EvalError> {
    builtin_signature(args)
}

/// PermutationOrder[perm] — the order (LCM of cycle lengths) of a permutation.
pub fn builtin_permutation_order(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PermutationOrder requires exactly 1 argument".to_string(),
        ));
    }
    if matches!(&args[0], Value::List(_)) || cycles_arg(&args[0]).is_some() {
        let perm = pad_identity(perm_from_value(&args[0])?);
        return Ok(Value::Integer(perm_order(&perm)));
    }
    Ok(Value::Call {
        head: "PermutationOrder".to_string(),
        args: args.to_vec(),
    })
}

/// Whether a value looks like a permutation (image list or Cycles).
fn is_perm_value(v: &Value) -> bool {
    matches!(v, Value::List(_)) || cycles_arg(v).is_some()
}

/// GroupOrder[perm] — order of a permutation, or size of the group generated by
/// a list of permutations.
pub fn builtin_group_order(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "GroupOrder requires exactly 1 argument".to_string(),
        ));
    }
    let v = &args[0];
    if let Value::List(items) = v
        && items.iter().all(is_perm_value)
    {
        return Ok(Value::Integer(group_order(items)?));
    }
    if matches!(v, Value::List(_)) || cycles_arg(v).is_some() {
        let perm = pad_identity(perm_from_value(v)?);
        return Ok(Value::Integer(perm_order(&perm)));
    }
    Ok(Value::Call {
        head: "GroupOrder".to_string(),
        args: args.to_vec(),
    })
}

// ── IntegerPartitions ────────────────────────────────────────────────────────

/// IntegerPartitions[n] — all partitions of integer n as a list of lists,
/// sorted in reverse lexicographic order (Mathematica convention).
/// IntegerPartitions[n, k] — partitions with at most k parts.
/// IntegerPartitions[n, {k}] — partitions with exactly k parts.
/// IntegerPartitions[n, {kmin, kmax}] — partitions with between kmin and kmax parts.
/// IntegerPartitions[n, spec, pmax] — additionally bound the part sizes.
/// IntegerPartitions[n, spec, {pmin, pmax}] — part sizes between pmin and pmax.
pub fn builtin_integer_partitions(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 3 {
        return Err(EvalError::Error(
            "IntegerPartitions requires 1 to 3 arguments".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) if !n.is_negative() => {
            let n_val = n
                .to_usize()
                .ok_or_else(|| EvalError::Error("IntegerPartitions: n too large".to_string()))?;
            let (min_parts, max_parts) = if args.len() >= 2 {
                parse_parts_spec(&args[1])?
            } else {
                (0, None)
            };
            let (min_allowed, max_allowed) = if args.len() >= 3 {
                parse_size_spec(&args[2], n_val)?
            } else {
                (1, n_val)
            };
            let mut partitions = Vec::new();
            generate_partitions(
                n_val,
                max_parts,
                min_parts,
                max_allowed,
                min_allowed,
                &mut Vec::new(),
                &mut partitions,
            );
            Ok(Value::List(partitions))
        }
        _ => Err(EvalError::TypeError {
            expected: "non-negative Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// Parse the optional part-count specification: `k`, `{k}` or `{kmin, kmax}`.
fn parse_parts_spec(spec: &Value) -> Result<(usize, Option<usize>), EvalError> {
    match spec {
        Value::Integer(k) if k.is_positive() => {
            let ku = k
                .to_usize()
                .ok_or_else(|| EvalError::Error("IntegerPartitions: k too large".to_string()))?;
            Ok((0, Some(ku)))
        }
        Value::List(items) => match items.as_slice() {
            [k] => {
                let ku = to_non_neg_usize(k, "IntegerPartitions")?;
                Ok((ku, Some(ku)))
            }
            [kmin, kmax] => {
                let lo = to_non_neg_usize(kmin, "IntegerPartitions")?;
                let hi = to_non_neg_usize(kmax, "IntegerPartitions")?;
                Ok((lo, Some(hi)))
            }
            _ => Err(EvalError::TypeError {
                expected: "k or {kmin, kmax}".to_string(),
                got: spec.type_name().to_string(),
            }),
        },
        Value::Symbol(s) if s == "All" => Ok((0, None)),
        _ => Err(EvalError::TypeError {
            expected: "positive Integer or {kmin, kmax}".to_string(),
            got: spec.type_name().to_string(),
        }),
    }
}

/// Parse the optional part-size specification: `p`, `{p}` or `{pmin, pmax}`.
fn parse_size_spec(spec: &Value, n: usize) -> Result<(usize, usize), EvalError> {
    match spec {
        Value::Integer(p) if p.is_positive() => {
            let pu = p
                .to_usize()
                .ok_or_else(|| EvalError::Error("IntegerPartitions: part too large".to_string()))?;
            Ok((1, pu))
        }
        Value::List(items) => match items.as_slice() {
            [pmax] => {
                let hi = to_non_neg_usize(pmax, "IntegerPartitions")?;
                Ok((1, hi))
            }
            [pmin, pmax] => {
                let lo = to_non_neg_usize(pmin, "IntegerPartitions")?;
                let hi = to_non_neg_usize(pmax, "IntegerPartitions")?;
                Ok((lo, hi))
            }
            _ => Err(EvalError::TypeError {
                expected: "p or {pmin, pmax}".to_string(),
                got: spec.type_name().to_string(),
            }),
        },
        Value::Symbol(s) if s == "All" => Ok((1, n)),
        _ => Err(EvalError::TypeError {
            expected: "positive Integer or {pmin, pmax}".to_string(),
            got: spec.type_name().to_string(),
        }),
    }
}

/// Extract a non-negative `usize` from a Value, erroring on other types.
fn to_non_neg_usize(val: &Value, fname: &str) -> Result<usize, EvalError> {
    match val {
        Value::Integer(i) if !i.is_negative() => i
            .to_usize()
            .ok_or_else(|| EvalError::Error(format!("{fname}: value too large"))),
        _ => Err(EvalError::TypeError {
            expected: "non-negative Integer".to_string(),
            got: val.type_name().to_string(),
        }),
    }
}

/// Recursively generate partitions of `remaining` into parts in
/// `[min_allowed, max_allowed]`, with a number of parts in `[min_parts, max_parts]`.
/// Results are collected in reverse lexicographic order by construction.
fn generate_partitions(
    remaining: usize,
    max_parts: Option<usize>,
    min_parts: usize,
    max_allowed: usize,
    min_allowed: usize,
    current: &mut Vec<usize>,
    result: &mut Vec<Value>,
) {
    if remaining == 0 {
        if current.len() >= min_parts {
            result.push(Value::List(
                current
                    .iter()
                    .map(|&p| Value::Integer(Integer::from(p)))
                    .collect(),
            ));
        }
        return;
    }
    if let Some(k) = max_parts
        && current.len() >= k
    {
        return;
    }
    let upper = remaining.min(max_allowed);
    let lower = min_allowed.max(1);
    if lower > upper {
        return;
    }
    // Try parts from largest to smallest to get reverse lexicographic order.
    for part in (lower..=upper).rev() {
        current.push(part);
        generate_partitions(
            remaining - part,
            max_parts,
            min_parts,
            part,
            min_allowed,
            current,
            result,
        );
        current.pop();
    }
}

// ── Helper: exact factorial ──────────────────────────────────────────────────

fn integer_factorial(n: &Integer) -> Integer {
    let n = n.to_usize().unwrap_or(0);
    let mut result = Integer::from(1);
    for i in 2..=n {
        result *= Integer::from(i);
    }
    result
}

/// Register the combinatorics builtins. Called from `super::register_builtins`.
/// Add polynomial `b` to `a` in place (ascending coefficients).
fn poly_add(a: &mut Vec<Integer>, b: &[Integer]) {
    if b.len() > a.len() {
        a.resize(b.len(), Integer::from(0));
    }
    for (i, c) in b.iter().enumerate() {
        a[i] += c;
    }
}

/// Render an integer-coefficient polynomial in `q` in canonical form.
fn poly_to_value(coeffs: &[Integer], q: &Value) -> Value {
    let mut terms: Vec<Value> = Vec::new();
    for (k, c) in coeffs.iter().enumerate() {
        if c.is_zero() {
            continue;
        }
        let power = match k {
            0 => None,
            1 => Some(q.clone()),
            _ => Some(Value::Call {
                head: "Power".to_string(),
                args: vec![q.clone(), Value::Integer(Integer::from(k as i64))],
            }),
        };
        match power {
            None => terms.push(Value::Integer(c.clone())),
            Some(p) if *c == 1 => terms.push(p),
            Some(p) => terms.push(Value::Call {
                head: "Times".to_string(),
                args: vec![Value::Integer(c.clone()), p],
            }),
        }
    }
    if terms.is_empty() {
        Value::Integer(Integer::from(0))
    } else if terms.len() == 1 {
        terms.pop().unwrap()
    } else {
        Value::Call {
            head: "Plus".to_string(),
            args: terms,
        }
    }
}

/// Gaussian binomial coefficient as a polynomial in `q` (DP recurrence).
fn q_binomial_poly(n: usize, m: usize, q: &Value) -> Value {
    let mut dp: Vec<Vec<Integer>> = vec![Vec::new(); m + 1];
    dp[0] = vec![Integer::from(1)];
    for i in 1..=n {
        for j in (1..=m.min(i)).rev() {
            let shift = i - j;
            let mut add = vec![Integer::from(0); shift];
            add.extend(dp[j - 1].iter().cloned());
            let mut cur = std::mem::take(&mut dp[j]);
            poly_add(&mut cur, &add);
            dp[j] = cur;
        }
    }
    poly_to_value(&dp[m], q)
}

/// QBinomial[n, m, q] — the Gaussian (q-) binomial coefficient
/// `Π_{i=1}^{m} (1 - q^{n-m+i}) / (1 - q^i)`.
pub fn builtin_q_binomial(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "QBinomial requires exactly 3 arguments".to_string(),
        ));
    }
    let n = non_neg_int(&args[0])?;
    let m = non_neg_int(&args[1])?;
    if m > n {
        return Ok(Value::Integer(Integer::from(0)));
    }
    let m_u = m.to_usize().unwrap_or(0);
    let n_u = n.to_usize().unwrap_or(0);
    match &args[2] {
        Value::Integer(_) | Value::Rational(_) => {
            let q: rug::Rational = match &args[2] {
                Value::Integer(i) => rug::Rational::from(i.clone()),
                Value::Rational(r) => (**r).clone(),
                _ => unreachable!(),
            };
            if q == 1 {
                return builtin_binomial(&[args[0].clone(), args[1].clone()]);
            }
            let qpow = |e: usize| -> rug::Rational {
                let mut acc = rug::Rational::from(1);
                for _ in 0..e {
                    acc *= &q;
                }
                acc
            };
            let mut num = rug::Rational::from(1);
            let mut den = rug::Rational::from(1);
            for i in 1..=m_u {
                num *= rug::Rational::from(1) - qpow(n_u - m_u + i);
                den *= rug::Rational::from(1) - qpow(i);
            }
            Ok(Value::Rational(Box::new(num / den)))
        }
        Value::Real(_) => {
            let Some(q) = super::to_f64(&args[2]) else {
                return Ok(Value::Call {
                    head: "QBinomial".to_string(),
                    args: args.to_vec(),
                });
            };
            if q == 1.0 {
                return builtin_binomial(&[args[0].clone(), args[1].clone()]);
            }
            let mut num = 1.0_f64;
            let mut den = 1.0_f64;
            for i in 1..=m_u {
                num *= 1.0 - q.powi((n_u - m_u + i) as i32);
                den *= 1.0 - q.powi(i as i32);
            }
            Ok(super::real(num / den))
        }
        Value::Symbol(_) => Ok(q_binomial_poly(n_u, m_u, &args[2])),
        _ => Ok(Value::Call {
            head: "QBinomial".to_string(),
            args: args.to_vec(),
        }),
    }
}

pub fn register(env: &Env) {
    super::register_builtin(env, "Binomial", builtin_binomial);
    super::register_builtin(env, "QBinomial", builtin_q_binomial);
    super::register_builtin(env, "Multinomial", builtin_multinomial);
    super::register_builtin(env, "Factorial2", builtin_factorial2);
    super::register_builtin(env, "AlternatingFactorial", builtin_alternating_factorial);
    super::register_builtin(env, "Subfactorial", builtin_subfactorial);
    super::register_builtin(env, "Permutations", builtin_permutations);
    super::register_builtin(env, "Subsets", builtin_subsets);
    super::register_builtin(env, "Tuples", builtin_tuples);
    super::register_builtin(env, "Arrangements", builtin_arrangements);
    super::register_builtin(env, "StirlingS2", builtin_stirling_s2);
    super::register_builtin(env, "StirlingS1", builtin_stirling_s1);
    super::register_builtin(env, "LucasL", builtin_lucas_l);
    super::register_builtin(env, "BellB", builtin_bell_b);
    super::register_builtin(env, "BellNumber", builtin_bell_number);
    super::register_builtin(env, "Derangements", builtin_derangements);
    super::register_builtin(env, "EulerE", builtin_euler_e);
    super::register_builtin(env, "EulerNumber", builtin_euler_number);
    super::register_builtin(env, "Cycles", builtin_cycles);
    super::register_builtin(env, "PermutationCycles", builtin_permutation_cycles);
    super::register_builtin(env, "PermutationList", builtin_permutation_list);
    super::register_builtin(env, "PermutationSignature", builtin_permutation_signature);
    super::register_builtin(env, "Signature", builtin_signature);
    super::register_builtin(env, "PermutationOrder", builtin_permutation_order);
    super::register_builtin(env, "GroupOrder", builtin_group_order);
    super::register_builtin(env, "CatalanNumber", builtin_catalan_number);
    super::register_builtin(env, "Fibonacci", builtin_fibonacci);
    super::register_builtin(env, "HarmonicNumber", builtin_harmonic_number);
    super::register_builtin(env, "IntegerPartitions", builtin_integer_partitions);
    super::register_builtin(env, "PartitionsP", builtin_partitions_p);
    super::register_builtin(env, "PartitionsQ", builtin_partitions_q);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn list(items: Vec<Value>) -> Value {
        Value::List(items)
    }

    // ── Binomial ────────────────────────────────────────────────────────────

    #[test]
    fn test_binomial_basic() {
        assert_eq!(builtin_binomial(&[int(5), int(2)]).unwrap(), int(10));
        assert_eq!(builtin_binomial(&[int(10), int(3)]).unwrap(), int(120));
        assert_eq!(builtin_binomial(&[int(0), int(0)]).unwrap(), int(1));
        assert_eq!(builtin_binomial(&[int(5), int(5)]).unwrap(), int(1));
        assert_eq!(builtin_binomial(&[int(5), int(0)]).unwrap(), int(1));
    }

    #[test]
    fn test_binomial_edge() {
        assert_eq!(builtin_binomial(&[int(5), int(6)]).unwrap(), int(0));
        assert_eq!(builtin_binomial(&[int(5), int(-1)]).unwrap(), int(0));
        // Generalized binomial supports negative n.
        assert_eq!(builtin_binomial(&[int(-1), int(0)]).unwrap(), int(1));
        assert_eq!(builtin_binomial(&[int(-1), int(1)]).unwrap(), int(-1));
        assert_eq!(builtin_binomial(&[int(-2), int(3)]).unwrap(), int(-4));
        assert_eq!(builtin_binomial(&[int(-3), int(2)]).unwrap(), int(6));
        // Zero numerator from the falling factorial when k > n >= 0.
        assert_eq!(builtin_binomial(&[int(0), int(3)]).unwrap(), int(0));
    }

    #[test]
    fn test_binomial_exact_large() {
        // C(100, 50) = 100891344545564193334812497256
        assert_eq!(
            builtin_binomial(&[int(100), int(50)]).unwrap(),
            Value::Integer(Integer::from(100891344545564193334812497256u128))
        );
        // C(n, k) == C(n, n-k).
        let a = builtin_binomial(&[int(80), int(30)]).unwrap();
        let b = builtin_binomial(&[int(80), int(50)]).unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn test_binomial_large() {
        let result = builtin_binomial(&[int(50), int(25)]).unwrap();
        if let Value::Integer(v) = result {
            assert!(!v.is_zero());
        } else {
            panic!("Expected Integer");
        }
    }

    // ── Multinomial ─────────────────────────────────────────────────────────

    #[test]
    fn test_multinomial_basic() {
        // (1+2+3)! / (1! * 2! * 3!) = 720 / (1*2*6) = 60
        assert_eq!(
            builtin_multinomial(&[int(1), int(2), int(3)]).unwrap(),
            int(60)
        );
    }

    #[test]
    fn test_multinomial_list() {
        // Multinomial[{2, 2}] = 4! / (2! * 2!) = 6
        assert_eq!(
            builtin_multinomial(&[list(vec![int(2), int(2)])]).unwrap(),
            int(6)
        );
    }

    #[test]
    fn test_multinomial_empty() {
        let result = builtin_multinomial(&[list(vec![])]).unwrap();
        assert_eq!(result, int(1));
        // No arguments at all is also 1.
        assert_eq!(builtin_multinomial(&[]).unwrap(), int(1));
    }

    #[test]
    fn test_multinomial_single_integer() {
        // A bare integer is a one-element sequence: n! / n! = 1.
        assert_eq!(builtin_multinomial(&[int(5)]).unwrap(), int(1));
        assert_eq!(builtin_multinomial(&[int(0)]).unwrap(), int(1));
    }

    // ── Factorial2 ──────────────────────────────────────────────────────────

    #[test]
    fn test_factorial2_basic() {
        assert_eq!(builtin_factorial2(&[int(0)]).unwrap(), int(1));
        assert_eq!(builtin_factorial2(&[int(1)]).unwrap(), int(1));
        assert_eq!(builtin_factorial2(&[int(5)]).unwrap(), int(15)); // 5*3*1
        assert_eq!(builtin_factorial2(&[int(6)]).unwrap(), int(48)); // 6*4*2
    }

    #[test]
    fn test_factorial2_negative() {
        assert_eq!(builtin_factorial2(&[int(-1)]).unwrap(), int(1));
        // Factorial2[-3] = 1 / ((-(-3) - 2)!!) = 1 / (1!!) = 1
        let result = builtin_factorial2(&[int(-3)]).unwrap();
        if let Value::Call { head, args } = result {
            assert_eq!(head, "Divide");
            assert_eq!(args[0], int(1));
            assert_eq!(args[1], int(1));
        } else {
            panic!("Expected Divide");
        }
    }

    // ── AlternatingFactorial ────────────────────────────────────────────────

    #[test]
    fn test_alternating_factorial() {
        assert_eq!(builtin_alternating_factorial(&[int(0)]).unwrap(), int(0));
        assert_eq!(builtin_alternating_factorial(&[int(1)]).unwrap(), int(1));
        assert_eq!(builtin_alternating_factorial(&[int(2)]).unwrap(), int(-1)); // 1! - 2! = 1 - 2
        assert_eq!(builtin_alternating_factorial(&[int(3)]).unwrap(), int(5)); // 1 - 2 + 6
        assert_eq!(builtin_alternating_factorial(&[int(4)]).unwrap(), int(-19)); // 1 - 2 + 6 - 24
    }

    // ── Subfactorial ────────────────────────────────────────────────────────

    #[test]
    fn test_subfactorial() {
        assert_eq!(builtin_subfactorial(&[int(0)]).unwrap(), int(1));
        assert_eq!(builtin_subfactorial(&[int(1)]).unwrap(), int(0));
        assert_eq!(builtin_subfactorial(&[int(2)]).unwrap(), int(1));
        assert_eq!(builtin_subfactorial(&[int(3)]).unwrap(), int(2));
        assert_eq!(builtin_subfactorial(&[int(4)]).unwrap(), int(9));
        assert_eq!(builtin_subfactorial(&[int(5)]).unwrap(), int(44));
    }

    // ── Permutations ────────────────────────────────────────────────────────

    #[test]
    fn test_permutations_all() {
        let result = builtin_permutations(&[list(vec![int(1), int(2), int(3)])]).unwrap();
        if let Value::List(perms) = result {
            assert_eq!(perms.len(), 6);
            // Every one of the six distinct permutations must be present.
            for expected in [
                [1, 2, 3],
                [1, 3, 2],
                [2, 1, 3],
                [2, 3, 1],
                [3, 1, 2],
                [3, 2, 1],
            ] {
                let want = list(expected.iter().map(|&x| int(x)).collect());
                assert!(perms.contains(&want), "missing permutation {want:?}");
            }
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_permutations_k() {
        let result = builtin_permutations(&[list(vec![int(1), int(2), int(3)]), int(2)]).unwrap();
        if let Value::List(perms) = result {
            assert_eq!(perms.len(), 6);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_permutations_range() {
        let result = builtin_permutations(&[
            list(vec![int(1), int(2), int(3)]),
            list(vec![int(1), int(2)]),
        ])
        .unwrap();
        if let Value::List(perms) = result {
            assert_eq!(perms.len(), 9);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_permutations_duplicates() {
        // Repeated elements yield distinct permutations only.
        let result = builtin_permutations(&[list(vec![int(1), int(1)])]).unwrap();
        if let Value::List(perms) = result {
            assert_eq!(perms.len(), 1);
            assert_eq!(perms[0], list(vec![int(1), int(1)]));
        } else {
            panic!("Expected List");
        }
        let result = builtin_permutations(&[list(vec![int(1), int(1), int(2)])]).unwrap();
        if let Value::List(perms) = result {
            assert_eq!(perms.len(), 3);
        } else {
            panic!("Expected List");
        }
    }

    // ── Subsets ─────────────────────────────────────────────────────────────

    #[test]
    fn test_subsets_all() {
        let result = builtin_subsets(&[list(vec![int(1), int(2)])]).unwrap();
        if let Value::List(subs) = result {
            assert_eq!(subs.len(), 4);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_subsets_k() {
        let result = builtin_subsets(&[list(vec![int(1), int(2), int(3)]), int(2)]).unwrap();
        if let Value::List(subs) = result {
            assert_eq!(subs.len(), 3);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_subsets_range() {
        let result = builtin_subsets(&[
            list(vec![int(1), int(2), int(3)]),
            list(vec![int(1), int(2)]),
        ])
        .unwrap();
        if let Value::List(subs) = result {
            assert_eq!(subs.len(), 6);
        } else {
            panic!("Expected List");
        }
    }

    // ── Tuples ──────────────────────────────────────────────────────────────

    #[test]
    fn test_tuples_basic() {
        let result = builtin_tuples(&[list(vec![int(1), int(2)]), int(3)]).unwrap();
        if let Value::List(tuples) = result {
            assert_eq!(tuples.len(), 8);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_tuples_zero() {
        let result = builtin_tuples(&[list(vec![int(1)]), int(0)]).unwrap();
        assert_eq!(result, list(vec![list(vec![])]));
    }

    #[test]
    fn test_tuples_single() {
        let result = builtin_tuples(&[list(vec![int(1), int(2)]), int(1)]).unwrap();
        assert_eq!(result, list(vec![list(vec![int(1)]), list(vec![int(2)])]));
    }

    // ── Arrangements ────────────────────────────────────────────────────────

    #[test]
    fn test_arrangements() {
        let result = builtin_arrangements(&[list(vec![int(1), int(2), int(3)]), int(2)]).unwrap();
        if let Value::List(arrs) = result {
            assert_eq!(arrs.len(), 6);
        } else {
            panic!("Expected List");
        }
    }

    // ── StirlingS2 ──────────────────────────────────────────────────────────

    #[test]
    fn test_stirling_s2() {
        assert_eq!(builtin_stirling_s2(&[int(0), int(0)]).unwrap(), int(1));
        assert_eq!(builtin_stirling_s2(&[int(3), int(2)]).unwrap(), int(3));
        assert_eq!(builtin_stirling_s2(&[int(4), int(2)]).unwrap(), int(7));
        assert_eq!(builtin_stirling_s2(&[int(5), int(2)]).unwrap(), int(15));
        assert_eq!(builtin_stirling_s2(&[int(5), int(3)]).unwrap(), int(25));
        assert_eq!(builtin_stirling_s2(&[int(4), int(0)]).unwrap(), int(0));
        assert_eq!(builtin_stirling_s2(&[int(5), int(6)]).unwrap(), int(0));
    }

    // ── StirlingS1 ──────────────────────────────────────────────────────────

    #[test]
    fn test_stirling_s1() {
        assert_eq!(builtin_stirling_s1(&[int(0), int(0)]).unwrap(), int(1));
        assert_eq!(builtin_stirling_s1(&[int(3), int(2)]).unwrap(), int(3));
        assert_eq!(builtin_stirling_s1(&[int(4), int(2)]).unwrap(), int(11));
        assert_eq!(builtin_stirling_s1(&[int(5), int(2)]).unwrap(), int(50));
        assert_eq!(builtin_stirling_s1(&[int(4), int(0)]).unwrap(), int(0));
        assert_eq!(builtin_stirling_s1(&[int(5), int(6)]).unwrap(), int(0));
    }

    // ── LucasL ──────────────────────────────────────────────────────────────

    #[test]
    fn test_lucas_l() {
        assert_eq!(builtin_lucas_l(&[int(0)]).unwrap(), int(2));
        assert_eq!(builtin_lucas_l(&[int(1)]).unwrap(), int(1));
        assert_eq!(builtin_lucas_l(&[int(2)]).unwrap(), int(3));
        assert_eq!(builtin_lucas_l(&[int(3)]).unwrap(), int(4));
        assert_eq!(builtin_lucas_l(&[int(4)]).unwrap(), int(7));
        assert_eq!(builtin_lucas_l(&[int(5)]).unwrap(), int(11));
        assert_eq!(builtin_lucas_l(&[int(6)]).unwrap(), int(18));
        assert_eq!(builtin_lucas_l(&[int(7)]).unwrap(), int(29));
    }

    // ── Fibonacci ────────────────────────────────────────────────────────────

    #[test]
    fn test_fibonacci_basic() {
        assert_eq!(builtin_fibonacci(&[int(0)]).unwrap(), int(0));
        assert_eq!(builtin_fibonacci(&[int(1)]).unwrap(), int(1));
        assert_eq!(builtin_fibonacci(&[int(2)]).unwrap(), int(1));
        assert_eq!(builtin_fibonacci(&[int(10)]).unwrap(), int(55));
        assert_eq!(builtin_fibonacci(&[int(20)]).unwrap(), int(6765));
    }

    #[test]
    fn test_fibonacci_polynomial() {
        // F_0(x) = 0
        assert_eq!(builtin_fibonacci(&[int(0), int(5)]).unwrap(), int(0));
        // F_1(x) = 1
        assert_eq!(builtin_fibonacci(&[int(1), int(5)]).unwrap(), int(1));
        // F_2(x) = x
        assert_eq!(builtin_fibonacci(&[int(2), int(3)]).unwrap(), int(3));
        // F_3(x) = x^2 + 1
        assert_eq!(builtin_fibonacci(&[int(3), int(2)]).unwrap(), int(5));
        // F_4(x) = x^3 + 2x
        assert_eq!(builtin_fibonacci(&[int(4), int(2)]).unwrap(), int(12));
    }

    #[test]
    fn test_fibonacci_negative() {
        // F_{-n} = (-1)^(n+1) F_n.
        assert_eq!(builtin_fibonacci(&[int(-1)]).unwrap(), int(1));
        assert_eq!(builtin_fibonacci(&[int(-2)]).unwrap(), int(-1));
        assert_eq!(builtin_fibonacci(&[int(-3)]).unwrap(), int(2));
        assert_eq!(builtin_fibonacci(&[int(-10)]).unwrap(), int(-55));
    }

    // ── CatalanNumber ────────────────────────────────────────────────────────

    #[test]
    fn test_catalan_number() {
        assert_eq!(builtin_catalan_number(&[int(0)]).unwrap(), int(1));
        assert_eq!(builtin_catalan_number(&[int(1)]).unwrap(), int(1));
        assert_eq!(builtin_catalan_number(&[int(2)]).unwrap(), int(2));
        assert_eq!(builtin_catalan_number(&[int(3)]).unwrap(), int(5));
        assert_eq!(builtin_catalan_number(&[int(4)]).unwrap(), int(14));
        assert_eq!(builtin_catalan_number(&[int(5)]).unwrap(), int(42));
        assert_eq!(builtin_catalan_number(&[int(10)]).unwrap(), int(16796));
    }

    // ── HarmonicNumber ───────────────────────────────────────────────────────

    #[test]
    fn test_harmonic_number() {
        // H_0 = 0
        assert_eq!(builtin_harmonic_number(&[int(0)]).unwrap(), int(0));
        // H_1 = 1
        let result = builtin_harmonic_number(&[int(1)]).unwrap();
        match &result {
            Value::Rational(r) => assert_eq!(r.to_f64(), 1.0),
            _ => panic!("Expected Rational for H_1"),
        }
        // H_2 = 1 + 1/2 = 3/2
        let result = builtin_harmonic_number(&[int(2)]).unwrap();
        match &result {
            Value::Rational(r) => assert_eq!(r.to_f64(), 1.5),
            _ => panic!("Expected Rational for H_2"),
        }
    }

    #[test]
    fn test_harmonic_number_r() {
        // H(3, 2) = 1 + 1/4 + 1/9 = 49/36
        let result = builtin_harmonic_number(&[int(3), int(2)]).unwrap();
        match &result {
            Value::Rational(r) => {
                let expected = 1.0 + 0.25 + 1.0 / 9.0;
                assert!((r.to_f64() - expected).abs() < 1e-10);
            }
            _ => panic!("Expected Rational"),
        }
    }

    // ── PartitionsP ──────────────────────────────────────────────────────────

    #[test]
    fn test_partitions_p() {
        assert_eq!(builtin_partitions_p(&[int(0)]).unwrap(), int(1));
        assert_eq!(builtin_partitions_p(&[int(1)]).unwrap(), int(1));
        assert_eq!(builtin_partitions_p(&[int(2)]).unwrap(), int(2));
        assert_eq!(builtin_partitions_p(&[int(3)]).unwrap(), int(3));
        assert_eq!(builtin_partitions_p(&[int(4)]).unwrap(), int(5));
        assert_eq!(builtin_partitions_p(&[int(5)]).unwrap(), int(7));
        assert_eq!(builtin_partitions_p(&[int(10)]).unwrap(), int(42));
        assert_eq!(builtin_partitions_p(&[int(20)]).unwrap(), int(627));
    }

    // ── PartitionsQ ──────────────────────────────────────────────────────────

    #[test]
    fn test_partitions_q() {
        assert_eq!(builtin_partitions_q(&[int(0)]).unwrap(), int(1));
        assert_eq!(builtin_partitions_q(&[int(1)]).unwrap(), int(1));
        assert_eq!(builtin_partitions_q(&[int(2)]).unwrap(), int(1));
        assert_eq!(builtin_partitions_q(&[int(3)]).unwrap(), int(2));
        assert_eq!(builtin_partitions_q(&[int(4)]).unwrap(), int(2));
        assert_eq!(builtin_partitions_q(&[int(5)]).unwrap(), int(3));
        assert_eq!(builtin_partitions_q(&[int(10)]).unwrap(), int(10));
    }

    // ── BellB ────────────────────────────────────────────────────────────────

    #[test]
    fn test_bell_b() {
        assert_eq!(builtin_bell_b(&[int(0)]).unwrap(), int(1));
        assert_eq!(builtin_bell_b(&[int(1)]).unwrap(), int(1));
        assert_eq!(builtin_bell_b(&[int(2)]).unwrap(), int(2));
        assert_eq!(builtin_bell_b(&[int(3)]).unwrap(), int(5));
        assert_eq!(builtin_bell_b(&[int(4)]).unwrap(), int(15));
        assert_eq!(builtin_bell_b(&[int(5)]).unwrap(), int(52));
        assert_eq!(builtin_bell_b(&[int(6)]).unwrap(), int(203));
    }

    #[test]
    fn test_bell_b_nk() {
        // BellB[n, k] = StirlingS2[n, k]
        assert_eq!(builtin_bell_b(&[int(3), int(1)]).unwrap(), int(1)); // S(3,1)=1
        assert_eq!(builtin_bell_b(&[int(3), int(2)]).unwrap(), int(3)); // S(3,2)=3
        assert_eq!(builtin_bell_b(&[int(3), int(3)]).unwrap(), int(1)); // S(3,3)=1
        // Sum of BellB[n,k] for k=1..n = BellB[n]
        assert_eq!(builtin_bell_b(&[int(4), int(1)]).unwrap(), int(1));
        assert_eq!(builtin_bell_b(&[int(4), int(2)]).unwrap(), int(7));
        assert_eq!(builtin_bell_b(&[int(4), int(3)]).unwrap(), int(6));
        assert_eq!(builtin_bell_b(&[int(4), int(4)]).unwrap(), int(1));
        // Sum = 1+7+6+1 = 15 = BellB[4] ✓
    }

    // ── IntegerPartitions ────────────────────────────────────────────────────

    #[test]
    fn test_integer_partitions_basic() {
        // IntegerPartitions[0] = {{}}
        let result = builtin_integer_partitions(&[int(0)]).unwrap();
        if let Value::List(parts) = &result {
            assert_eq!(parts.len(), 1);
            assert_eq!(parts[0], list(vec![]));
        } else {
            panic!("Expected List");
        }

        // IntegerPartitions[1] = {{1}}
        let result = builtin_integer_partitions(&[int(1)]).unwrap();
        if let Value::List(parts) = &result {
            assert_eq!(parts.len(), 1);
            assert_eq!(parts[0], list(vec![int(1)]));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_integer_partitions_four() {
        // IntegerPartitions[4] = {{4}, {3,1}, {2,2}, {2,1,1}, {1,1,1,1}}
        let result = builtin_integer_partitions(&[int(4)]).unwrap();
        if let Value::List(parts) = &result {
            assert_eq!(parts.len(), 5);
            assert_eq!(parts[0], list(vec![int(4)]));
            assert_eq!(parts[1], list(vec![int(3), int(1)]));
            assert_eq!(parts[2], list(vec![int(2), int(2)]));
            assert_eq!(parts[3], list(vec![int(2), int(1), int(1)]));
            assert_eq!(parts[4], list(vec![int(1), int(1), int(1), int(1)]));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_integer_partitions_max_parts() {
        // IntegerPartitions[4, 2] = {{4}, {3,1}, {2,2}} (at most 2 parts)
        let result = builtin_integer_partitions(&[int(4), int(2)]).unwrap();
        if let Value::List(parts) = &result {
            assert_eq!(parts.len(), 3);
            assert_eq!(parts[0], list(vec![int(4)]));
            assert_eq!(parts[1], list(vec![int(3), int(1)]));
            assert_eq!(parts[2], list(vec![int(2), int(2)]));
        } else {
            panic!("Expected List");
        }

        // IntegerPartitions[4, 1] = {{4}}
        let result = builtin_integer_partitions(&[int(4), int(1)]).unwrap();
        if let Value::List(parts) = &result {
            assert_eq!(parts.len(), 1);
            assert_eq!(parts[0], list(vec![int(4)]));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_integer_partitions_count() {
        // p(5) = 7
        let result = builtin_integer_partitions(&[int(5)]).unwrap();
        if let Value::List(parts) = &result {
            assert_eq!(parts.len(), 7);
        } else {
            panic!("Expected List");
        }
        // p(10) = 42
        let result = builtin_integer_partitions(&[int(10)]).unwrap();
        if let Value::List(parts) = &result {
            assert_eq!(parts.len(), 42);
        } else {
            panic!("Expected List");
        }
    }

    // ── New helpers ─────────────────────────────────────────────────────────

    /// Build a `Cycles[{{...}, ...}]` value for tests.
    fn cycles(cs: &[&[i64]]) -> Value {
        Value::Call {
            head: "Cycles".to_string(),
            args: vec![list(
                cs.iter()
                    .map(|c| list(c.iter().map(|&x| int(x)).collect()))
                    .collect(),
            )],
        }
    }

    // ── BellNumber ──────────────────────────────────────────────────────────

    #[test]
    fn test_bell_number() {
        for (n, expected) in [
            (0, 1),
            (1, 1),
            (2, 2),
            (3, 5),
            (4, 15),
            (5, 52),
            (10, 115975),
        ] {
            assert_eq!(builtin_bell_number(&[int(n)]).unwrap(), int(expected));
        }
        // BellNumber is the same as BellB, including the two-argument form.
        assert_eq!(builtin_bell_number(&[int(5), int(2)]).unwrap(), int(15));
    }

    // ── Derangements ────────────────────────────────────────────────────────

    #[test]
    fn test_derangements_count() {
        assert_eq!(builtin_derangements(&[int(0)]).unwrap(), int(1));
        assert_eq!(builtin_derangements(&[int(1)]).unwrap(), int(0));
        assert_eq!(builtin_derangements(&[int(2)]).unwrap(), int(1));
        assert_eq!(builtin_derangements(&[int(3)]).unwrap(), int(2));
        assert_eq!(builtin_derangements(&[int(4)]).unwrap(), int(9));
        assert_eq!(builtin_derangements(&[int(5)]).unwrap(), int(44));
        assert_eq!(builtin_derangements(&[int(6)]).unwrap(), int(265));
        // Large value is exact.
        assert_eq!(
            builtin_derangements(&[int(20)]).unwrap(),
            int(895014631192902121)
        );
    }

    #[test]
    fn test_derangements_list() {
        let result = builtin_derangements(&[list(vec![int(1), int(2), int(3)])]).unwrap();
        if let Value::List(perms) = result {
            assert_eq!(perms.len(), 2);
            assert!(perms.contains(&list(vec![int(2), int(3), int(1)])));
            assert!(perms.contains(&list(vec![int(3), int(1), int(2)])));
        } else {
            panic!("Expected List");
        }
        // An empty list has exactly one (empty) derangement.
        let result = builtin_derangements(&[list(vec![])]).unwrap();
        assert_eq!(result, list(vec![list(vec![])]));
    }

    // ── EulerE ──────────────────────────────────────────────────────────────

    #[test]
    fn test_euler_e() {
        assert_eq!(builtin_euler_e(&[int(0)]).unwrap(), int(1));
        assert_eq!(builtin_euler_e(&[int(1)]).unwrap(), int(0));
        assert_eq!(builtin_euler_e(&[int(2)]).unwrap(), int(-1));
        assert_eq!(builtin_euler_e(&[int(3)]).unwrap(), int(0));
        assert_eq!(builtin_euler_e(&[int(4)]).unwrap(), int(5));
        assert_eq!(builtin_euler_e(&[int(6)]).unwrap(), int(-61));
        assert_eq!(builtin_euler_e(&[int(8)]).unwrap(), int(1385));
        assert_eq!(builtin_euler_e(&[int(10)]).unwrap(), int(-50521));
        assert_eq!(builtin_euler_e(&[int(12)]).unwrap(), int(2702765));
    }

    // ── EulerNumber (zigzag) ────────────────────────────────────────────────

    #[test]
    fn test_euler_number() {
        let expected = [
            1, 1, 1, 2, 5, 16, 61, 272, 1385, 7936, 50521, 353792, 2702765,
        ];
        for (n, &e) in expected.iter().enumerate() {
            assert_eq!(builtin_euler_number(&[int(n as i64)]).unwrap(), int(e));
        }
    }

    // ── Cycles / PermutationCycles / PermutationList ────────────────────────

    #[test]
    fn test_cycles_normalization() {
        // Cycles[{{1, 2}}] stays canonical.
        let expected = cycles(&[&[1, 2]]);
        assert_eq!(
            builtin_cycles(&[list(vec![list(vec![int(1), int(2)])])]).unwrap(),
            expected
        );
        // A bare list of cycles is also accepted.
        assert_eq!(
            builtin_cycles(&[list(vec![int(1), int(2)])]).unwrap(),
            expected
        );
    }

    #[test]
    fn test_permutation_cycles() {
        assert_eq!(
            builtin_permutation_cycles(&[list(vec![int(2), int(3), int(1)])]).unwrap(),
            cycles(&[&[1, 2, 3]])
        );
        assert_eq!(
            builtin_permutation_cycles(&[list(vec![int(2), int(1), int(3)])]).unwrap(),
            cycles(&[&[1, 2]])
        );
        // Identity has no nontrivial cycles.
        assert_eq!(
            builtin_permutation_cycles(&[list(vec![int(1), int(2), int(3)])]).unwrap(),
            cycles(&[])
        );
        // Already-cyclic input is passed through.
        let c = cycles(&[&[1, 2]]);
        assert_eq!(builtin_permutation_cycles(&[c.clone()]).unwrap(), c);
    }

    #[test]
    fn test_permutation_list() {
        assert_eq!(
            builtin_permutation_list(&[cycles(&[&[1, 2, 3]])]).unwrap(),
            list(vec![int(2), int(3), int(1)])
        );
        assert_eq!(
            builtin_permutation_list(&[cycles(&[&[1, 2]])]).unwrap(),
            list(vec![int(2), int(1)])
        );
        // With an explicit length the permutation is padded with identities.
        assert_eq!(
            builtin_permutation_list(&[cycles(&[&[1, 2]]), int(4)]).unwrap(),
            list(vec![int(2), int(1), int(3), int(4)])
        );
        // A plain image list round-trips.
        assert_eq!(
            builtin_permutation_list(&[list(vec![int(2), int(3), int(1)])]).unwrap(),
            list(vec![int(2), int(3), int(1)])
        );
    }

    // ── Signature ───────────────────────────────────────────────────────────

    #[test]
    fn test_signature() {
        // Even permutation (3-cycle).
        assert_eq!(
            builtin_signature(&[list(vec![int(2), int(3), int(1)])]).unwrap(),
            int(1)
        );
        // Odd permutation (transposition).
        assert_eq!(
            builtin_signature(&[list(vec![int(2), int(1), int(3)])]).unwrap(),
            int(-1)
        );
        assert_eq!(
            builtin_signature(&[list(vec![int(1), int(2), int(3)])]).unwrap(),
            int(1)
        );
        // A repeated element is not a permutation → 0.
        assert_eq!(
            builtin_signature(&[list(vec![int(2), int(1), int(1)])]).unwrap(),
            int(0)
        );
        // Cycles input.
        assert_eq!(builtin_signature(&[cycles(&[&[1, 2]])]).unwrap(), int(-1));
        assert_eq!(
            builtin_permutation_signature(&[list(vec![int(2), int(3), int(1)])]).unwrap(),
            int(1)
        );
    }

    // ── PermutationOrder / GroupOrder ───────────────────────────────────────

    #[test]
    fn test_permutation_order() {
        assert_eq!(
            builtin_permutation_order(&[list(vec![int(2), int(3), int(1)])]).unwrap(),
            int(3)
        );
        assert_eq!(
            builtin_permutation_order(&[cycles(&[&[1, 2], &[3, 4]])]).unwrap(),
            int(2)
        );
        assert_eq!(
            builtin_permutation_order(&[cycles(&[&[1, 2, 3], &[4, 5]])]).unwrap(),
            int(6)
        );
        assert_eq!(
            builtin_permutation_order(&[list(vec![int(1), int(2), int(3)])]).unwrap(),
            int(1)
        );
        // A short image list is padded to a valid degree: {2,3} acts as the 3-cycle.
        assert_eq!(
            builtin_permutation_order(&[list(vec![int(2), int(3)])]).unwrap(),
            int(3)
        );
    }

    #[test]
    fn test_group_order() {
        // Single permutation.
        assert_eq!(
            builtin_group_order(&[cycles(&[&[1, 2, 3]])]).unwrap(),
            int(3)
        );
        // Group generated by (1 2) and (2 3) is S_3, of order 6.
        assert_eq!(
            builtin_group_order(&[list(vec![cycles(&[&[1, 2]]), cycles(&[&[2, 3]])])]).unwrap(),
            int(6)
        );
        // Trivial group.
        assert_eq!(builtin_group_order(&[list(vec![])]).unwrap(), int(1));
        // Raw image-list generators are padded to a common degree.
        assert_eq!(
            builtin_group_order(&[list(vec![
                list(vec![int(1), int(2)]),
                list(vec![int(2), int(3)]),
            ])])
            .unwrap(),
            int(3)
        );
    }

    // ── IntegerPartitions extras ────────────────────────────────────────────

    #[test]
    fn test_integer_partitions_exact_parts() {
        // Exactly 2 parts.
        let result = builtin_integer_partitions(&[int(5), list(vec![int(2)])]).unwrap();
        if let Value::List(parts) = &result {
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0], list(vec![int(4), int(1)]));
            assert_eq!(parts[1], list(vec![int(3), int(2)]));
        } else {
            panic!("Expected List");
        }
        // Between 2 and 3 parts.
        let result = builtin_integer_partitions(&[int(5), list(vec![int(2), int(3)])]).unwrap();
        if let Value::List(parts) = &result {
            assert_eq!(parts.len(), 4);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_integer_partitions_bounded_sizes() {
        // At most 3 parts, each part between 2 and 4.
        let result =
            builtin_integer_partitions(&[int(6), int(3), list(vec![int(2), int(4)])]).unwrap();
        if let Value::List(parts) = &result {
            assert_eq!(parts.len(), 3);
            assert!(parts.contains(&list(vec![int(4), int(2)])));
            assert!(parts.contains(&list(vec![int(3), int(3)])));
            assert!(parts.contains(&list(vec![int(2), int(2), int(2)])));
        } else {
            panic!("Expected List");
        }
        // At most 2 parts, each part ≤ 3: only {3, 2}.
        let result = builtin_integer_partitions(&[int(5), int(2), int(3)]).unwrap();
        if let Value::List(parts) = &result {
            assert_eq!(parts.len(), 1);
            assert_eq!(parts[0], list(vec![int(3), int(2)]));
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_integer_partitions_all_and_pmax() {
        // `All` for both specs reproduces the unrestricted partition count.
        let all = Value::Symbol("All".to_string());
        let result = builtin_integer_partitions(&[int(4), all.clone(), all]).unwrap();
        if let Value::List(parts) = &result {
            assert_eq!(parts.len(), 5);
        } else {
            panic!("Expected List");
        }
        // A bare pmax bounds the largest part.
        let result = builtin_integer_partitions(&[int(4), int(3), int(2)]).unwrap();
        if let Value::List(parts) = &result {
            assert_eq!(parts.len(), 2);
            assert_eq!(parts[0], list(vec![int(2), int(2)]));
            assert_eq!(parts[1], list(vec![int(2), int(1), int(1)]));
        } else {
            panic!("Expected List");
        }
    }

    // ── Subsets / Tuples edge cases ─────────────────────────────────────────

    #[test]
    fn test_subsets_edge_cases() {
        // Length-0 subsets.
        let result = builtin_subsets(&[list(vec![int(1), int(2), int(3)]), int(0)]).unwrap();
        assert_eq!(result, list(vec![list(vec![])]));
        // Subsets of the empty list.
        let result = builtin_subsets(&[list(vec![]), int(1)]).unwrap();
        assert_eq!(result, list(vec![]));
        // Range collapsing to just length 0.
        let result = builtin_subsets(&[
            list(vec![int(1), int(2), int(3)]),
            list(vec![int(0), int(0)]),
        ])
        .unwrap();
        assert_eq!(result, list(vec![list(vec![])]));
        // Empty input, unrestricted.
        let result = builtin_subsets(&[list(vec![])]).unwrap();
        assert_eq!(result, list(vec![list(vec![])]));
    }

    #[test]
    fn test_tuples_edge_cases() {
        // Tuples from the empty set is empty for n > 0.
        let result = builtin_tuples(&[list(vec![]), int(2)]).unwrap();
        assert_eq!(result, list(vec![]));
        // Length-0 tuples.
        let result = builtin_tuples(&[list(vec![int(1), int(2)]), int(0)]).unwrap();
        assert_eq!(result, list(vec![list(vec![])]));
        // Empty set with n = 0 still yields the empty tuple.
        let result = builtin_tuples(&[list(vec![]), int(0)]).unwrap();
        assert_eq!(result, list(vec![list(vec![])]));
        // Cardinality check.
        let result = builtin_tuples(&[list(vec![int(1), int(2)]), int(4)]).unwrap();
        if let Value::List(t) = result {
            assert_eq!(t.len(), 16);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_q_binomial_exact() {
        assert_eq!(
            builtin_q_binomial(&[int(5), int(2), int(1)]).unwrap(),
            int(10)
        );
        assert_eq!(
            builtin_q_binomial(&[int(6), int(3), int(2)]).unwrap(),
            int(1395)
        );
    }

    #[test]
    fn test_q_binomial_symbolic() {
        let out = builtin_q_binomial(&[int(5), int(2), Value::Symbol("q".to_string())])
            .unwrap()
            .to_string();
        assert_eq!(
            out,
            "Plus[1, q, Times[2, Power[q, 2]], Times[2, Power[q, 3]], Times[2, Power[q, 4]], Power[q, 5], Power[q, 6]]"
        );
    }
}
