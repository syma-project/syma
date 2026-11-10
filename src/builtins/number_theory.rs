use crate::env::Env;
use crate::eval::apply_function;
use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;
use rug::Rational;
use rug::integer::IsPrime;
use rug::ops::Pow;

// ── Primality ─────────────────────────────────────────────────────────────

/// Miller-Rabin witnesses that make the test deterministic for every `u64`
/// (Jim Sinclair's seven-base set).
const WITNESSES: &[u64] = &[2, 325, 9375, 28178, 450775, 9780504, 1795265022];

fn mod_pow(mut base: u64, mut exp: u64, modulus: u64) -> u64 {
    if modulus == 1 {
        return 0;
    }
    let mut result = 1u64;
    base %= modulus;
    while exp > 0 {
        if exp & 1 == 1 {
            result = (result as u128 * base as u128 % modulus as u128) as u64;
        }
        exp >>= 1;
        base = (base as u128 * base as u128 % modulus as u128) as u64;
    }
    result
}

fn miller_rabin(n: u64, a: u64) -> bool {
    if n == 2 {
        return true;
    }
    if n.is_multiple_of(2) {
        return false;
    }
    // Reduce the witness; a witness divisible by `n` is a no-op and passes.
    let a = a % n;
    if a == 0 {
        return true;
    }
    let mut d = n - 1;
    let mut r = 0u32;
    while d.is_multiple_of(2) {
        d /= 2;
        r += 1;
    }
    let mut x = mod_pow(a, d, n);
    if x == 1 || x == n - 1 {
        return true;
    }
    for _ in 0..r - 1 {
        x = (x as u128 * x as u128 % n as u128) as u64;
        if x == n - 1 {
            return true;
        }
    }
    false
}

pub fn is_prime_u64(n: u64) -> bool {
    if n < 2 {
        return false;
    }
    if n == 2 || n == 3 || n == 5 || n == 7 {
        return true;
    }
    if n.is_multiple_of(2) || (n % Integer::from(3)).is_zero() || (n % Integer::from(5)).is_zero() {
        return false;
    }
    WITNESSES.iter().all(|&a| miller_rabin(n, a))
}

pub fn builtin_prime_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PrimeQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => Ok(bool_value(is_prime_int(n))),
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// `True`/`False` symbol helper.
fn bool_value(b: bool) -> Value {
    Value::Symbol(if b { "True" } else { "False" }.to_string())
}

/// Primality test matching Mathematica's `PrimeQ`: deterministic for values
/// that fit in `u64`, a strong probable-prime test beyond that, with negative
/// inputs handled via `|n|` (`PrimeQ[-n] == PrimeQ[n]`).
fn is_prime_int(n: &Integer) -> bool {
    let a = n.clone().abs();
    if let Some(u) = a.to_u64() {
        is_prime_u64(u)
    } else {
        a.is_probably_prime(30) != IsPrime::No
    }
}

pub fn trial_prime_check(n: &Integer) -> bool {
    if n < &Integer::from(2) {
        return false;
    }
    let mut d = Integer::from(2);
    while d.clone() * d.clone() <= *n {
        if (n.clone() % d.clone()).is_zero() {
            return false;
        }
        d += Integer::from(1);
    }
    true
}

// ── FactorInteger ─────────────────────────────────────────────────────────

/// Trial division is used below this bound before the cofactor is handed to
/// Pollard's rho. Keeping the bound modest lets the O(1)-ish small-prime work
/// finish quickly while still stripping the bulk of easy factors.
const TRIAL_BOUND: u64 = 1000;

/// Strip all prime factors `<= TRIAL_BOUND` from `n`, appending `(p, e)`
/// pairs to `factors`, and return the remaining cofactor.
fn strip_small_factors(mut n: Integer, factors: &mut Vec<(Integer, u32)>) -> Integer {
    let mut d: u64 = 2;
    while d <= TRIAL_BOUND {
        let di = Integer::from(d);
        if di.clone() * di.clone() > n {
            break;
        }
        if (n.clone() % &di).is_zero() {
            let mut e = 0u32;
            while (n.clone() % &di).is_zero() {
                n /= di.clone();
                e += 1;
            }
            factors.push((di, e));
        }
        d = if d == 2 { 3 } else { d + 2 };
    }
    n
}

/// Pollard's rho (Brent's cycle-detecting variant) for a composite, odd `n`
/// that has no small factors. Always returns a non-trivial divisor.
fn pollard_rho(n: &Integer) -> Integer {
    if (n.clone() % 2u32).is_zero() {
        return Integer::from(2);
    }
    // Deterministic constants `c` are tried in turn; a fixed `c` can cycle
    // without exposing a factor (e.g. Fermat numbers with `c = 1`).
    let mut c = Integer::from(1);
    loop {
        let g = pollard_brent(n, &c);
        if g != 1 && g != *n {
            return g;
        }
        c += 1;
    }
}

/// One Brent iteration for `f(x) = x^2 + c (mod n)`. Returns a divisor of `n`
/// (possibly `n` itself if the sequence collapsed, prompting a new `c`).
fn pollard_brent(n: &Integer, c: &Integer) -> Integer {
    let f = |x: &Integer| -> Integer {
        let mut v = x.clone();
        v *= x.clone();
        v += c.clone();
        v %= n;
        v
    };
    let one = Integer::from(1);
    let mut y = Integer::from(2);
    let mut r: u64 = 1;
    let mut q = Integer::from(1);
    let mut g = Integer::from(1);
    let mut x = Integer::from(0);
    let mut ys = Integer::from(0);
    while g == one {
        x = y.clone();
        for _ in 0..r {
            y = f(&y);
        }
        let mut k: u64 = 0;
        while k < r && g == one {
            ys = y.clone();
            let lim = std::cmp::min(128, r - k);
            for _ in 0..lim {
                y = f(&y);
                let mut diff = x.clone() - &y;
                if diff.is_negative() {
                    diff = -diff;
                }
                q = (q * &diff) % n;
            }
            g = q.clone().gcd(n);
            k += 128;
        }
        r = r.saturating_mul(2);
    }
    if g == *n {
        // The block product overshot; recover a factor step by step.
        loop {
            ys = f(&ys);
            let mut diff = x.clone() - &ys;
            if diff.is_negative() {
                diff = -diff;
            }
            g = diff.gcd(n);
            if g != one {
                break;
            }
        }
    }
    g
}

/// Returns a sorted list of `{prime, exponent}` pairs.
///
/// Trial division handles small factors; large cofactors are split with
/// Miller–Rabin (via [`is_prime_int`]) plus Pollard's rho, so the cost no
/// longer grows like `sqrt(n)` for big semiprimes.
pub fn factor_integer(n: &Integer) -> Vec<(Integer, u32)> {
    let n = n.clone().abs();
    let mut factors: Vec<(Integer, u32)> = Vec::new();
    if n <= 1 {
        return factors;
    }
    let cofactor = strip_small_factors(n, &mut factors);
    let mut stack = vec![cofactor];
    while let Some(m) = stack.pop() {
        if m <= 1 {
            continue;
        }
        if is_prime_int(&m) {
            factors.push((m, 1));
            continue;
        }
        let d = pollard_rho(&m);
        stack.push(m.clone() / d.clone());
        stack.push(d);
    }
    factors.sort_by(|a, b| a.0.cmp(&b.0));
    let mut merged: Vec<(Integer, u32)> = Vec::with_capacity(factors.len());
    for (p, e) in factors {
        match merged.last_mut() {
            Some(last) if last.0 == p => last.1 += e,
            _ => merged.push((p, e)),
        }
    }
    merged
}

/// If `n_abs > 1` is a perfect power `a^b` with `b >= 2`, return `(a, b)`
/// with the largest such `b`. Otherwise return `None`.
fn perfect_power_repr(n_abs: &Integer) -> Option<(Integer, u32)> {
    if *n_abs <= 1 {
        return None;
    }
    let factors = factor_integer(n_abs);
    let g = factors.iter().map(|(_, e)| *e).reduce(gcd_u32)?;
    if g < 2 {
        return None;
    }
    let base: Integer = factors.iter().map(|(p, e)| p.clone().pow(*e / g)).product();
    Some((base, g))
}

pub fn builtin_factor_integer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FactorInteger requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n.is_zero() {
                return Ok(Value::List(vec![]));
            }
            let factors = factor_integer(n);
            let list = factors
                .into_iter()
                .map(|(p, e)| {
                    Value::List(vec![Value::Integer(p), Value::Integer(Integer::from(e))])
                })
                .collect();
            Ok(Value::List(list))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── Divisors ──────────────────────────────────────────────────────────────

/// All positive divisors of the non-negative integer `n_abs`, ascending.
/// Returns an empty vector for zero. Divisors are built from the prime
/// factorization, so this is fast even when `n_abs` has a large prime factor.
fn positive_divisors(n_abs: &Integer) -> Vec<Integer> {
    if n_abs.is_zero() {
        return Vec::new();
    }
    let mut divs = vec![Integer::from(1)];
    for (p, e) in factor_integer(n_abs) {
        let mut next = Vec::with_capacity(divs.len() * (e as usize + 1));
        let mut pe = Integer::from(1);
        for _ in 0..=e {
            for d in &divs {
                next.push(d.clone() * &pe);
            }
            pe *= &p;
        }
        divs = next;
    }
    divs.sort();
    divs
}

pub fn builtin_divisors(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Divisors requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            let divs = positive_divisors(&n.clone().abs());
            Ok(Value::List(divs.into_iter().map(Value::Integer).collect()))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── Prime[k] ─────────────────────────────────────────────────────────────

pub fn builtin_prime(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Prime requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(k) => {
            if k <= &Integer::from(0) {
                return Err(EvalError::Error(
                    "Prime: argument must be a positive integer".to_string(),
                ));
            }
            let k = k
                .to_usize()
                .ok_or_else(|| EvalError::Error("Prime: argument too large".to_string()))?;
            let mut count = 0usize;
            let mut n = 1u64;
            loop {
                n += 1;
                if is_prime_u64(n) {
                    count += 1;
                    if count == k {
                        return Ok(Value::Integer(Integer::from(n)));
                    }
                }
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── PrimePi ───────────────────────────────────────────────────────────────

pub fn builtin_prime_pi(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PrimePi requires exactly 1 argument".to_string(),
        ));
    }
    let x = match &args[0] {
        Value::Integer(n) => n.to_u64().unwrap_or(0),
        Value::Real(r) => r.to_f64() as u64,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Number".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let count = (2..=x).filter(|&n| is_prime_u64(n)).count();
    Ok(Value::Integer(Integer::from(count)))
}

// ── NextPrime ─────────────────────────────────────────────────────────────

pub fn builtin_next_prime(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "NextPrime requires 1 or 2 arguments: NextPrime[n, k]".to_string(),
        ));
    }
    let start = match &args[0] {
        Value::Integer(n) => n.clone(),
        Value::Real(r) => r
            .to_integer()
            .ok_or_else(|| EvalError::Error("NextPrime: argument too large".to_string()))?,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Number".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let k = if args.len() == 2 {
        match &args[1] {
            Value::Integer(k) => k
                .to_i64()
                .ok_or_else(|| EvalError::Error("NextPrime: k is too large".to_string()))?,
            _ => {
                return Err(EvalError::TypeError {
                    expected: "Integer".to_string(),
                    got: args[1].type_name().to_string(),
                });
            }
        }
    } else {
        1
    };
    if k == 0 {
        // The "0th" next prime is the largest prime <= n (n itself if prime).
        if is_prime_int(&start) {
            return Ok(Value::Integer(start));
        }
        let mut m = start;
        loop {
            m -= 1;
            if is_prime_int(&m) {
                return Ok(Value::Integer(m));
            }
        }
    }
    let step = Integer::from(k.signum());
    let mut n = start;
    for _ in 0..k.unsigned_abs() {
        loop {
            n += step.clone();
            if is_prime_int(&n) {
                break;
            }
        }
    }
    Ok(Value::Integer(n))
}

// ── PowerMod ──────────────────────────────────────────────────────────────

