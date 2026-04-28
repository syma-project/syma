use crate::value::{EvalError, Value};
use rug::Integer;

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
        Value::Integer(n) => Err(EvalError::TypeError {
            expected: "non-negative Integer".to_string(),
            got: "negative Integer".to_string(),
        }),
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: val.type_name().to_string(),
        }),
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
            if k.is_negative() || n.is_negative() || *k > *n {
                return Ok(Value::Integer(Integer::from(0)));
            }
            if k.is_zero() || *k == *n {
                return Ok(Value::Integer(Integer::from(1)));
            }
            let k_usize = k
                .to_usize()
                .ok_or_else(|| EvalError::Error("Binomial: k too large".to_string()))?;
            let n_usize = n
                .to_usize()
                .ok_or_else(|| EvalError::Error("Binomial: n too large".to_string()))?;
            // Use symmetry: C(n, k) = C(n, n-k) to minimize work.
            let k_eff = k_usize.min(n_usize - k_usize);
            let mut result = Integer::from(1);
            for i in 0..k_eff {
                // Multiply by (n - i), then divide by (i + 1), reducing by GCD each step.
                let mut numerator = Integer::from(n_usize - i);
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
/// Returns (n1+n2+...!) / (n1! * n2! * ...).
pub fn builtin_multinomial(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 1 {
        return Err(EvalError::Error(
            "Multinomial requires at least 1 argument".to_string(),
        ));
    }
    // Handle Multinomial[{n1, n2, ...}] (single list argument)
    let ints: Vec<Integer> = if args.len() == 1 {
        get_list(&args[0])?
            .iter()
            .map(to_int)
            .collect::<Result<Vec<_>, _>>()?
    } else {
        args.iter().map(to_int).collect::<Result<Vec<_>, _>>()?
    };
    if ints.is_empty() {
        return Ok(Value::Integer(Integer::from(1)));
    }
    let sum: Integer = ints.iter().cloned().fold(Integer::from(0), |a, b| a + b);
    let mut result = integer_factorial(&sum);
    for k in &ints {
        result /= integer_factorial(&k);
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
            if *n <= Integer::from(-2) {
                // Negative odd n: 1 / ((-n - 2)!!)
                let neg = -n.clone();
                let adjusted = neg - Integer::from(2);
                let denom = integer_double_factorial(&adjusted);
                Ok(Value::Call {
                    head: "Divide".to_string(),
                    args: vec![
                        Value::Integer(Integer::from(1)),
                        Value::Integer(denom),
                    ],
                })
            } else if *n == Integer::from(0) || *n == Integer::from(-1) {
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
    while i > Integer::from(0) {
        result *= i.clone();
        i -= Integer::from(2);
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
            // Iterative: !n = n * !(n-1) + (-1)^n.
            let mut prev = Integer::from(1); // !0
            let mut curr = Integer::from(0); // !1
            for i in 2..=n_val {
                let sign = if i % 2 == 0 {
                    Integer::from(1)
                } else {
                    Integer::from(-1)
                };
                curr = Integer::from(i) * prev.clone() + sign;
                prev = curr.clone();
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
        // All permutations of length len.
        let mut result = Vec::new();
        permute(items, len, &mut result);
        Ok(Value::List(result))
    } else if args.len() == 2 {
        // Parse spec.
        let perms = generate_permutations(items, &args[1])?;
        Ok(Value::List(perms))
    } else {
        Err(EvalError::Error(
            "Permutations: usage is Permutations[list], Permutations[list, k], or Permutations[list, {kmin, kmax}]"
                .to_string(),
        ))
    }
}

/// Generate permutations with a specification (k or {kmin, kmax}).
fn generate_permutations(
    items: &[Value],
    spec: &Value,
) -> Result<Vec<Value>, EvalError> {
    match spec {
        Value::Integer(k) => {
            let k = *k;
            if k <= Integer::from(0) {
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
            let mut current = Vec::with_capacity(k_usize);
            partial_permutations(items, &used, &mut current, k_usize, &mut result);
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
    if n == 1 {
        result.push(Value::List(vec![items[0].clone()]));
        return;
    }
    heap_permute(items.to_vec(), n, result);
}

fn heap_permute(items: Vec<Value>, n: usize, result: &mut Vec<Value>) {
    if n == 1 {
        result.push(Value::List(items));
        return;
    }
    let mut items = items;
    for i in 0..n {
        heap_permute(items.clone(), n - 1, result);
        if n % 2 == 1 {
            items.swap(0, n - 1);
        } else {
            items.swap(i, n - 1);
        }
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

fn generate_subsets_of_size(
    items: &[Value],
    n: usize,
    k: usize,
) -> Vec<Value> {
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
        // Next combination using standard algorithm.
        let mut j = k - 1;
        while j >= 0 && indices[j] == n - k + j {
            j -= 1;
        }
        if j == usize::MAX {
            break;
        }
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
            if k < Integer::from(0) {
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
                if k_val <= row {
                    dp[0] = Integer::from(0);
                }
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
                    dp[col] = dp[col - 1].clone()
                        + Integer::from(row - 1) * dp[col].clone();
                }
                if k_val <= row {
                    dp[0] = Integer::from(0);
                }
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

// ── Helper: exact factorial ──────────────────────────────────────────────────

fn integer_factorial(n: &Integer) -> Integer {
    let n = n.to_usize().unwrap_or(0);
    let mut result = Integer::from(1);
    for i in 2..=n {
        result *= Integer::from(i);
    }
    result
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
        assert_eq!(builtin_binomial(&[int(-1), int(0)]).unwrap(), int(0));
        assert_eq!(builtin_binomial(&[int(5), int(-1)]).unwrap(), int(0));
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
        if let Value::Call {
            head: "Divide",
            args,
        } = result
        {
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
        assert_eq!(
            builtin_alternating_factorial(&[int(2)]).unwrap(),
            int(-1)
        ); // 1! - 2! = 1 - 2
        assert_eq!(
            builtin_alternating_factorial(&[int(3)]).unwrap(),
            int(5)
        ); // 1 - 2 + 6
        assert_eq!(
            builtin_alternating_factorial(&[int(4)]).unwrap(),
            int(-19)
        ); // 1 - 2 + 6 - 24
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
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_permutations_k() {
        let result = builtin_permutations(&[list(vec![int(1), int(2), int(3)]), int(2)])
            .unwrap();
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
        assert_eq!(
            result,
            list(vec![list(vec![int(1)]), list(vec![int(2)])])
        );
    }

    // ── Arrangements ────────────────────────────────────────────────────────

    #[test]
    fn test_arrangements() {
        let result = builtin_arrangements(&[list(vec![int(1), int(2), int(3)]), int(2)])
            .unwrap();
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
}