pub fn builtin_power_mod(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "PowerMod requires exactly 3 arguments: PowerMod[a, b, n]".to_string(),
        ));
    }
    match (&args[0], &args[1], &args[2]) {
        (Value::Integer(a), Value::Integer(b), Value::Integer(n)) => {
            if n.is_zero() {
                return Err(EvalError::Error(
                    "PowerMod: modulus cannot be zero".to_string(),
                ));
            }
            let modulus = n.clone().abs();
            // Reduce the base into [0, |n|) so negative bases are handled.
            let base = ((a.clone() % &modulus) + &modulus) % &modulus;
            if b.is_negative() {
                // a^(-b) ≡ (a^b)^(-1) (mod n).
                let pow_val = compute_power_mod_int(&base, &(-b.clone()), &modulus);
                match mod_inverse(&pow_val, &modulus) {
                    Some(i) => Ok(Value::Integer(i)),
                    None => Ok(Value::Call {
                        head: "PowerMod".to_string(),
                        args: args.to_vec(),
                    }),
                }
            } else {
                Ok(Value::Integer(compute_power_mod_int(&base, b, &modulus)))
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn mod_inverse(a: &Integer, m: &Integer) -> Option<Integer> {
    // Extended Euclidean algorithm
    let (mut old_r, mut r) = (a.clone(), m.clone());
    let (mut old_s, mut s) = (Integer::from(1), Integer::from(0));
    while !r.is_zero() {
        let q = old_r.clone() / r.clone();
        let tmp = r.clone();
        r = old_r.clone() - q.clone() * r.clone();
        old_r = tmp;
        let tmp = s.clone();
        s = old_s.clone() - q.clone() * s.clone();
        old_s = tmp;
    }
    if old_r != 1 {
        return None; // Not invertible
    }
    let result = (old_s % m.clone() + m.clone()) % m.clone();
    Some(result)
}

// ── EulerPhi ──────────────────────────────────────────────────────────────

pub fn builtin_euler_phi(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "EulerPhi requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            // Mathematica: EulerPhi[0] == 0 and EulerPhi[-n] == EulerPhi[n].
            let n_abs = n.clone().abs();
            if n_abs.is_zero() {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let factors = factor_integer(&n_abs);
            let mut result = n_abs.clone();
            for (p, _) in &factors {
                result = result.clone() / p.clone() * (p.clone() - Integer::from(1));
            }
            Ok(Value::Integer(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── MoebiusMu ─────────────────────────────────────────────────────────────

pub fn builtin_moebius_mu(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "MoebiusMu requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            // Mathematica: MoebiusMu[0] == 0 and MoebiusMu[-n] == MoebiusMu[n].
            let n_abs = n.clone().abs();
            if n_abs.is_zero() {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let factors = factor_integer(&n_abs);
            // If any prime factor has exponent > 1, μ(n) = 0
            for (_, e) in &factors {
                if *e > 1 {
                    return Ok(Value::Integer(Integer::from(0)));
                }
            }
            // Otherwise μ(n) = (-1)^(number of distinct prime factors)
            let sign = if factors.len().is_multiple_of(2) {
                1
            } else {
                -1
            };
            Ok(Value::Integer(Integer::from(sign)))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── DivisorSigma ──────────────────────────────────────────────────────────

pub fn builtin_divisor_sigma(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "DivisorSigma requires 2 arguments: DivisorSigma[k, n]".to_string(),
        ));
    }
    let k = match &args[0] {
        Value::Integer(k) => k.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let n_abs = match &args[1] {
        Value::Integer(n) => n.clone().abs(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };
    if n_abs.is_zero() {
        return Ok(Value::Integer(Integer::from(0)));
    }
    let divs = positive_divisors(&n_abs);
    if k.is_negative() {
        // DivisorSigma[-k, n] = sum over divisors of 1/d^k, an exact rational.
        let e = (-k)
            .to_u32()
            .ok_or_else(|| EvalError::Error("DivisorSigma: |k| is too large".to_string()))?;
        let mut sum = Rational::from(0);
        for d in &divs {
            sum += Rational::from((Integer::from(1), d.clone().pow(e)));
        }
        Ok(Value::Rational(Box::new(sum)))
    } else {
        let e = k.to_u32().ok_or_else(|| {
            EvalError::Error("DivisorSigma: k must be a non-negative integer".to_string())
        })?;
        let mut sum = Integer::from(0);
        for d in &divs {
            sum += d.clone().pow(e);
        }
        Ok(Value::Integer(sum))
    }
}

// ── Divisible / CoprimeQ ──────────────────────────────────────────────────

pub fn builtin_divisible(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Divisible requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(m), Value::Integer(n)) => {
            if n.is_zero() {
                return Err(EvalError::Error(
                    "Divisible: second argument cannot be zero".to_string(),
                ));
            }
            let r = m.clone() % n.clone();
            Ok(Value::Symbol(
                if r.is_zero() { "True" } else { "False" }.to_string(),
            ))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_coprime_q(args: &[Value]) -> Result<Value, EvalError> {
    // Accept both `CoprimeQ[a, b, ...]` and a single list `CoprimeQ[{a, b, ...}]`.
    let flat: Vec<&Value> = match args {
        [Value::List(items)] => items.iter().collect(),
        _ => args.iter().collect(),
    };
    if flat.len() < 2 {
        return Ok(Value::Symbol("True".to_string()));
    }
    for i in 0..flat.len() {
        for j in (i + 1)..flat.len() {
            match (flat[i], flat[j]) {
                (Value::Integer(a), Value::Integer(b)) => {
                    let g = gcd_int(a.clone().abs(), b.clone().abs());
                    if g != 1 {
                        return Ok(Value::Symbol("False".to_string()));
                    }
                }
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: flat[i].type_name().to_string(),
                    });
                }
            }
        }
    }
    Ok(Value::Symbol("True".to_string()))
}

fn gcd_int(mut a: Integer, mut b: Integer) -> Integer {
    while !b.is_zero() {
        let t = b.clone();
        b = a % b;
        a = t;
    }
    a
}

// ── IntegerDigits ─────────────────────────────────────────────────────────

pub fn builtin_integer_digits(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 3 {
        return Err(EvalError::Error(
            "IntegerDigits requires 1 to 3 arguments: IntegerDigits[n, b, len]".to_string(),
        ));
    }
    let base = if args.len() >= 2 {
        match &args[1] {
            Value::Integer(b) if *b >= 1 => b.clone(),
            _ => {
                return Err(EvalError::Error(
                    "IntegerDigits: base must be an integer >= 1".to_string(),
                ));
            }
        }
    } else {
        Integer::from(10)
    };
    let n = match &args[0] {
        Value::Integer(n) => n.clone().abs(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    // Base 1 ("unary") cannot use the usual division loop (n/1 == n), so it is
    // handled separately: one digit per unit.
    let mut digits: Vec<Integer> = if n.is_zero() {
        vec![Integer::from(0)]
    } else if base == 1 {
        vec![Integer::from(1); n.to_usize().unwrap_or(0)]
    } else {
        let mut m = n;
        let mut ds = Vec::new();
        while m > 0 {
            ds.push(m.clone() % &base);
            m /= &base;
        }
        ds.reverse();
        ds
    };
    if args.len() == 3 {
        let len = match &args[2] {
            Value::Integer(l) if !l.is_negative() => l.to_usize().ok_or_else(|| {
                EvalError::Error("IntegerDigits: length is too large".to_string())
            })?,
            _ => {
                return Err(EvalError::Error(
                    "IntegerDigits: length must be a non-negative integer".to_string(),
                ));
            }
        };
        if len < digits.len() {
            // Keep only the `len` least significant digits.
            digits = digits.split_off(digits.len() - len);
        } else {
            while digits.len() < len {
                digits.insert(0, Integer::from(0));
            }
        }
    }
    Ok(Value::List(
        digits.into_iter().map(Value::Integer).collect(),
    ))
}

// ── IntegerLength ───────────────────────────────────────────────────────────

/// `IntegerLength[n]` / `IntegerLength[n, b]` — the number of base-`b` digits
/// in `|n|` (most significant first, sign discarded). Following Mathematica,
/// `IntegerLength[0]` is 0. Works for arbitrarily large bases.
pub fn builtin_integer_length(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "IntegerLength requires 1 or 2 arguments: IntegerLength[n, b]".to_string(),
        ));
    }
    let base = if args.len() == 2 {
        match &args[1] {
            Value::Integer(b) if *b >= 2 => b.clone(),
            _ => {
                return Err(EvalError::Error(
                    "IntegerLength: base must be an integer >= 2".to_string(),
                ));
            }
        }
    } else {
        Integer::from(10)
    };
    match &args[0] {
        Value::Integer(n) => {
            let mut m = n.clone().abs();
            if m.is_zero() {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let mut len: u64 = 0;
            while m > 0 {
                m /= &base;
                len += 1;
            }
            Ok(Value::Integer(Integer::from(len)))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── Modular exponentiation with Integer exponent ─────────────────────

fn compute_power_mod_int(base: &Integer, exp: &Integer, modulus: &Integer) -> Integer {
    if modulus == &Integer::from(1) {
        return Integer::from(0);
    }
    let mut result = Integer::from(1);
    let mut base_val = base.clone() % modulus;
    let mut exp_val = exp.clone();
    while exp_val > 0 {
        if (exp_val.clone() % Integer::from(2)) != 0 {
            result = (result * base_val.clone()) % modulus;
        }
        exp_val >>= 1u32;
        base_val = (base_val.clone() * base_val.clone()) % modulus;
    }
    result
}

// ── ModularInverse ─────────────────────────────────────────────────────

pub fn builtin_modular_inverse(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ModularInverse requires exactly 2 arguments: ModularInverse[a, m]".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(m)) => {
            if m.is_zero() {
                return Err(EvalError::Error(
                    "ModularInverse: modulus cannot be zero".to_string(),
                ));
            }
            let a = ((a.clone() % m.clone()) + m.clone()) % m.clone();
            match mod_inverse(&a, m) {
                Some(inv) => Ok(Value::Integer(inv)),
                None => Ok(Value::Call {
                    head: "ModularInverse".to_string(),
                    args: args.to_vec(),
                }),
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── PrimeOmega ─────────────────────────────────────────────────────────

pub fn builtin_prime_omega(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PrimeOmega requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n <= &Integer::from(1) {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let factors = factor_integer(n);
            let total: u32 = factors.iter().map(|(_, e)| e).sum();
            Ok(Value::Integer(Integer::from(total)))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── PrimeNu ────────────────────────────────────────────────────────────

pub fn builtin_prime_nu(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PrimeNu requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n <= &Integer::from(1) {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let factors = factor_integer(n);
            Ok(Value::Integer(Integer::from(factors.len() as u32)))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── DigitCount ─────────────────────────────────────────────────────────

pub fn builtin_digit_count(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 3 {
        return Err(EvalError::Error(
            "DigitCount requires 1 to 3 arguments: DigitCount[n, base, digit]".to_string(),
        ));
    }
    let base: u32 = if args.len() >= 2 {
        match &args[1] {
            Value::Integer(b) if *b >= 2 => b.to_u32().unwrap_or(10),
            _ => {
                return Err(EvalError::Error(
                    "DigitCount: base must be an integer >= 2".to_string(),
                ));
            }
        }
    } else {
        10
    };
    let specific_digit = if args.len() == 3 {
        match &args[2] {
            Value::Integer(d) if *d >= 0 && *d < base => d.to_u32().unwrap(),
            _ => {
                return Err(EvalError::Error(
                    "DigitCount: invalid digit for this base".to_string(),
                ));
            }
        }
    } else {
        u32::MAX // sentinel: return all counts
    };

    match &args[0] {
        Value::Integer(n) => {
            if n.is_zero() {
                if specific_digit != u32::MAX {
                    return Ok(Value::Integer(Integer::from(if specific_digit == 0 {
                        1
                    } else {
                        0
                    })));
                }
                let mut counts = vec![0u32; base as usize];
                counts[0] = 1;
                return Ok(Value::List(
                    counts
                        .into_iter()
                        .map(|c| Value::Integer(Integer::from(c)))
                        .collect(),
                ));
            }
            let mut m = n.clone().abs();
            let base_int = Integer::from(base);
            let mut counts = vec![0u32; base as usize];
            while m > 0 {
                let d = (m.clone() % base_int.clone()).to_u32().unwrap_or(0);
                counts[d as usize] += 1;
                m /= &base_int;
            }
            if specific_digit != u32::MAX {
                Ok(Value::Integer(Integer::from(
                    counts[specific_digit as usize],
                )))
            } else {
                Ok(Value::List(
                    counts
                        .into_iter()
                        .map(|c| Value::Integer(Integer::from(c)))
                        .collect(),
                ))
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── JacobiSymbol ──────────────────────────────────────────────────────

pub fn builtin_jacobi_symbol(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "JacobiSymbol requires exactly 2 arguments: JacobiSymbol[a, n]".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(n)) => {
            if n <= &Integer::from(0) || (n.clone() % Integer::from(2)).is_zero() {
                return Err(EvalError::Error(
                    "JacobiSymbol: second argument must be a positive odd integer".to_string(),
                ));
            }
            let result = jacobi_symbol(a, n);
            Ok(Value::Integer(Integer::from(result)))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// Compute the Jacobi symbol (a/n) using the law of quadratic reciprocity.
fn jacobi_symbol(a: &Integer, n: &Integer) -> i32 {
    let mut a = a.clone() % n.clone();
    if a.is_negative() {
        a += n.clone();
    }
    let mut n = n.clone();
    let mut t = 1i32;

    while !a.is_zero() {
        // Remove factors of 2 from a
        let mut e = 0u32;
        while (a.clone() % Integer::from(2)).is_zero() {
            a /= 2u32;
            e += 1;
        }
        if e % 2 == 1 {
            let n_mod_8 = n.clone() % 8;
            if n_mod_8 == 3 || n_mod_8 == 5 {
                t = -t;
            }
        }
        // Quadratic reciprocity
        let a_mod_4 = a.clone() % Integer::from(4);
        let n_mod_4 = n.clone() % Integer::from(4);
        if a_mod_4 == 3 && n_mod_4 == 3 {
            t = -t;
        }
        let tmp = a.clone();
        a = n % tmp.clone();
        n = tmp;
    }
    if n == 1 { t } else { 0 }
}

// ── ChineseRemainder ──────────────────────────────────────────────────

pub fn builtin_chinese_remainder(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ChineseRemainder requires exactly 2 arguments: \
             ChineseRemainder[{a1, a2, ...}, {n1, n2, ...}]"
                .to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::List(remainders), Value::List(moduli)) => {
            if remainders.len() != moduli.len() || remainders.is_empty() {
                return Err(EvalError::Error(
                    "ChineseRemainder: both lists must have the same non-zero length".to_string(),
                ));
            }
            let n_eqns = remainders.len();
            let mut rem_int = Vec::with_capacity(n_eqns);
            let mut mod_int = Vec::with_capacity(n_eqns);

            for (r, m) in remainders.iter().zip(moduli.iter()) {
                match (r, m) {
                    (Value::Integer(r), Value::Integer(n)) => {
                        if n <= &Integer::from(0) {
                            return Err(EvalError::Error(
                                "ChineseRemainder: moduli must be positive".to_string(),
                            ));
                        }
                        // Normalize remainder to [0, n-1]
                        let rn = ((r.clone() % n.clone()) + n.clone()) % n.clone();
                        rem_int.push(rn);
                        mod_int.push(n.clone());
                    }
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: "Integer".to_string(),
                            got: r.type_name().to_string(),
                        });
                    }
                }
            }

            // Check pairwise coprimality of moduli
            for i in 0..n_eqns {
                for j in (i + 1)..n_eqns {
                    let g = gcd_int(mod_int[i].clone(), mod_int[j].clone());
                    if g != 1 {
                        return Ok(Value::Call {
                            head: "ChineseRemainder".to_string(),
                            args: args.to_vec(),
                        });
                    }
                }
            }

            // CRT: result = Σ a_i * N_i * inv(N_i mod n_i)  (mod N)
            let mut n_total = Integer::from(1);
            for m in &mod_int {
                n_total *= m.clone();
            }
            let mut result = Integer::from(0);
            for i in 0..n_eqns {
                let ni = n_total.clone() / mod_int[i].clone();
                let xi = mod_inverse(&ni, &mod_int[i]).expect("pairwise coprime");
                result += rem_int[i].clone() * ni * xi;
            }
            result %= &n_total;
            if result.is_negative() {
                result += &n_total;
            }
            Ok(Value::Integer(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── MultiplicativeOrder ───────────────────────────────────────────────

pub fn builtin_multiplicative_order(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MultiplicativeOrder requires exactly 2 arguments: \
             MultiplicativeOrder[a, n]"
                .to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(n)) => {
            if n <= &Integer::from(1) {
                return Err(EvalError::Error(
                    "MultiplicativeOrder: modulus must be > 1".to_string(),
                ));
            }
            // Normalize a to [0, n-1]
            let a_norm = ((a.clone() % n.clone()) + n.clone()) % n.clone();
            let g = gcd_int(a_norm.clone(), n.clone());
            if g != 1 {
                return Ok(Value::Call {
                    head: "MultiplicativeOrder".to_string(),
                    args: args.to_vec(),
                });
            }
            // Compute φ(n)
            let phi = {
                let factors = factor_integer(n);
                let mut phi = n.clone();
                for (p, _) in &factors {
                    phi = phi.clone() / p.clone() * (p.clone() - Integer::from(1));
                }
                phi
            };
            // Find the order — try divisors of φ by removing prime factors
            let mut order = phi.clone();
            let phi_factors = factor_integer(&phi);
            for (p, _) in &phi_factors {
                while (order.clone() % p.clone()).is_zero() {
                    let candidate = order.clone() / p.clone();
                    let pow_val = compute_power_mod_int(&a_norm, &candidate, n);
                    if pow_val == 1 {
                        order = candidate;
                    } else {
                        break;
                    }
                }
            }
            Ok(Value::Integer(order))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── PrimitiveRoot / PrimitiveRootList ─────────────────────────────────

/// Euler's totient `φ(n)` for `n >= 1`, via the prime factorization.
fn euler_phi_int(n: &Integer) -> Integer {
    let mut phi = n.clone();
    for (p, _) in factor_integer(n) {
        phi = phi.clone() / p.clone() * (p.clone() - Integer::from(1));
    }
    phi
}

/// Smallest primitive root modulo `n`, or `None` when the multiplicative
/// group `(Z/nZ)*` is not cyclic.
fn smallest_primitive_root(n: &Integer) -> Option<Integer> {
    if *n == 1 {
        return Some(Integer::from(1));
    }
    if *n == 2 {
        return Some(Integer::from(1));
    }
    let phi = euler_phi_int(n);
    // Distinct prime factors of φ; a generator must miss 1 when raised to
    // φ/p for every such prime p.
    let distinct: Vec<Integer> = factor_integer(&phi)
        .iter()
        .map(|(p, _)| p.clone())
        .collect();
    let mut g = Integer::from(2);
    while g < *n {
        if gcd_int(g.clone(), n.clone()) != 1 {
            g += Integer::from(1);
            continue;
        }
        if distinct
            .iter()
            .all(|p| compute_power_mod_int(&g, &(phi.clone() / p.clone()), n) != 1)
        {
            return Some(g);
        }
        g += Integer::from(1);
    }
    None
}

/// Whether `(Z/nZ)*` is cyclic, i.e. a primitive root exists. This holds for
/// `n = 1, 2, 4, p^k, 2 p^k` with `p` an odd prime.
fn has_primitive_root(n: &Integer) -> bool {
    if *n <= 4 {
        return *n == 1 || *n == 2 || *n == 4;
    }
    let n = n.clone();
    if (n.clone() % 2u32).is_zero() {
        let m = n / 2u32;
        (m.clone() % 2u32) != 0 && factor_integer(&m).len() == 1
    } else {
        factor_integer(&n).len() == 1
    }
}

pub fn builtin_primitive_root(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PrimitiveRoot requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n <= &Integer::from(1) {
                return Err(EvalError::Error(
                    "PrimitiveRoot: argument must be > 1".to_string(),
                ));
            }
            match smallest_primitive_root(n) {
                Some(g) => Ok(Value::Integer(g)),
                None => Err(EvalError::Error(
                    "PrimitiveRoot: no primitive root exists for this modulus".to_string(),
                )),
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

/// `PrimitiveRootList[n]` — all primitive roots modulo `n`, ascending (empty
/// when none exist).
pub fn builtin_primitive_root_list(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated_call("PrimitiveRootList", args));
    }
    match &args[0] {
        Value::Integer(n) if !n.is_negative() => {
            if *n == 1 {
                return Ok(Value::List(vec![Value::Integer(Integer::from(1))]));
            }
            if !has_primitive_root(n) {
                return Ok(Value::List(vec![]));
            }
            let g = match smallest_primitive_root(n) {
                Some(g) => g,
                None => return Ok(Value::List(vec![])),
            };
            let phi = euler_phi_int(n);
            let mut roots: Vec<Integer> = Vec::new();
            let mut k = Integer::from(1);
            while k <= phi {
                if gcd_int(k.clone(), phi.clone()) == 1 {
                    roots.push(compute_power_mod_int(&g, &k, n));
                }
                k += Integer::from(1);
            }
            roots.sort();
            roots.dedup();
            Ok(Value::List(roots.into_iter().map(Value::Integer).collect()))
        }
        _ => Ok(unevaluated_call("PrimitiveRootList", args)),
    }
}

// ── PerfectNumberQ ─────────────────────────────────────────────────────

pub fn builtin_perfect_number_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PerfectNumberQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n <= &Integer::from(1) {
                return Ok(Value::Symbol("False".to_string()));
            }
            let n_abs = n.clone().abs();
            let mut sum = Integer::from(0);
            let mut d = Integer::from(1);
            while d.clone() * d.clone() <= n_abs {
                if (n_abs.clone() % d.clone()).is_zero() {
                    sum += d.clone();
                    let other = n_abs.clone() / d.clone();
                    if other != d {
                        sum += other;
                    }
                }
                d += Integer::from(1);
            }
            let perfect = sum == Integer::from(2) * &n_abs;
            Ok(Value::Symbol(
                if perfect { "True" } else { "False" }.to_string(),
            ))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── MangoldtLambda ─────────────────────────────────────────────────────

pub fn builtin_mangoldt_lambda(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "MangoldtLambda requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n <= &Integer::from(1) {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let factors = factor_integer(n);
            if factors.len() == 1 {
                // n = p^k: return ln(p) as a Real
                let (p, _) = &factors[0];
                let p_f = Float::with_val(DEFAULT_PRECISION, p);
                let log_p = p_f.ln();
                Ok(Value::Real(log_p))
            } else {
                Ok(Value::Integer(Integer::from(0)))
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── LiouvilleLambda ────────────────────────────────────────────────────

pub fn builtin_liouville_lambda(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "LiouvilleLambda requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n <= &Integer::from(1) {
                return Ok(Value::Integer(Integer::from(1)));
            }
            let factors = factor_integer(n);
            let omega: u32 = factors.iter().map(|(_, e)| e).sum();
            let val = if omega.is_multiple_of(2) { 1 } else { -1 };
            Ok(Value::Integer(Integer::from(val)))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── DivisorSum ─────────────────────────────────────────────────────────

pub fn builtin_divisor_sum(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "DivisorSum requires exactly 2 arguments: DivisorSum[n, form]".to_string(),
        ));
    }
    let n = match &args[0] {
        Value::Integer(n) => n.clone().abs(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    let form = &args[1];

    if n.is_zero() {
        return Ok(Value::Integer(Integer::from(0)));
    }

    let mut sum = Integer::from(0);
    let mut d = Integer::from(1);
    while d.clone() * d.clone() <= n {
        if (n.clone() % d.clone()).is_zero() {
            let val1 = apply_function(form, &[Value::Integer(d.clone())], env)?;
            match &val1 {
                Value::Integer(i) => sum += i.clone(),
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: val1.type_name().to_string(),
                    });
                }
            }
            let other = n.clone() / d.clone();
            if other != d {
                let val2 = apply_function(form, &[Value::Integer(other.clone())], env)?;
                match &val2 {
                    Value::Integer(i) => sum += i.clone(),
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: "Integer".to_string(),
                            got: val2.type_name().to_string(),
                        });
                    }
                }
            }
        }
        d += Integer::from(1);
    }
    Ok(Value::Integer(sum))
}

// ── PrimePowerQ ─────────────────────────────────────────────────────────────

/// PrimePowerQ[n] — true if n = p^k for some prime p, k >= 1.
pub fn builtin_prime_power_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PrimePowerQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if *n <= 1 {
                return Ok(Value::Symbol("False".to_string()));
            }
            let factors = factor_integer(n);
            // A prime power has exactly one distinct prime factor
            let result = factors.len() == 1;
            Ok(Value::Symbol(
                if result { "True" } else { "False" }.to_string(),
            ))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── SquareFreeQ ─────────────────────────────────────────────────────────────

/// SquareFreeQ[n] — true if no prime^2 divides n.
/// Uses the fact that MoebiusMu(n) != 0 iff n is squarefree.
pub fn builtin_square_free_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "SquareFreeQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n.is_zero() {
                return Ok(Value::Symbol("False".to_string()));
            }
            let n_abs = n.clone().abs();
            let factors = factor_integer(&n_abs);
            for (_, e) in &factors {
                if *e > 1 {
                    return Ok(Value::Symbol("False".to_string()));
                }
            }
            Ok(Value::Symbol("True".to_string()))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── CompositeQ ──────────────────────────────────────────────────────────────

/// CompositeQ[n] — true if n > 1 and not prime.
pub fn builtin_composite_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "CompositeQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if *n <= 1 {
                return Ok(Value::Symbol("False".to_string()));
            }
            Ok(bool_value(!is_prime_int(n)))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── PerfectPowerQ ───────────────────────────────────────────────────────────

/// PerfectPowerQ[n] — true if n = a^b for some integer a >= 1, b >= 2.
pub fn builtin_perfect_power_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "PerfectPowerQ requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if *n <= 1 {
                return Ok(Value::Symbol(
                    if *n == 1 { "True" } else { "False" }.to_string(),
                ));
            }
            let result = perfect_power_repr(&n.clone().abs()).is_some();
            Ok(Value::Symbol(
                if result { "True" } else { "False" }.to_string(),
            ))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn gcd_u32(mut a: u32, mut b: u32) -> u32 {
    while b != 0 {
        let t = b;
        b = a % b;
        a = t;
    }
    a
}

// ── IntegerExponent ─────────────────────────────────────────────────────────

/// IntegerExponent[n, b] — largest k such that b^k divides n.
/// Default b = 10.
pub fn builtin_integer_exponent(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "IntegerExponent requires 1 or 2 arguments".to_string(),
        ));
    }
    let base = if args.len() == 2 {
        match &args[1] {
            Value::Integer(b) if *b >= 2 => b.clone(),
            _ => {
                return Err(EvalError::Error(
                    "IntegerExponent: base must be an integer >= 2".to_string(),
                ));
            }
        }
    } else {
        Integer::from(10)
    };
    match &args[0] {
        Value::Integer(n) => {
            if n.is_zero() {
                // Convention: IntegerExponent[0, b] = Infinity, but return a large value
                return Ok(Value::Symbol("Infinity".to_string()));
            }
            let mut m = n.clone().abs();
            let mut count = 0u32;
            while (m.clone() % base.clone()).is_zero() {
                m /= base.clone();
                count += 1;
            }
            Ok(Value::Integer(Integer::from(count)))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── FromDigits ──────────────────────────────────────────────────────────────

/// FromDigits[list, b] — convert digit list to integer in base b (default 10).
/// Also handles string input.
pub fn builtin_from_digits(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "FromDigits requires 1 or 2 arguments".to_string(),
        ));
    }
    let base = if args.len() == 2 {
        match &args[1] {
            Value::Integer(b) if *b >= 2 => b.to_u32().unwrap_or(10),
            _ => {
                return Err(EvalError::Error(
                    "FromDigits: base must be an integer >= 2".to_string(),
                ));
            }
        }
    } else {
        10
    };
    match &args[0] {
        Value::List(digits) => {
            let base_int = Integer::from(base);
            let mut result = Integer::from(0);
            for d in digits {
                match d {
                    Value::Integer(d) => {
                        // Mathematica allows digits outside [0, base): the list
                        // is evaluated as a positional polynomial in `base`.
                        result = result * &base_int + d;
                    }
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: "Integer".to_string(),
                            got: d.type_name().to_string(),
                        });
                    }
                }
            }
            Ok(Value::Integer(result))
        }
        Value::Str(s) => {
            // Parse string as a number in the given base
            let base_int = Integer::from(base);
            let mut result = Integer::from(0);
            let chars: Vec<char> = s.chars().collect();
            let has_sign = !chars.is_empty() && (chars[0] == '-' || chars[0] == '+');
            let start = if has_sign { 1 } else { 0 };
            let negative = has_sign && chars[0] == '-';
            for &c in &chars[start..] {
                let digit = char_to_digit(c, base).ok_or_else(|| {
                    EvalError::Error(format!(
                        "FromDigits: invalid character '{}' for base {}",
                        c, base
                    ))
                })?;
                result = result * &base_int + Integer::from(digit);
            }
            if negative {
                result = -result;
            }
            Ok(Value::Integer(result))
        }
        _ => Err(EvalError::TypeError {
            expected: "List or String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn char_to_digit(c: char, base: u32) -> Option<u32> {
    let d = if c.is_ascii_digit() {
        c as u32 - '0' as u32
    } else if c.is_ascii_lowercase() {
        c as u32 - 'a' as u32 + 10
    } else if c.is_ascii_uppercase() {
        c as u32 - 'A' as u32 + 10
    } else {
        return None;
    };
    if d < base { Some(d) } else { None }
}

// ── Residue ───────────────────────────────────────────────────────────────

/// Residue[expr, {x, x0}] — compute the residue of expr at x = x0.
///
/// For rational functions p(x)/q(x) with a simple pole at x0,
/// residue = p(x0) / q'(x0).
/// Returns symbolic form for complex cases.
pub fn builtin_residue(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Residue requires exactly 2 arguments: Residue[expr, {x, x0}]".to_string(),
        ));
    }
    let _point_spec = match &args[1] {
        Value::List(items) if items.len() == 2 => items,
        _ => {
            return Err(EvalError::Error(
                "Residue: second argument must be {x, x0}".to_string(),
            ));
        }
    };
    // For now, return symbolic form — full symbolic residue computation
    // requires the symbolic engine to handle Laurent series.
    Ok(Value::Call {
        head: "Residue".to_string(),
        args: args.to_vec(),
    })
}

// ── Pochhammer (RisingFactorial) ──────────────────────────────────────────

/// Pochhammer[x, n] — the rising factorial (Pochhammer symbol):
/// x * (x+1) * (x+2) * ... * (x+n-1).
///
/// For integer x and n, computes exactly.
/// For non-integer, computes numerically.
pub fn builtin_pochhammer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Pochhammer requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(x), Value::Integer(n)) => {
            if n.is_negative() {
                return Err(EvalError::Error(
                    "Pochhammer: n must be non-negative".to_string(),
                ));
            }
            if n.is_zero() {
                return Ok(Value::Integer(Integer::from(1)));
            }
            let n_u64 = n.to_u64().unwrap_or(u64::MAX);
            if n_u64 > 10000 {
                return Err(EvalError::Error(
                    "Pochhammer: n too large for direct computation".to_string(),
                ));
            }
            let mut result = Integer::from(1);
            for i in 0..n_u64 {
                result *= x.clone() + Integer::from(i);
            }
            Ok(Value::Integer(result))
        }
        (Value::Real(x), Value::Integer(n)) => {
            if n.is_negative() {
                return Err(EvalError::Error(
                    "Pochhammer: n must be non-negative".to_string(),
                ));
            }
            if n.is_zero() {
                return Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, 1u32)));
            }
            let n_u64 = n.to_u64().unwrap_or(u64::MAX);
            let prec = x.prec();
            let mut result = Float::with_val(prec, 1u32);
            for i in 0..n_u64 {
                let term = x.clone() + Float::with_val(prec, i);
                result *= term;
            }
            Ok(Value::Real(result))
        }
        (Value::Integer(_x), Value::Real(_n)) => {
            // For non-integer n, use Gamma(x+n)/Gamma(x)
            // Fall back to symbolic for now
            Ok(Value::Call {
                head: "Pochhammer".to_string(),
                args: args.to_vec(),
            })
        }
        (Value::Real(_x), Value::Real(_n)) => {
            // For non-integer n, use Gamma(x+n)/Gamma(x)
            // Fall back to symbolic for now
            Ok(Value::Call {
                head: "Pochhammer".to_string(),
                args: args.to_vec(),
            })
        }
        // Symbolic arguments — return symbolic form
        _ => Ok(Value::Call {
            head: "Pochhammer".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── FallingFactorial ──────────────────────────────────────────────────────

/// FallingFactorial[x, n] — the falling factorial:
/// x * (x-1) * (x-2) * ... * (x-n+1).
pub fn builtin_falling_factorial(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FallingFactorial requires exactly 2 arguments".to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(x), Value::Integer(n)) => {
            if n.is_negative() {
                return Err(EvalError::Error(
                    "FallingFactorial: n must be non-negative".to_string(),
                ));
            }
            if n.is_zero() {
                return Ok(Value::Integer(Integer::from(1)));
            }
            let n_u64 = n.to_u64().unwrap_or(u64::MAX);
            if n_u64 > 10000 {
                return Err(EvalError::Error(
                    "FallingFactorial: n too large for direct computation".to_string(),
                ));
            }
            let mut result = Integer::from(1);
            for i in 0..n_u64 {
                result *= x.clone() - Integer::from(i);
            }
            Ok(Value::Integer(result))
        }
        (Value::Real(x), Value::Integer(n)) => {
            if n.is_negative() {
                return Err(EvalError::Error(
                    "FallingFactorial: n must be non-negative".to_string(),
                ));
            }
            if n.is_zero() {
                return Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, 1u32)));
            }
            let n_u64 = n.to_u64().unwrap_or(u64::MAX);
            let prec = x.prec();
            let mut result = Float::with_val(prec, 1u32);
            for i in 0..n_u64 {
                let term = x.clone() - Float::with_val(prec, i);
                result *= term;
            }
            Ok(Value::Real(result))
        }
        // Symbolic arguments — return symbolic form
        _ => Ok(Value::Call {
            head: "FallingFactorial".to_string(),
            args: args.to_vec(),
        }),
    }
}

// ── ToDigits ────────────────────────────────────────────────────────────────

/// ToDigits[n, b] — convert integer to digit list in base b (default 10).
pub fn builtin_to_digits(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "ToDigits requires 1 or 2 arguments".to_string(),
        ));
    }
    let base = if args.len() == 2 {
        match &args[1] {
            Value::Integer(b) if *b >= 2 => b.to_u32().unwrap_or(10),
            _ => {
                return Err(EvalError::Error(
                    "ToDigits: base must be an integer >= 2".to_string(),
                ));
            }
        }
    } else {
        10
    };
    match &args[0] {
        Value::Integer(n) => {
            if n.is_zero() {
                return Ok(Value::List(vec![Value::Integer(Integer::from(0))]));
            }
            let mut m = n.clone().abs();
            let base_int = Integer::from(base);
            let mut digits: Vec<Integer> = Vec::new();
            while m > 0 {
                digits.push(m.clone() % base_int.clone());
                m /= base_int.clone();
            }
            digits.reverse();
            Ok(Value::List(
                digits.into_iter().map(Value::Integer).collect(),
            ))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── ContinuedFraction ───────────────────────────────────────────────────────

/// ContinuedFraction[x] — continued fraction representation.
/// For rationals, returns the exact CF expansion.
/// ContinuedFraction[x, n] — at most n terms for reals.
pub fn builtin_continued_fraction(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "ContinuedFraction requires 1 or 2 arguments".to_string(),
        ));
    }
    let max_terms = if args.len() == 2 {
        match &args[1] {
            Value::Integer(n) if n.is_positive() => n.to_usize().unwrap_or(100),
            _ => 100,
        }
    } else {
        100
    };
    match &args[0] {
        Value::Integer(n) => {
            // Integer: just [n]
            Ok(Value::List(vec![Value::Integer(n.clone())]))
        }
        Value::Rational(r) => {
            // Exact CF expansion of a rational
            let cf = continued_fraction_rational(r, max_terms);
            Ok(Value::List(cf.into_iter().map(Value::Integer).collect()))
        }
        Value::Real(r) => {
            // CF expansion of a real (floating point)
            let cf = continued_fraction_real(r, max_terms);
            Ok(Value::List(cf.into_iter().map(Value::Integer).collect()))
        }
        _ => Ok(Value::Call {
            head: "ContinuedFraction".to_string(),
            args: args.to_vec(),
        }),
    }
}

/// Exact continued-fraction expansion of a rational, using floor division so
/// the result is canonical (all terms after the first are positive, and the
/// final term is `>= 2` unless the value is an integer). Handles negative
/// rationals the way Mathematica does.
fn continued_fraction_rational(r: &Rational, max_terms: usize) -> Vec<Integer> {
    let mut cf = Vec::new();
    let mut num = r.numer().clone();
    let mut den = r.denom().clone();
    while !den.is_zero() && cf.len() < max_terms {
        let (a, rem) = num.clone().div_rem_floor(den.clone());
        cf.push(a);
        if rem.is_zero() {
            break;
        }
        num = den;
        den = rem;
    }
    cf
}

fn continued_fraction_real(r: &Float, max_terms: usize) -> Vec<Integer> {
    let mut cf = Vec::new();
    let mut x = r.clone();
    if x.is_nan() || x.is_infinite() {
        return cf;
    }
    for _ in 0..max_terms {
        let prec = x.prec();
        let a_int = x.clone().floor().to_integer().unwrap_or_default();
        cf.push(a_int.clone());
        let frac = x.clone() - Float::with_val(prec, &a_int);
        if frac.is_zero() || frac.clone().abs() < Float::with_val(prec, 1e-14_f64) {
            break;
        }
        x = Float::with_val(prec, 1u32) / frac;
        if x.is_infinite() || x.is_nan() {
            break;
        }
    }
    cf
}

// ── FromContinuedFraction ───────────────────────────────────────────────────

/// FromContinuedFraction[list] — reconstruct number from continued fraction list.
pub fn builtin_from_continued_fraction(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FromContinuedFraction requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::List(items) => {
            if items.is_empty() {
                return Err(EvalError::Error(
                    "FromContinuedFraction: list must not be empty".to_string(),
                ));
            }
            // Standard convergent recurrence with h_{-1}=1, h_{-2}=0 and
            // k_{-1}=0, k_{-2}=1; the final h/k reduces automatically via
            // `Rational`'s sign/denominator normalization.
            let mut h_prev = Integer::from(1); // h_{-1}
            let mut h_prev2 = Integer::from(0); // h_{-2}
            let mut k_prev = Integer::from(0); // k_{-1}
            let mut k_prev2 = Integer::from(1); // k_{-2}
            for item in items {
                let a = match item {
                    Value::Integer(a) => a.clone(),
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: "Integer".to_string(),
                            got: item.type_name().to_string(),
                        });
                    }
                };
                let h = a.clone() * h_prev.clone() + h_prev2;
                let k = a * k_prev.clone() + k_prev2;
                h_prev2 = h_prev;
                h_prev = h;
                k_prev2 = k_prev;
                k_prev = k;
            }
            if k_prev.is_zero() {
                return Ok(Value::Integer(h_prev));
            }
            Ok(rat_or_int(Rational::from((h_prev, k_prev))))
        }
        _ => Err(EvalError::TypeError {
            expected: "List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── NumberExpand ────────────────────────────────────────────────────────────

/// NumberExpand[n, b] — digit expansion as list of digit * base^position.
/// Example: NumberExpand[1234] = {1000, 200, 30, 4}
pub fn builtin_number_expand(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "NumberExpand requires 1 or 2 arguments".to_string(),
        ));
    }
    let base = if args.len() == 2 {
        match &args[1] {
            Value::Integer(b) if *b >= 2 => b.to_u32().unwrap_or(10),
            _ => {
                return Err(EvalError::Error(
                    "NumberExpand: base must be an integer >= 2".to_string(),
                ));
            }
        }
    } else {
        10
    };
    match &args[0] {
        Value::Integer(n) => {
            if n.is_zero() {
                return Ok(Value::List(vec![Value::Integer(Integer::from(0))]));
            }
            let base_int = Integer::from(base);
            let mut m = n.clone().abs();
            let mut digits: Vec<Integer> = Vec::new();
            while m > 0 {
                digits.push(m.clone() % base_int.clone());
                m /= base_int.clone();
            }
            digits.reverse();
            // Now multiply each digit by its positional value
            let mut power = Integer::from(1);
            let mut expanded: Vec<Integer> = Vec::with_capacity(digits.len());
            for d in digits.iter().rev() {
                expanded.push(d.clone() * power.clone());
                power *= base_int.clone();
            }
            expanded.reverse();
            Ok(Value::List(
                expanded.into_iter().map(Value::Integer).collect(),
            ))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── ExtendedGCD ───────────────────────────────────────────────────────────

/// Build an unevaluated `head[args]` expression (used for bad input).
fn unevaluated_call(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

/// Extended Euclidean algorithm for a single pair. Returns `(g, x, y)` with
/// `a*x + b*y == g` and `g >= 0`.
fn extended_gcd_pair(a: &Integer, b: &Integer) -> (Integer, Integer, Integer) {
    let (mut old_r, mut r) = (a.clone(), b.clone());
    let (mut old_s, mut s) = (Integer::from(1), Integer::from(0));
    let (mut old_t, mut t) = (Integer::from(0), Integer::from(1));
    while !r.is_zero() {
        let q = old_r.clone() / r.clone();
        let nr = old_r.clone() - q.clone() * r.clone();
        let ns = old_s.clone() - q.clone() * s.clone();
        let nt = old_t.clone() - q.clone() * t.clone();
        old_r = std::mem::replace(&mut r, nr);
        old_s = std::mem::replace(&mut s, ns);
        old_t = std::mem::replace(&mut t, nt);
    }
    if old_r.is_negative() {
        (-old_r, -old_s, -old_t)
    } else {
        (old_r, old_s, old_t)
    }
}

/// `ExtendedGCD[n1, n2, ...]` — returns `{g, {r1, r2, ...}}` where
/// `g == GCD[n1, n2, ...]` and `g == r1*n1 + r2*n2 + ...`.
pub fn builtin_extended_gcd(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(unevaluated_call("ExtendedGCD", args));
    }
    let mut ints = Vec::with_capacity(args.len());
    for a in args {
        match a {
            Value::Integer(n) => ints.push(n.clone()),
            _ => return Ok(unevaluated_call("ExtendedGCD", args)),
        }
    }
    if ints.len() == 1 {
        let n = &ints[0];
        let g = n.clone().abs();
        let coeff = if n.is_zero() {
            Integer::from(0)
        } else if n.is_negative() {
            Integer::from(-1)
        } else {
            Integer::from(1)
        };
        return Ok(Value::List(vec![
            Value::Integer(g),
            Value::List(vec![Value::Integer(coeff)]),
        ]));
    }
    let mut g = ints[0].clone();
    let mut coeffs = vec![Integer::from(1)];
    for nxt in &ints[1..] {
        let (g2, x, y) = extended_gcd_pair(&g, nxt);
        for c in coeffs.iter_mut() {
            *c *= &x;
        }
        coeffs.push(y);
        g = g2;
    }
    // All-zero input has GCD 0; report the canonical {0, {0, ...}}.
    if g.is_zero() {
        for c in coeffs.iter_mut() {
            *c = Integer::from(0);
        }
    }
    Ok(Value::List(vec![
        Value::Integer(g),
        Value::List(coeffs.into_iter().map(Value::Integer).collect()),
    ]))
}

// ── MantissaExponent / ExpMantissa ────────────────────────────────────────

/// Exact `(mantissa, exponent)` for a non-zero rational `x` in `base`, with
/// `x == mantissa * base^exponent` and `1/base <= |mantissa| < 1`.
fn mantissa_exponent_exact(x: &Rational, base: &Integer) -> (Rational, Integer) {
    let neg = x.is_negative();
    let b = Rational::from(base.clone());
    let b_recip = Rational::from((Integer::from(1), base.clone()));
    let one = Rational::from(1);
    let mut a = x.clone().abs();
    let mut e = Integer::from(0);
    if a >= one {
        while a >= one {
            a /= &b;
            e += Integer::from(1);
        }
    } else {
        while a < b_recip {
            a *= &b;
            e -= Integer::from(1);
        }
    }
    if neg {
        a = -a;
    }
    (a, e)
}

/// `MantissaExponent[x]` / `MantissaExponent[x, b]` — returns `{m, e}` with
/// `x == m * b^e` and `1/b <= |m| < 1`. Exact for exact input.
pub fn builtin_mantissa_exponent(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated_call("MantissaExponent", args));
    }
    let base = if args.len() == 2 {
        match &args[1] {
            Value::Integer(b) if *b >= 2 => b.clone(),
            _ => return Ok(unevaluated_call("MantissaExponent", args)),
        }
    } else {
        Integer::from(10)
    };
    match &args[0] {
        Value::Integer(n) => {
            if n.is_zero() {
                return Ok(Value::List(vec![
                    Value::Integer(Integer::from(0)),
                    Value::Integer(Integer::from(0)),
                ]));
            }
            let (m, e) = mantissa_exponent_exact(&Rational::from(n.clone()), &base);
            Ok(Value::List(vec![
                Value::Rational(Box::new(m)),
                Value::Integer(e),
            ]))
        }
        Value::Rational(r) => {
            if r.is_zero() {
                return Ok(Value::List(vec![
                    Value::Integer(Integer::from(0)),
                    Value::Integer(Integer::from(0)),
                ]));
            }
            let (m, e) = mantissa_exponent_exact(r, &base);
            Ok(Value::List(vec![
                Value::Rational(Box::new(m)),
                Value::Integer(e),
            ]))
        }
        Value::Real(x) => {
            if x.is_zero() {
                return Ok(Value::List(vec![
                    Value::Real(Float::with_val(x.prec(), 0)),
                    Value::Integer(Integer::from(0)),
                ]));
            }
            let prec = x.prec();
            let ln_abs = x.clone().abs().ln();
            let ln_base = Float::with_val(prec, &base).ln();
            let e = (ln_abs / ln_base).floor().to_f64() as i32 + 1;
            let scale = Float::with_val(prec, &base).pow(e);
            let m = x.clone() / scale;
            Ok(Value::List(vec![
                Value::Real(m),
                Value::Integer(Integer::from(e)),
            ]))
        }
        _ => Ok(unevaluated_call("MantissaExponent", args)),
    }
}

/// Rescale an exact rational by `pow` (positive) or its reciprocal.
fn scale_exact(r: &Rational, pow: &Integer, neg_exp: bool) -> Rational {
    let p = Rational::from(pow.clone());
    if neg_exp {
        (r / &p).into()
    } else {
        (r * &p).into()
    }
}

/// Collapse a denominator-1 rational back to an integer.
fn rat_or_int(r: Rational) -> Value {
    if r.denom() == &Integer::from(1) {
        Value::Integer(r.numer().clone())
    } else {
        Value::Rational(Box::new(r))
    }
}

/// `ExpMantissa[{m, e}]` — inverse of `MantissaExponent`: `m * 10^e`.
/// Also accepts `ExpMantissa[m, e]` and `ExpMantissa[{m, e}, b]`.
pub fn builtin_exp_mantissa(args: &[Value]) -> Result<Value, EvalError> {
    let (m, e, base) = match args {
        [Value::List(items)] if items.len() == 2 => (&items[0], &items[1], Integer::from(10)),
        [Value::List(items), Value::Integer(b)] if items.len() == 2 => {
            (&items[0], &items[1], b.clone())
        }
        [m, e] => (m, e, Integer::from(10)),
        _ => return Ok(unevaluated_call("ExpMantissa", args)),
    };
    if base < 2 {
        return Ok(unevaluated_call("ExpMantissa", args));
    }
    let e_int = match e {
        Value::Integer(n) => n.clone(),
        _ => return Ok(unevaluated_call("ExpMantissa", args)),
    };
    let neg_exp = e_int.is_negative();
    let e_u32 = match e_int.abs().to_u32() {
        Some(v) => v,
        None => return Ok(unevaluated_call("ExpMantissa", args)),
    };
    let pow = base.pow(e_u32);
    match m {
        Value::Integer(n) => Ok(rat_or_int(scale_exact(
            &Rational::from(n.clone()),
            &pow,
            neg_exp,
        ))),
        Value::Rational(r) => Ok(rat_or_int(scale_exact(r, &pow, neg_exp))),
        Value::Real(x) => {
            let prec = x.prec();
            let f = Float::with_val(prec, &pow);
            let result = if neg_exp {
                x.clone() / f
            } else {
                x.clone() * f
            };
            Ok(Value::Real(result))
        }
        _ => Ok(unevaluated_call("ExpMantissa", args)),
    }
}

// ── SquareFreePart ────────────────────────────────────────────────────────

/// `SquareFreePart[n]` — the product of the primes dividing `n` to an odd
/// power, carrying the sign of `n`. Equals `n` divided by its largest square
/// divisor.
pub fn builtin_square_free_part(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated_call("SquareFreePart", args));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n.is_zero() {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let neg = n.is_negative();
            let mut part = Integer::from(1);
            for (p, e) in factor_integer(n) {
                if !e.is_multiple_of(2) {
                    part *= p;
                }
            }
            if neg {
                part = -part;
            }
            Ok(Value::Integer(part))
        }
        _ => Ok(unevaluated_call("SquareFreePart", args)),
    }
}

/// FareySequence[n] — fractions between 0 and 1 with denominator at most `n`,
/// in increasing order.
pub fn builtin_farey_sequence(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FareySequence requires exactly 1 argument".to_string(),
        ));
    }
    let n = match &args[0] {
        Value::Integer(i) if !i.is_negative() => i.to_i64().unwrap_or(1),
        _ => return Ok(unevaluated_call("FareySequence", args)),
    };
    if n == 0 {
        return Ok(Value::List(vec![
            Value::Integer(rug::Integer::from(0)),
            Value::Integer(rug::Integer::from(1)),
        ]));
    }
    let mut out: Vec<Value> = Vec::new();
    let (mut a, mut b, mut c, mut d) = (0i64, 1i64, 1i64, n);
    while c <= n {
        out.push(Value::Rational(Box::new(rug::Rational::from((a, b)))));
        let k = (n + b) / d;
        let (na, nb) = (c, d);
        let (nc, nd) = (k * c - a, k * d - b);
        a = na;
        b = nb;
        c = nc;
        d = nd;
    }
    out.push(Value::Integer(rug::Integer::from(1)));
    Ok(Value::List(out))
}

// ── Radical ─────────────────────────────────────────────────────────────

/// `Radical[n]` — the product of the distinct prime factors of `n`, carrying
/// the sign of `n` (`Radical[0] == 0`). Equals the squarefree kernel with its
/// exponents dropped to one.
pub fn builtin_radical(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Radical requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            if n.is_zero() {
                return Ok(Value::Integer(Integer::from(0)));
            }
            let neg = n.is_negative();
            let mut rad = Integer::from(1);
            for (p, _) in factor_integer(&n.clone().abs()) {
                rad *= p;
            }
            if neg {
                rad = -rad;
            }
            Ok(Value::Integer(rad))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

// ── PowerModList ────────────────────────────────────────────────────────

/// Modular square roots of `a` modulo an odd prime `p`, via Tonelli–Shanks.
fn sqrt_mod_prime(a: &Integer, p: &Integer) -> Vec<Integer> {
    let a = ((a.clone() % p.clone()) + p.clone()) % p.clone();
    if a.is_zero() {
        return vec![Integer::from(0)];
    }
    if jacobi_symbol(&a, p) != 1 {
        return Vec::new();
    }
    if (p.clone() % 4u32) == 3u32 {
        let e = (p.clone() + 1u32) / 4u32;
        let s = compute_power_mod_int(&a, &e, p);
        return sorted_distinct_pair(s, p.clone());
    }
    // Write p - 1 = q * 2^ss with q odd.
    let mut q = p.clone() - 1u32;
    let mut ss = 0u32;
    while (q.clone() % 2u32).is_zero() {
        q /= 2u32;
        ss += 1;
    }
    let mut z = Integer::from(2);
    while jacobi_symbol(&z, p) != -1 {
        z += 1;
    }
    let mut m = ss;
    let mut c = compute_power_mod_int(&z, &q, p);
    let mut t = compute_power_mod_int(&a, &q, p);
    let mut r = compute_power_mod_int(&a, &((q.clone() + 1u32) / 2u32), p);
    while t != 1 {
        let mut i = 0u32;
        let mut t2 = t.clone();
        while t2 != 1 {
            t2 = (t2.clone() * t2) % p;
            i += 1;
        }
        let b = compute_power_mod_int(&c, &(Integer::from(1) << (m - i - 1)), p);
        m = i;
        c = (b.clone() * b.clone()) % p;
        t = (t.clone() * c.clone()) % p;
        r = (r.clone() * b) % p;
    }
    sorted_distinct_pair(r, p.clone())
}

fn sorted_distinct_pair(s: Integer, p: Integer) -> Vec<Integer> {
    let mut out = vec![s.clone(), p - s];
    out.sort();
    out.dedup();
    out
}

/// Roots of `x^r ≡ target (mod m)`, when brute force is not already used.
fn modular_roots_fallback(target: &Integer, r: u32, m: &Integer) -> Vec<Integer> {
    if r == 2 && *m > 2 && is_prime_int(m) {
        return sqrt_mod_prime(target, m);
    }
    if let Some(mu) = m.to_usize() {
        let mut out = Vec::new();
        let ri = Integer::from(r);
        for x in 0..mu {
            let xi = Integer::from(x);
            if compute_power_mod_int(&xi, &ri, m) == *target {
                out.push(xi);
            }
        }
        return out;
    }
    vec![target.clone()]
}

/// `PowerModList[a, s/r, m]` — all `x` modulo `m` with `x^r ≡ a^s (mod m)`.
/// Special cases: `s/r = -1` gives the modular inverse, `s/r = 1/2` the
/// modular square roots.
pub fn builtin_power_mod_list(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Ok(unevaluated_call("PowerModList", args));
    }
    let a = match &args[0] {
        Value::Integer(a) => a.clone(),
        _ => return Ok(unevaluated_call("PowerModList", args)),
    };
    let m = match &args[2] {
        Value::Integer(m) if m.is_positive() => m.clone(),
        _ => return Ok(unevaluated_call("PowerModList", args)),
    };
    let (s, r) = match &args[1] {
        Value::Integer(s) => (s.clone(), Integer::from(1)),
        Value::Rational(q) => {
            let (mut s, mut r) = (q.numer().clone(), q.denom().clone());
            if r.is_negative() {
                s = -s;
                r = -r;
            }
            if r.is_zero() {
                return Ok(unevaluated_call("PowerModList", args));
            }
            (s, r)
        }
        _ => return Ok(unevaluated_call("PowerModList", args)),
    };
    let r_u32 = match r.to_u32() {
        Some(v) if v >= 1 => v,
        _ => return Ok(unevaluated_call("PowerModList", args)),
    };
    let a_norm = ((a % &m) + &m) % &m;
    // target = a^s mod m (s may be negative).
    let target = if s.is_negative() {
        let inv = match mod_inverse(&a_norm, &m) {
            Some(x) => x,
            None => return Ok(Value::List(vec![])),
        };
        compute_power_mod_int(&inv, &(-s.clone()), &m)
    } else {
        compute_power_mod_int(&a_norm, &s, &m)
    };
    let mut roots: Vec<Integer> = if r_u32 == 1 {
        vec![target.clone()]
    } else if let Some(mu) = m.to_u64() {
        if mu <= 2_000_000 {
            let ri = Integer::from(r_u32);
            (0..mu)
                .map(Integer::from)
                .filter(|x| compute_power_mod_int(x, &ri, &m) == target)
                .collect()
        } else {
            modular_roots_fallback(&target, r_u32, &m)
        }
    } else {
        modular_roots_fallback(&target, r_u32, &m)
    };
    roots.sort();
    Ok(Value::List(roots.into_iter().map(Value::Integer).collect()))
}

// ── SquaresR / SumOfSquares ─────────────────────────────────────────────

/// `SquaresR[r, n]` — the number of ways to write `n` as an ordered sum of `r`
/// signed squares. Exact formulas are provided for `r = 0, 1, 2, 4`.
pub fn builtin_squares_r(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated_call("SquaresR", args));
    }
    let (r, n) = match (&args[0], &args[1]) {
        (Value::Integer(r), Value::Integer(n)) => (r.clone(), n.clone()),
        _ => return Ok(unevaluated_call("SquaresR", args)),
    };
    if n.is_negative() {
        return Ok(Value::Integer(Integer::from(0)));
    }
    if n.is_zero() {
        return Ok(Value::Integer(Integer::from(1)));
    }
    match r.to_u32() {
        Some(0) => Ok(Value::Integer(Integer::from(0))),
        Some(1) => {
            let s = n.clone().sqrt();
            let count = if s.clone() * s.clone() == n { 2 } else { 0 };
            Ok(Value::Integer(Integer::from(count)))
        }
        Some(2) => {
            // r2(n) = 4 (d1(n) - d3(n)) for the divisor counts by residue mod 4.
            let (mut d1, mut d3) = (0i64, 0i64);
            for d in positive_divisors(&n) {
                match (d.clone() % 4u32).to_u32() {
                    Some(1) => d1 += 1,
                    Some(3) => d3 += 1,
                    _ => {}
                }
            }
            Ok(Value::Integer(Integer::from(4 * (d1 - d3))))
        }
        Some(4) => {
            // r4(n) = 8 * sum of divisors not divisible by 4.
            let mut sum = Integer::from(0);
            for d in positive_divisors(&n) {
                if !(d.clone() % 4u32).is_zero() {
                    sum += d;
                }
            }
            Ok(Value::Integer(Integer::from(8) * sum))
        }
        _ => Ok(unevaluated_call("SquaresR", args)),
    }
}

/// `SumOfSquares[n]` — all unordered representations `{a, b}` with
/// `0 <= b <= a` and `a^2 + b^2 == n` (empty when none).
pub fn builtin_sum_of_squares(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated_call("SumOfSquares", args));
    }
    let n = match &args[0] {
        Value::Integer(n) => n.clone(),
        _ => return Ok(unevaluated_call("SumOfSquares", args)),
    };
    if n.is_negative() {
        return Ok(Value::List(vec![]));
    }
    let mut out = Vec::new();
    let mut b = Integer::from(0);
    while Integer::from(2) * b.clone() * b.clone() <= n {
        let rem = n.clone() - b.clone() * b.clone();
        let a = rem.clone().sqrt();
        if a.clone() * a.clone() == rem {
            out.push(Value::List(vec![
                Value::Integer(a),
                Value::Integer(b.clone()),
            ]));
        }
        b += Integer::from(1);
    }
    Ok(Value::List(out))
}

// ── IntegerPart / FractionalPart (exact) ────────────────────────────────

/// `IntegerPart[x]` with exact rational support (truncation toward zero).
pub fn builtin_integer_part_exact(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated_call("IntegerPart", args));
    }
    match &args[0] {
        Value::Integer(n) => Ok(Value::Integer(n.clone())),
        Value::Rational(r) => Ok(Value::Integer(r.clone().trunc().numer().clone())),
        Value::Real(x) => {
            let truncated = x.clone().trunc();
            Ok(Value::Integer(
                truncated.to_integer().unwrap_or_else(Integer::new),
            ))
        }
        _ => Ok(unevaluated_call("IntegerPart", args)),
    }
}

/// `FractionalPart[x]` with exact rational support (`x - IntegerPart[x]`).
pub fn builtin_fractional_part_exact(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated_call("FractionalPart", args));
    }
    match &args[0] {
        Value::Integer(_) => Ok(Value::Integer(Integer::from(0))),
        Value::Rational(r) => {
            let value: &Rational = r;
            let frac = value.clone() - value.clone().trunc();
            Ok(rat_or_int(frac))
        }
        Value::Real(x) => Ok(Value::Real(x.clone().fract())),
        _ => Ok(unevaluated_call("FractionalPart", args)),
    }
}

/// Register the additional number-theory builtins defined in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "ExtendedGCD", builtin_extended_gcd);
    super::register_builtin(env, "MantissaExponent", builtin_mantissa_exponent);
    super::register_builtin(env, "ExpMantissa", builtin_exp_mantissa);
    super::register_builtin(env, "SquareFreePart", builtin_square_free_part);
    super::register_builtin(env, "FareySequence", builtin_farey_sequence);
    super::register_builtin(env, "Radical", builtin_radical);
    super::register_builtin(env, "PowerModList", builtin_power_mod_list);
    super::register_builtin(env, "PrimitiveRootList", builtin_primitive_root_list);
    super::register_builtin(env, "SquaresR", builtin_squares_r);
    super::register_builtin(env, "SumOfSquares", builtin_sum_of_squares);
    // Exact rational forms override the float-only math.rs versions.
    super::register_builtin(env, "IntegerPart", builtin_integer_part_exact);
    super::register_builtin(env, "FractionalPart", builtin_fractional_part_exact);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn rat(n: i64, d: i64) -> Value {
        Value::Rational(Box::new(Rational::from((
            Integer::from(n),
            Integer::from(d),
        ))))
    }

    fn call_is(v: &Value, head: &str) -> bool {
        matches!(v, Value::Call { head: h, .. } if h == head)
    }

    #[test]
    fn test_prime_q() {
        assert_eq!(
            builtin_prime_q(&[int(2)]).unwrap(),
            Value::Symbol("True".to_string())
        );
        assert_eq!(
            builtin_prime_q(&[int(17)]).unwrap(),
            Value::Symbol("True".to_string())
        );
        assert_eq!(
            builtin_prime_q(&[int(4)]).unwrap(),
            Value::Symbol("False".to_string())
        );
        assert_eq!(
            builtin_prime_q(&[int(1)]).unwrap(),
            Value::Symbol("False".to_string())
        );
    }

    #[test]
    fn test_prime_q_large_deterministic() {
        // Strong pseudoprime to bases 2,3,5,7 (so the old four-witness set
        // wrongly reported it prime) and the smallest strong pseudoprime to
        // the first nine prime bases.
        assert_eq!(
            builtin_prime_q(&[int(3_215_031_751)]).unwrap(),
            Value::Symbol("False".to_string())
        );
        assert_eq!(
            builtin_prime_q(&[Value::Integer(Integer::from(3_825_123_056_546_413_051u64))])
                .unwrap(),
            Value::Symbol("False".to_string())
        );
        // Largest prime below 2^64, and an ordinary large prime.
        assert_eq!(
            builtin_prime_q(&[Value::Integer(Integer::from(18_446_744_073_709_551_557u64))])
                .unwrap(),
            Value::Symbol("True".to_string())
        );
        assert_eq!(
            builtin_prime_q(&[int(32_416_190_071)]).unwrap(),
            Value::Symbol("True".to_string())
        );
    }

    #[test]
    fn test_factor_integer() {
        let result = builtin_factor_integer(&[int(12)]).unwrap();
        // 12 = 2^2 * 3^1
        if let Value::List(factors) = result {
            assert_eq!(factors.len(), 2);
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_divisors() {
        let result = builtin_divisors(&[int(6)]).unwrap();
        if let Value::List(divs) = result {
            assert_eq!(divs.len(), 4); // 1, 2, 3, 6
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_next_prime() {
        assert_eq!(builtin_next_prime(&[int(10)]).unwrap(), int(11));
        assert_eq!(builtin_next_prime(&[int(2)]).unwrap(), int(3));
    }

    #[test]
    fn test_prime() {
        assert_eq!(builtin_prime(&[int(1)]).unwrap(), int(2));
        assert_eq!(builtin_prime(&[int(4)]).unwrap(), int(7));
    }

    #[test]
    fn test_euler_phi() {
        // φ(6) = 2 (1 and 5 are coprime to 6)
        assert_eq!(builtin_euler_phi(&[int(6)]).unwrap(), int(2));
        // φ(1) = 1
        assert_eq!(builtin_euler_phi(&[int(1)]).unwrap(), int(1));
    }

    #[test]
    fn test_moebius_mu() {
        assert_eq!(builtin_moebius_mu(&[int(1)]).unwrap(), int(1));
        assert_eq!(builtin_moebius_mu(&[int(4)]).unwrap(), int(0)); // 4 = 2^2
        assert_eq!(builtin_moebius_mu(&[int(6)]).unwrap(), int(1)); // 6 = 2*3, 2 primes → (-1)^2 = 1
    }

    #[test]
    fn test_divisor_sigma() {
        // DivisorSigma[0, 6] = count of divisors = 4
        assert_eq!(builtin_divisor_sigma(&[int(0), int(6)]).unwrap(), int(4));
        // DivisorSigma[1, 6] = 1+2+3+6 = 12
        assert_eq!(builtin_divisor_sigma(&[int(1), int(6)]).unwrap(), int(12));
    }

    #[test]
    fn test_power_mod() {
        // 2^10 mod 100 = 1024 mod 100 = 24
        assert_eq!(
            builtin_power_mod(&[int(2), int(10), int(100)]).unwrap(),
            int(24)
        );
        // 3^(-1) mod 7 = 5 (since 3*5 = 15 ≡ 1 mod 7)
        let inv = builtin_power_mod(&[int(3), int(-1), int(7)]).unwrap();
        assert_eq!(inv, int(5));
    }

    #[test]
    fn test_integer_digits() {
        let result = builtin_integer_digits(&[int(255), int(16)]).unwrap();
        if let Value::List(digits) = result {
            assert_eq!(digits.len(), 2); // 0xFF = [15, 15]
        } else {
            panic!("Expected List");
        }
    }

    #[test]
    fn test_modular_inverse() {
        assert_eq!(builtin_modular_inverse(&[int(3), int(7)]).unwrap(), int(5));
        let result = builtin_modular_inverse(&[int(2), int(4)]).unwrap();
        assert!(matches!(result, Value::Call { .. }));
    }

    #[test]
    fn test_prime_omega() {
        assert_eq!(builtin_prime_omega(&[int(1)]).unwrap(), int(0));
        assert_eq!(builtin_prime_omega(&[int(12)]).unwrap(), int(3));
        assert_eq!(builtin_prime_omega(&[int(8)]).unwrap(), int(3));
    }

    #[test]
    fn test_prime_nu() {
        assert_eq!(builtin_prime_nu(&[int(1)]).unwrap(), int(0));
        assert_eq!(builtin_prime_nu(&[int(12)]).unwrap(), int(2));
        assert_eq!(builtin_prime_nu(&[int(8)]).unwrap(), int(1));
    }

    #[test]
    fn test_digit_count() {
        let result = builtin_digit_count(&[int(123)]).unwrap();
        if let Value::List(counts) = &result {
            assert_eq!(counts.len(), 10);
            assert_eq!(counts[1], int(1));
            assert_eq!(counts[2], int(1));
            assert_eq!(counts[3], int(1));
        } else {
            panic!("Expected List");
        }
        assert_eq!(
            builtin_digit_count(&[int(255), int(16), int(15)]).unwrap(),
            int(2)
        );
    }

    #[test]
    fn test_jacobi_symbol() {
        assert_eq!(builtin_jacobi_symbol(&[int(1), int(7)]).unwrap(), int(1));
        assert_eq!(builtin_jacobi_symbol(&[int(2), int(7)]).unwrap(), int(1));
        assert_eq!(builtin_jacobi_symbol(&[int(2), int(5)]).unwrap(), int(-1));
    }

    #[test]
    fn test_chinese_remainder() {
        let result = builtin_chinese_remainder(&[
            Value::List(vec![int(2), int(3), int(2)]),
            Value::List(vec![int(3), int(5), int(7)]),
        ])
        .unwrap();
        assert_eq!(result, int(23));
        let incompat = builtin_chinese_remainder(&[
            Value::List(vec![int(0), int(0)]),
            Value::List(vec![int(4), int(6)]),
        ])
        .unwrap();
        assert!(matches!(incompat, Value::Call { .. }));
    }

    #[test]
    fn test_multiplicative_order() {
        assert_eq!(
            builtin_multiplicative_order(&[int(3), int(7)]).unwrap(),
            int(6)
        );
        assert_eq!(
            builtin_multiplicative_order(&[int(2), int(7)]).unwrap(),
            int(3)
        );
        assert_eq!(
            builtin_multiplicative_order(&[int(4), int(5)]).unwrap(),
            int(2)
        );
    }

    #[test]
    fn test_primitive_root() {
        assert_eq!(builtin_primitive_root(&[int(7)]).unwrap(), int(3));
        assert_eq!(builtin_primitive_root(&[int(11)]).unwrap(), int(2));
        assert!(builtin_primitive_root(&[int(8)]).is_err());
    }

    #[test]
    fn test_perfect_number_q() {
        assert_eq!(
            builtin_perfect_number_q(&[int(6)]).unwrap(),
            Value::Symbol("True".to_string())
        );
        assert_eq!(
            builtin_perfect_number_q(&[int(12)]).unwrap(),
            Value::Symbol("False".to_string())
        );
    }

    #[test]
    fn test_mangoldt_lambda() {
        assert_eq!(builtin_mangoldt_lambda(&[int(1)]).unwrap(), int(0));
        assert_eq!(builtin_mangoldt_lambda(&[int(6)]).unwrap(), int(0));
        let result = builtin_mangoldt_lambda(&[int(9)]).unwrap();
        let expected = (3.0f64).ln();
        match result {
            Value::Real(r) => {
                let diff = (r.to_f64() - expected).abs();
                assert!(diff < 1e-10);
            }
            _ => panic!("Expected Real for Λ(9)"),
        }
    }

    #[test]
    fn test_liouville_lambda() {
        assert_eq!(builtin_liouville_lambda(&[int(1)]).unwrap(), int(1));
        assert_eq!(builtin_liouville_lambda(&[int(2)]).unwrap(), int(-1));
        assert_eq!(builtin_liouville_lambda(&[int(4)]).unwrap(), int(1));
        assert_eq!(builtin_liouville_lambda(&[int(12)]).unwrap(), int(-1));
    }

    // ── PrimePowerQ ─────────────────────────────────────────────────────

    #[test]
    fn test_prime_power_q() {
        assert_eq!(
            builtin_prime_power_q(&[int(2)]).unwrap(),
            Value::Symbol("True".to_string())
        );
        assert_eq!(
            builtin_prime_power_q(&[int(8)]).unwrap(),
            Value::Symbol("True".to_string())
        ); // 2^3
        assert_eq!(
            builtin_prime_power_q(&[int(9)]).unwrap(),
            Value::Symbol("True".to_string())
        ); // 3^2
        assert_eq!(
            builtin_prime_power_q(&[int(6)]).unwrap(),
            Value::Symbol("False".to_string())
        ); // 2*3
        assert_eq!(
            builtin_prime_power_q(&[int(1)]).unwrap(),
            Value::Symbol("False".to_string())
        );
        assert_eq!(
            builtin_prime_power_q(&[int(12)]).unwrap(),
            Value::Symbol("False".to_string())
        );
    }

    // ── SquareFreeQ ─────────────────────────────────────────────────────

    #[test]
    fn test_square_free_q() {
        assert_eq!(
            builtin_square_free_q(&[int(1)]).unwrap(),
            Value::Symbol("True".to_string())
        );
        assert_eq!(
            builtin_square_free_q(&[int(6)]).unwrap(),
            Value::Symbol("True".to_string())
        ); // 2*3
        assert_eq!(
            builtin_square_free_q(&[int(4)]).unwrap(),
            Value::Symbol("False".to_string())
        ); // 2^2
        assert_eq!(
            builtin_square_free_q(&[int(12)]).unwrap(),
            Value::Symbol("False".to_string())
        ); // 2^2 * 3
        assert_eq!(
            builtin_square_free_q(&[int(30)]).unwrap(),
            Value::Symbol("True".to_string())
        ); // 2*3*5
    }

    // ── CompositeQ ──────────────────────────────────────────────────────

    #[test]
    fn test_composite_q() {
        assert_eq!(
            builtin_composite_q(&[int(1)]).unwrap(),
            Value::Symbol("False".to_string())
        );
        assert_eq!(
            builtin_composite_q(&[int(2)]).unwrap(),
            Value::Symbol("False".to_string())
        );
        assert_eq!(
            builtin_composite_q(&[int(4)]).unwrap(),
            Value::Symbol("True".to_string())
        );
        assert_eq!(
            builtin_composite_q(&[int(6)]).unwrap(),
            Value::Symbol("True".to_string())
        );
        assert_eq!(
            builtin_composite_q(&[int(7)]).unwrap(),
            Value::Symbol("False".to_string())
        );
    }

    // ── PerfectPowerQ ───────────────────────────────────────────────────

    #[test]
    fn test_perfect_power_q() {
        assert_eq!(
            builtin_perfect_power_q(&[int(1)]).unwrap(),
            Value::Symbol("True".to_string())
        );
        assert_eq!(
            builtin_perfect_power_q(&[int(4)]).unwrap(),
            Value::Symbol("True".to_string())
        ); // 2^2
        assert_eq!(
            builtin_perfect_power_q(&[int(8)]).unwrap(),
            Value::Symbol("True".to_string())
        ); // 2^3
        assert_eq!(
            builtin_perfect_power_q(&[int(36)]).unwrap(),
            Value::Symbol("True".to_string())
        ); // 6^2
        assert_eq!(
            builtin_perfect_power_q(&[int(6)]).unwrap(),
            Value::Symbol("False".to_string())
        );
        assert_eq!(
            builtin_perfect_power_q(&[int(12)]).unwrap(),
            Value::Symbol("False".to_string())
        );
    }

    // ── IntegerExponent ─────────────────────────────────────────────────

    #[test]
    fn test_integer_exponent() {
        assert_eq!(builtin_integer_exponent(&[int(100)]).unwrap(), int(2)); // 10^2 divides 100
        assert_eq!(builtin_integer_exponent(&[int(1000)]).unwrap(), int(3));
        assert_eq!(builtin_integer_exponent(&[int(123)]).unwrap(), int(0));
        // With base 2
        assert_eq!(
            builtin_integer_exponent(&[int(16), int(2)]).unwrap(),
            int(4)
        ); // 2^4 = 16
        assert_eq!(
            builtin_integer_exponent(&[int(12), int(2)]).unwrap(),
            int(2)
        ); // 2^2 divides 12
        assert_eq!(
            builtin_integer_exponent(&[int(0)]).unwrap(),
            Value::Symbol("Infinity".to_string())
        );
    }

    // ── FromDigits ──────────────────────────────────────────────────────

    #[test]
    fn test_from_digits() {
        assert_eq!(
            builtin_from_digits(&[Value::List(vec![int(1), int(2), int(3), int(4)])]).unwrap(),
            int(1234)
        );
        // Binary
        assert_eq!(
            builtin_from_digits(&[Value::List(vec![int(1), int(0), int(1)]), int(2)]).unwrap(),
            int(5)
        );
    }

    // ── ToDigits ────────────────────────────────────────────────────────

    #[test]
    fn test_to_digits() {
        let result = builtin_to_digits(&[int(1234)]).unwrap();
        assert_eq!(result, Value::List(vec![int(1), int(2), int(3), int(4)]));
        // Binary
        let result = builtin_to_digits(&[int(5), int(2)]).unwrap();
        assert_eq!(result, Value::List(vec![int(1), int(0), int(1)]));
        // Zero
        assert_eq!(
            builtin_to_digits(&[int(0)]).unwrap(),
            Value::List(vec![int(0)])
        );
    }

    // ── ContinuedFraction ───────────────────────────────────────────────

    #[test]
    fn test_continued_fraction() {
        // CF of 3/2 = [1, 2]
        let r = Rational::from((3, 2));
        let result = builtin_continued_fraction(&[Value::Rational(Box::new(r))]).unwrap();
        assert_eq!(result, Value::List(vec![int(1), int(2)]));
        // CF of 7/5 = [1, 2, 2]
        let r = Rational::from((7, 5));
        let result = builtin_continued_fraction(&[Value::Rational(Box::new(r))]).unwrap();
        assert_eq!(result, Value::List(vec![int(1), int(2), int(2)]));
        // Integer
        assert_eq!(
            builtin_continued_fraction(&[int(5)]).unwrap(),
            Value::List(vec![int(5)])
        );
    }

    // ── FromContinuedFraction ───────────────────────────────────────────

    #[test]
    fn test_from_continued_fraction() {
        // [1, 2] = 3/2
        let result = builtin_from_continued_fraction(&[Value::List(vec![int(1), int(2)])]).unwrap();
        match &result {
            Value::Rational(r) => assert_eq!(r.to_f64(), 1.5),
            _ => panic!("Expected Rational"),
        }
        // [3] = 3
        assert_eq!(
            builtin_from_continued_fraction(&[Value::List(vec![int(3)])]).unwrap(),
            int(3)
        );
    }

    // ── NumberExpand ────────────────────────────────────────────────────

    #[test]
    fn test_number_expand() {
        let result = builtin_number_expand(&[int(1234)]).unwrap();
        assert_eq!(
            result,
            Value::List(vec![int(1000), int(200), int(30), int(4)])
        );
        // Binary: 11 = 1011 in binary = 8+0+2+1
        let result = builtin_number_expand(&[int(11), int(2)]).unwrap();
        assert_eq!(result, Value::List(vec![int(8), int(0), int(2), int(1)]));
        // Zero
        assert_eq!(
            builtin_number_expand(&[int(0)]).unwrap(),
            Value::List(vec![int(0)])
        );
    }

    // ── Pochhammer ──────────────────────────────────────────────────────

    #[test]
    fn test_pochhammer() {
        // Pochhammer[5, 3] = 5*6*7 = 210
        assert_eq!(builtin_pochhammer(&[int(5), int(3)]).unwrap(), int(210));
        // Pochhammer[x, 0] = 1
        assert_eq!(builtin_pochhammer(&[int(100), int(0)]).unwrap(), int(1));
        // Pochhammer[1, n] = n!
        // Pochhammer[1, 5] = 1*2*3*4*5 = 120
        assert_eq!(builtin_pochhammer(&[int(1), int(5)]).unwrap(), int(120));
        // Pochhammer[3, 1] = 3
        assert_eq!(builtin_pochhammer(&[int(3), int(1)]).unwrap(), int(3));
    }

    #[test]
    fn test_pochhammer_symbolic() {
        // Symbolic arguments should return symbolic form
        let result = builtin_pochhammer(&[Value::Symbol("x".to_string()), int(3)]).unwrap();
        assert!(matches!(result, Value::Call { head, .. } if head == "Pochhammer"));
    }

    // ── FallingFactorial ────────────────────────────────────────────────

    #[test]
    fn test_falling_factorial() {
        // FallingFactorial[10, 3] = 10*9*8 = 720
        assert_eq!(
            builtin_falling_factorial(&[int(10), int(3)]).unwrap(),
            int(720)
        );
        // FallingFactorial[n, 0] = 1
        assert_eq!(
            builtin_falling_factorial(&[int(100), int(0)]).unwrap(),
            int(1)
        );
        // FallingFactorial[5, 1] = 5
        assert_eq!(
            builtin_falling_factorial(&[int(5), int(1)]).unwrap(),
            int(5)
        );
        // FallingFactorial[5, 5] = 5*4*3*2*1 = 120
        assert_eq!(
            builtin_falling_factorial(&[int(5), int(5)]).unwrap(),
            int(120)
        );
    }

    #[test]
    fn test_falling_factorial_symbolic() {
        // Symbolic arguments should return symbolic form
        let result = builtin_falling_factorial(&[Value::Symbol("x".to_string()), int(3)]).unwrap();
        assert!(matches!(result, Value::Call { head, .. } if head == "FallingFactorial"));
    }

    // ── Residue ─────────────────────────────────────────────────────────

    #[test]
    fn test_residue() {
        // Residue returns symbolic form for now
        let result = builtin_residue(&[
            Value::Symbol("expr".to_string()),
            Value::List(vec![Value::Symbol("x".to_string()), int(0)]),
        ])
        .unwrap();
        assert!(matches!(result, Value::Call { head, .. } if head == "Residue"));
    }

    // ── Hardening: negative / edge inputs ────────────────────────────────

    #[test]
    fn test_prime_q_negative() {
        // Mathematica: PrimeQ[-n] == PrimeQ[n]
        assert_eq!(builtin_prime_q(&[int(-7)]).unwrap(), bool_value(true));
        assert_eq!(builtin_prime_q(&[int(-8)]).unwrap(), bool_value(false));
        assert_eq!(builtin_prime_q(&[int(-2)]).unwrap(), bool_value(true));
        assert_eq!(builtin_prime_q(&[int(0)]).unwrap(), bool_value(false));
    }

    #[test]
    fn test_prime_q_huge() {
        // A 2^127-1 Mersenne prime, beyond u64.
        let mersenne = Integer::from(1) << 127;
        let mersenne = mersenne - 1;
        assert_eq!(
            builtin_prime_q(&[Value::Integer(mersenne)]).unwrap(),
            bool_value(true)
        );
        // 2^127 is composite.
        assert_eq!(
            builtin_prime_q(&[Value::Integer(Integer::from(1) << 127)]).unwrap(),
            bool_value(false)
        );
    }

    #[test]
    fn test_power_mod_negative_base() {
        // (-3)^3 mod 7 = 1, and the modular inverse of -3 is 2 (mod 7).
        assert_eq!(
            builtin_power_mod(&[int(-3), int(3), int(7)]).unwrap(),
            int(1)
        );
        assert_eq!(
            builtin_power_mod(&[int(-3), int(-1), int(7)]).unwrap(),
            int(2)
        );
        // Result is always in [0, |n|). (-2)^3 = -8, and -8 mod 5 = 2.
        assert_eq!(
            builtin_power_mod(&[int(-2), int(3), int(5)]).unwrap(),
            int(2)
        );
    }

    #[test]
    fn test_power_mod_huge_exponent() {
        // Exponent beyond u64 must not collapse to 2^0. 2^(10^21) mod 1000 = 376.
        let e = Integer::from(10).pow(21);
        assert_eq!(
            builtin_power_mod(&[int(2), Value::Integer(e), int(1000)]).unwrap(),
            int(376)
        );
    }

    #[test]
    fn test_euler_phi_zero_and_negative() {
        assert_eq!(builtin_euler_phi(&[int(0)]).unwrap(), int(0));
        assert_eq!(builtin_euler_phi(&[int(-6)]).unwrap(), int(2));
        assert_eq!(builtin_euler_phi(&[int(-7)]).unwrap(), int(6));
    }

    #[test]
    fn test_moebius_mu_zero_and_negative() {
        assert_eq!(builtin_moebius_mu(&[int(0)]).unwrap(), int(0));
        assert_eq!(builtin_moebius_mu(&[int(-6)]).unwrap(), int(1));
        assert_eq!(builtin_moebius_mu(&[int(-4)]).unwrap(), int(0));
    }

    #[test]
    fn test_divisor_sigma_negative_k() {
        // sigma_{-1}(6) = 1 + 1/2 + 1/3 + 1/6 = 2.
        let got = builtin_divisor_sigma(&[int(-1), int(6)]).unwrap();
        assert_eq!(got, Value::Rational(Box::new(Rational::from(2))));
    }

    #[test]
    fn test_integer_digits_base_one() {
        assert_eq!(
            builtin_integer_digits(&[int(5), int(1)]).unwrap(),
            Value::List(vec![int(1), int(1), int(1), int(1), int(1)])
        );
    }

    #[test]
    fn test_integer_digits_padding() {
        // Pad to length 6.
        assert_eq!(
            builtin_integer_digits(&[int(1234), int(10), int(6)]).unwrap(),
            Value::List(vec![int(0), int(0), int(1), int(2), int(3), int(4)])
        );
        // Truncate to the 2 least significant digits.
        assert_eq!(
            builtin_integer_digits(&[int(1234), int(10), int(2)]).unwrap(),
            Value::List(vec![int(3), int(4)])
        );
    }

    #[test]
    fn test_integer_digits_large_base_and_negative() {
        // Digits may exceed the base's usual range (Mathematica semantics).
        assert_eq!(
            builtin_integer_digits(&[int(58127), int(123)]).unwrap(),
            Value::List(vec![int(3), int(103), int(71)])
        );
        assert_eq!(
            builtin_integer_digits(&[int(-1234)]).unwrap(),
            Value::List(vec![int(1), int(2), int(3), int(4)])
        );
    }

    #[test]
    fn test_integer_length() {
        assert_eq!(builtin_integer_length(&[int(1234)]).unwrap(), int(4));
        assert_eq!(
            builtin_integer_length(&[int(255), int(16)]).unwrap(),
            int(2)
        );
        assert_eq!(builtin_integer_length(&[int(0)]).unwrap(), int(0));
        assert_eq!(builtin_integer_length(&[int(-99)]).unwrap(), int(2));
    }

    #[test]
    fn test_from_digits_out_of_range() {
        // Mathematica evaluates the digit list as a positional polynomial.
        assert_eq!(
            builtin_from_digits(&[Value::List(vec![int(10), int(2)])]).unwrap(),
            int(102)
        );
        assert_eq!(
            builtin_from_digits(&[Value::List(vec![int(-1), int(2)])]).unwrap(),
            int(-8)
        );
    }

    #[test]
    fn test_next_prime_negative_and_k() {
        // Primes are searched over all integers: |n| decides primality, so the
        // smallest prime above -5 is -3 (matching NextPrime's index semantics).
        assert_eq!(builtin_next_prime(&[int(-5)]).unwrap(), int(-3));
        assert_eq!(builtin_next_prime(&[int(-3), int(-1)]).unwrap(), int(-5));
        assert_eq!(builtin_next_prime(&[int(10), int(3)]).unwrap(), int(17));
        assert_eq!(builtin_next_prime(&[int(10), int(-2)]).unwrap(), int(5));
        // The zeroth next prime is the largest prime <= n.
        assert_eq!(builtin_next_prime(&[int(10), int(0)]).unwrap(), int(7));
        assert_eq!(builtin_next_prime(&[int(7), int(0)]).unwrap(), int(7));
    }

    // ── ExtendedGCD ──

    #[test]
    fn test_extended_gcd_known_pairs() {
        // 2*(-1) + 3*1 = 1
        assert_eq!(
            builtin_extended_gcd(&[int(2), int(3)]).unwrap(),
            Value::List(vec![int(1), Value::List(vec![int(-1), int(1)])])
        );
        // 60*1 + 18*(-3) = 6
        assert_eq!(
            builtin_extended_gcd(&[int(60), int(18)]).unwrap(),
            Value::List(vec![int(6), Value::List(vec![int(1), int(-3)])])
        );
        // 6*(-2) + 15*1 + 30*0 = 3
        assert_eq!(
            builtin_extended_gcd(&[int(6), int(15), int(30)]).unwrap(),
            Value::List(vec![int(3), Value::List(vec![int(-2), int(1), int(0)])])
        );
    }

    #[test]
    fn test_extended_gcd_bezout_identity() {
        for (a, b) in [(6i64, 15i64), (12, 8), (7, 13), (-12, 8), (0, 5), (-9, -6)] {
            let out = builtin_extended_gcd(&[int(a), int(b)]).unwrap();
            let Value::List(items) = out else {
                panic!("expected a list")
            };
            let Value::Integer(g) = &items[0] else {
                panic!("expected integer gcd")
            };
            let Value::List(coeffs) = &items[1] else {
                panic!("expected coefficient list")
            };
            let (Value::Integer(x), Value::Integer(y)) = (&coeffs[0], &coeffs[1]) else {
                panic!("expected integer coefficients")
            };
            assert_eq!(g, &(Integer::from(a) * x + Integer::from(b) * y));
            assert!(!g.is_negative());
        }
    }

    #[test]
    fn test_extended_gcd_single_and_zero() {
        assert_eq!(
            builtin_extended_gcd(&[int(-6)]).unwrap(),
            Value::List(vec![int(6), Value::List(vec![int(-1)])])
        );
        assert_eq!(
            builtin_extended_gcd(&[int(0), int(0)]).unwrap(),
            Value::List(vec![int(0), Value::List(vec![int(0), int(0)])])
        );
    }

    #[test]
    fn test_extended_gcd_bad_input() {
        assert!(call_is(
            &builtin_extended_gcd(&[Value::Symbol("x".to_string()), int(3)]).unwrap(),
            "ExtendedGCD"
        ));
        assert!(call_is(&builtin_extended_gcd(&[]).unwrap(), "ExtendedGCD"));
    }

    // ── MantissaExponent ──

    #[test]
    fn test_mantissa_exponent_exact() {
        assert_eq!(
            builtin_mantissa_exponent(&[int(123451)]).unwrap(),
            Value::List(vec![rat(123451, 1000000), int(6)])
        );
        assert_eq!(
            builtin_mantissa_exponent(&[int(100)]).unwrap(),
            Value::List(vec![rat(1, 10), int(3)])
        );
        assert_eq!(
            builtin_mantissa_exponent(&[rat(1, 2)]).unwrap(),
            Value::List(vec![rat(1, 2), int(0)])
        );
        assert_eq!(
            builtin_mantissa_exponent(&[int(-123451)]).unwrap(),
            Value::List(vec![rat(-123451, 1000000), int(6)])
        );
        assert_eq!(
            builtin_mantissa_exponent(&[int(0)]).unwrap(),
            Value::List(vec![int(0), int(0)])
        );
    }

    #[test]
    fn test_mantissa_exponent_base() {
        // 1027 / 2048 lies in [1/2, 1), so the base-2 exponent is 11.
        assert_eq!(
            builtin_mantissa_exponent(&[int(1027), int(2)]).unwrap(),
            Value::List(vec![rat(1027, 2048), int(11)])
        );
    }

    #[test]
    fn test_mantissa_exponent_real() {
        let out =
            builtin_mantissa_exponent(&[Value::Real(Float::with_val(DEFAULT_PRECISION, 123.456))])
                .unwrap();
        let Value::List(items) = out else {
            panic!("expected list")
        };
        assert_eq!(items[1], int(3));
        let Value::Real(m) = &items[0] else {
            panic!("expected real mantissa")
        };
        assert!((m.to_f64() - 0.123456).abs() < 1e-9);
    }

    #[test]
    fn test_mantissa_exponent_bad_input() {
        assert!(call_is(
            &builtin_mantissa_exponent(&[Value::Symbol("x".to_string())]).unwrap(),
            "MantissaExponent"
        ));
        assert!(call_is(
            &builtin_mantissa_exponent(&[int(5), int(1)]).unwrap(),
            "MantissaExponent"
        ));
        assert!(call_is(
            &builtin_mantissa_exponent(&[]).unwrap(),
            "MantissaExponent"
        ));
    }

    // ── ExpMantissa ──

    #[test]
    fn test_exp_mantissa_exact() {
        assert_eq!(
            builtin_exp_mantissa(&[Value::List(vec![rat(1, 10), int(3)])]).unwrap(),
            int(100)
        );
        assert_eq!(
            builtin_exp_mantissa(&[Value::List(vec![int(5), int(-2)])]).unwrap(),
            rat(1, 20)
        );
        // Two-argument form.
        assert_eq!(builtin_exp_mantissa(&[int(5), int(2)]).unwrap(), int(500));
        // Base-2 form inverts the base-2 MantissaExponent.
        assert_eq!(
            builtin_exp_mantissa(&[Value::List(vec![rat(1027, 2048), int(11)]), int(2)]).unwrap(),
            int(1027)
        );
    }

    #[test]
    fn test_exp_mantissa_roundtrip() {
        let n = int(987654321);
        let decomposed = builtin_mantissa_exponent(&[n.clone()]).unwrap();
        assert_eq!(builtin_exp_mantissa(&[decomposed]).unwrap(), n);
    }

    #[test]
    fn test_exp_mantissa_bad_input() {
        assert!(call_is(
            &builtin_exp_mantissa(&[Value::Symbol("x".to_string())]).unwrap(),
            "ExpMantissa"
        ));
        assert!(call_is(
            &builtin_exp_mantissa(&[Value::List(vec![int(1), rat(1, 2)])]).unwrap(),
            "ExpMantissa"
        ));
    }

    // ── SquareFreePart ──

    #[test]
    fn test_square_free_part() {
        assert_eq!(builtin_square_free_part(&[int(12)]).unwrap(), int(3));
        assert_eq!(builtin_square_free_part(&[int(24)]).unwrap(), int(6));
        assert_eq!(builtin_square_free_part(&[int(9)]).unwrap(), int(1));
        assert_eq!(builtin_square_free_part(&[int(1)]).unwrap(), int(1));
        assert_eq!(builtin_square_free_part(&[int(0)]).unwrap(), int(0));
        assert_eq!(builtin_square_free_part(&[int(-12)]).unwrap(), int(-3));
        assert_eq!(builtin_square_free_part(&[int(150)]).unwrap(), int(6));
    }

    #[test]
    fn test_square_free_part_bad_input() {
        assert!(call_is(
            &builtin_square_free_part(&[Value::Symbol("x".to_string())]).unwrap(),
            "SquareFreePart"
        ));
        assert!(call_is(
            &builtin_square_free_part(&[int(1), int(2)]).unwrap(),
            "SquareFreePart"
        ));
    }

    // ── End-to-end registration ──

    #[test]
    fn test_new_builtins_registered_in_env() {
        let env = crate::env::Env::new();
        crate::builtins::register_builtins(&env);
        let eval = |input: &str| -> Value {
            let tokens = crate::lexer::tokenize(input).unwrap();
            let ast = crate::parser::parse(tokens).unwrap();
            crate::eval::eval_program(&ast, &env).unwrap()
        };
        assert_eq!(eval("ExtendedGCD[60, 18]"), eval("{6, {1, -3}}"));
        assert_eq!(eval("SquareFreePart[24]"), int(6));
        assert_eq!(
            eval("ExpMantissa[MantissaExponent[987654321]]"),
            int(987654321)
        );
    }

    // ── Fast factorization (Pollard rho + Miller–Rabin) ──────────────────

    fn factor_pairs(n: i64) -> Vec<(i64, u32)> {
        factor_integer(&Integer::from(n))
            .into_iter()
            .map(|(p, e)| (p.to_i64().unwrap(), e))
            .collect()
    }

    #[test]
    fn test_factor_small_and_prime_power() {
        assert!(factor_pairs(1).is_empty());
        assert_eq!(factor_pairs(-360), vec![(2i64, 3u32), (3, 2), (5, 1)]);
        let p = Integer::from(1_000_003);
        let n = p.clone().pow(4u32);
        assert_eq!(factor_integer(&n), vec![(p, 4)]);
    }

    #[test]
    fn test_factor_large_semiprime() {
        // Trial division alone would need ~10^9 steps for this.
        let a = Integer::from(1_000_000_007);
        let b = Integer::from(1_000_000_009);
        let n = a.clone() * b.clone();
        assert_eq!(factor_integer(&n), vec![(a, 1), (b, 1)]);
    }

    #[test]
    fn test_factor_huge_semiprime_bigint() {
        // ~94-bit semiprime with two large prime factors.
        let a = Integer::from(18_446_744_073_709_551_557u64); // largest prime < 2^64
        let b = Integer::from(1_000_000_007u64);
        let n = a.clone() * b.clone();
        assert_eq!(factor_integer(&n), vec![(b, 1), (a, 1)]);
    }

    #[test]
    fn test_divisors_via_factorization_large() {
        // 2 * 3 * 1_000_003 has 8 divisors.
        let n = Integer::from(6) * Integer::from(1_000_003);
        assert_eq!(positive_divisors(&n).len(), 8);
    }

    #[test]
    fn test_perfect_power_repr() {
        assert_eq!(perfect_power_repr(&Integer::from(36)), {
            Some((Integer::from(6), 2))
        });
        assert_eq!(perfect_power_repr(&Integer::from(64)), {
            Some((Integer::from(2), 6))
        });
        assert_eq!(perfect_power_repr(&Integer::from(30)), None);
        assert_eq!(perfect_power_repr(&Integer::from(1)), None);
    }

    // ── Radical ──

    #[test]
    fn test_radical() {
        assert_eq!(builtin_radical(&[int(12)]).unwrap(), int(6));
        assert_eq!(builtin_radical(&[int(72)]).unwrap(), int(6));
        assert_eq!(builtin_radical(&[int(1)]).unwrap(), int(1));
        assert_eq!(builtin_radical(&[int(0)]).unwrap(), int(0));
        assert_eq!(builtin_radical(&[int(-12)]).unwrap(), int(-6));
        assert_eq!(builtin_radical(&[int(30)]).unwrap(), int(30));
    }

    // ── PowerModList ──

    #[test]
    fn test_power_mod_list() {
        assert_eq!(
            builtin_power_mod_list(&[int(3), rat(1, 2), int(11)]).unwrap(),
            Value::List(vec![int(5), int(6)])
        );
        // Modular inverse via -1.
        assert_eq!(
            builtin_power_mod_list(&[int(3), int(-1), int(7)]).unwrap(),
            Value::List(vec![int(5)])
        );
        // Integer exponent r = 1 gives the single power.
        assert_eq!(
            builtin_power_mod_list(&[int(2), int(3), int(7)]).unwrap(),
            Value::List(vec![int(1)])
        );
        // 2 is a quadratic residue mod 7 (3^2 = 2): roots {3, 4}.
        assert_eq!(
            builtin_power_mod_list(&[int(2), rat(1, 2), int(7)]).unwrap(),
            Value::List(vec![int(3), int(4)])
        );
        // 5 is a non-residue mod 7: no square roots.
        assert_eq!(
            builtin_power_mod_list(&[int(5), rat(1, 2), int(7)]).unwrap(),
            Value::List(vec![])
        );
    }

    // ── PrimitiveRootList ──

    #[test]
    fn test_primitive_root_list() {
        assert_eq!(
            builtin_primitive_root_list(&[int(7)]).unwrap(),
            Value::List(vec![int(3), int(5)])
        );
        assert_eq!(
            builtin_primitive_root_list(&[int(9)]).unwrap(),
            Value::List(vec![int(2), int(5)])
        );
        assert_eq!(
            builtin_primitive_root_list(&[int(8)]).unwrap(),
            Value::List(vec![])
        );
        assert_eq!(
            builtin_primitive_root_list(&[int(2)]).unwrap(),
            Value::List(vec![int(1)])
        );
        assert_eq!(
            builtin_primitive_root_list(&[int(1)]).unwrap(),
            Value::List(vec![int(1)])
        );
    }

    // ── SquaresR / SumOfSquares ──

    #[test]
    fn test_squares_r() {
        assert_eq!(builtin_squares_r(&[int(2), int(5)]).unwrap(), int(8));
        assert_eq!(builtin_squares_r(&[int(2), int(1)]).unwrap(), int(4));
        assert_eq!(builtin_squares_r(&[int(2), int(3)]).unwrap(), int(0));
        assert_eq!(builtin_squares_r(&[int(4), int(1)]).unwrap(), int(8));
        assert_eq!(builtin_squares_r(&[int(4), int(2)]).unwrap(), int(24));
        assert_eq!(builtin_squares_r(&[int(1), int(9)]).unwrap(), int(2));
    }

    #[test]
    fn test_sum_of_squares() {
        assert_eq!(
            builtin_sum_of_squares(&[int(25)]).unwrap(),
            Value::List(vec![
                Value::List(vec![int(5), int(0)]),
                Value::List(vec![int(4), int(3)]),
            ])
        );
        assert_eq!(
            builtin_sum_of_squares(&[int(5)]).unwrap(),
            Value::List(vec![Value::List(vec![int(2), int(1)])])
        );
        assert_eq!(
            builtin_sum_of_squares(&[int(3)]).unwrap(),
            Value::List(vec![])
        );
    }

    // ── CoprimeQ breadth ──

    #[test]
    fn test_coprime_q_list_and_flat() {
        assert_eq!(
            builtin_coprime_q(&[Value::List(vec![int(8), int(9), int(25)])]).unwrap(),
            bool_value(true)
        );
        assert_eq!(
            builtin_coprime_q(&[Value::List(vec![int(6), int(10)])]).unwrap(),
            bool_value(false)
        );
        assert_eq!(
            builtin_coprime_q(&[int(8), int(9), int(25)]).unwrap(),
            bool_value(true)
        );
    }

    // ── ContinuedFraction / FromContinuedFraction hardening ──

    #[test]
    fn test_continued_fraction_negative() {
        let r = Rational::from((-7, 5));
        let cf = builtin_continued_fraction(&[Value::Rational(Box::new(r))]).unwrap();
        assert_eq!(cf, Value::List(vec![int(-2), int(1), int(1), int(2)]));
        // Round trip back to the exact rational.
        assert_eq!(
            builtin_from_continued_fraction(&[cf]).unwrap(),
            Value::Rational(Box::new(Rational::from((-7, 5))))
        );
    }

    #[test]
    fn test_from_continued_fraction_zero_first_term() {
        assert_eq!(
            builtin_from_continued_fraction(&[Value::List(vec![int(0), int(2)])]).unwrap(),
            rat(1, 2)
        );
    }

    // ── IntegerPart / FractionalPart exactness ──

    #[test]
    fn test_integer_and_fractional_part_exact() {
        assert_eq!(builtin_integer_part_exact(&[rat(7, 2)]).unwrap(), int(3));
        assert_eq!(builtin_integer_part_exact(&[rat(-7, 2)]).unwrap(), int(-3));
        assert_eq!(builtin_integer_part_exact(&[int(5)]).unwrap(), int(5));
        assert_eq!(
            builtin_fractional_part_exact(&[rat(7, 2)]).unwrap(),
            rat(1, 2)
        );
        assert_eq!(
            builtin_fractional_part_exact(&[rat(-7, 2)]).unwrap(),
            rat(-1, 2)
        );
        assert_eq!(builtin_fractional_part_exact(&[int(5)]).unwrap(), int(0));
    }

    #[test]
    fn test_new_number_theory_builtins_registered() {
        let env = crate::env::Env::new();
        crate::builtins::register_builtins(&env);
        let eval = |input: &str| -> Value {
            let tokens = crate::lexer::tokenize(input).unwrap();
            let ast = crate::parser::parse(tokens).unwrap();
            crate::eval::eval_program(&ast, &env).unwrap()
        };
        assert_eq!(eval("Radical[72]"), int(6));
        assert_eq!(eval("PrimitiveRootList[9]"), eval("{2, 5}"));
        assert_eq!(eval("PowerModList[3, 1/2, 11]"), eval("{5, 6}"));
        assert_eq!(eval("SumOfSquares[25]"), eval("{{5, 0}, {4, 3}}"));
        assert_eq!(eval("IntegerPart[7/2]"), int(3));
        assert_eq!(eval("FractionalPart[7/2]"), eval("1/2"));
    }
}
