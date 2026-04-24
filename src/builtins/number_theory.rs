use crate::env::Env;
use crate::eval::apply_function;
use crate::value::{EvalError, Value, DEFAULT_PRECISION};
use rug::ops::Pow;
use rug::Float;
use rug::Integer;

// ── Primality ─────────────────────────────────────────────────────────────

/// Deterministic Miller-Rabin witnesses sufficient for n < 3,215,031,751
const WITNESSES: &[u64] = &[2, 3, 5, 7];

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
    if n == a {
        return true;
    }
    if n % 2 == 0 {
        return false;
    }
    let mut d = n - 1;
    let mut r = 0u32;
    while d % 2 == 0 {
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
    if n % 2 == 0 || (n.clone() % Integer::from(3)).is_zero() || (n.clone() % Integer::from(5)).is_zero() {
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
        Value::Integer(n) => {
            if n.is_negative() || n.is_zero() {
                return Ok(Value::Symbol("False".to_string()));
            }
            let n64 = n.to_u64().unwrap_or(u64::MAX);
            // For large integers beyond u64, fall back to trial division
            let result = if n <= &Integer::from(u64::MAX) {
                is_prime_u64(n64)
            } else {
                // Trial division up to sqrt for large integers (slow but correct)
                trial_prime_check(n)
            };
            Ok(Value::Symbol(if result { "True" } else { "False" }.to_string()))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn trial_prime_check(n: &Integer) -> bool {
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

/// Returns a sorted list of {prime, exponent} pairs.
pub fn factor_integer(n: &Integer) -> Vec<(Integer, u32)> {
    let mut n = n.clone();
    if n < 0 {
        n = -n;
    }
    let mut factors: Vec<(Integer, u32)> = Vec::new();
    if n <= 1 {
        return factors;
    }
    let mut d = Integer::from(2);
    while d.clone() * d.clone() <= n {
        if (n.clone() % d.clone()).is_zero() {
            let mut exp = 0u32;
            while (n.clone() % d.clone()).is_zero() {
                n /= d.clone();
                exp += 1;
            }
            factors.push((d.clone(), exp));
        }
        d += Integer::from(1);
    }
    if n > 1 {
        factors.push((n, 1));
    }
    factors
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
                    Value::List(vec![
                        Value::Integer(p),
                        Value::Integer(Integer::from(e)),
                    ])
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

pub fn builtin_divisors(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Divisors requires exactly 1 argument".to_string(),
        ));
    }
    match &args[0] {
        Value::Integer(n) => {
            let n_abs = n.clone().abs();
            if n_abs.is_zero() {
                return Ok(Value::List(vec![]));
            }
            let mut divs: Vec<Integer> = Vec::new();
            let mut d = Integer::from(1);
            while d.clone() * d.clone() <= n_abs {
                if (n_abs.clone() % d.clone()).is_zero() {
                    divs.push(d.clone());
                    let other = n_abs.clone() / d.clone();
                    if other != d {
                        divs.push(other);
                    }
                }
                d += Integer::from(1);
            }
            divs.sort();
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
            let k = k.to_usize().ok_or_else(|| {
                EvalError::Error("Prime: argument too large".to_string())
            })?;
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
            })
        }
    };
    let count = (2..=x).filter(|&n| is_prime_u64(n)).count();
    Ok(Value::Integer(Integer::from(count)))
}

// ── NextPrime ─────────────────────────────────────────────────────────────

pub fn builtin_next_prime(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "NextPrime requires exactly 1 argument".to_string(),
        ));
    }
    let start = match &args[0] {
        Value::Integer(n) => n.to_u64().unwrap_or(0),
        Value::Real(r) => r.to_f64() as u64,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Number".to_string(),
                got: args[0].type_name().to_string(),
            })
        }
    };
    let mut n = start + 1;
    while !is_prime_u64(n) {
        n += 1;
    }
    Ok(Value::Integer(Integer::from(n)))
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
                return Err(EvalError::Error("PowerMod: modulus cannot be zero".to_string()));
            }
            if b.is_negative() {
                // Modular inverse: find x such that a*x ≡ 1 (mod n)
                let pos_b = (-b.clone()).to_u64().unwrap_or(0);
                let a_u = a.clone().abs();
                let n_u = n.clone().abs();
                // Compute a^|b| mod n, then find modular inverse
                let pow_val = compute_power_mod(&a_u, pos_b, &n_u);
                // Extended GCD for modular inverse
                let inv = mod_inverse(&pow_val, &n_u);
                match inv {
                    Some(i) => Ok(Value::Integer(i)),
                    None => Ok(Value::Call {
                        head: "PowerMod".to_string(),
                        args: args.to_vec(),
                    }),
                }
            } else {
                let b_u = b.to_u64().unwrap_or(0);
                let result = compute_power_mod(a, b_u, n);
                Ok(Value::Integer(result))
            }
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

fn compute_power_mod(base: &Integer, mut exp: u64, modulus: &Integer) -> Integer {
    if modulus == &Integer::from(1) {
        return Integer::from(0);
    }
    let mut result = Integer::from(1);
    let mut base = base.clone() % modulus;
    while exp > 0 {
        if exp & 1 == 1 {
            result = result * base.clone() % modulus;
        }
        exp >>= 1;
        base = base.clone() * base.clone() % modulus;
    }
    result
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
            if n <= &Integer::from(0) {
                return Err(EvalError::Error(
                    "EulerPhi: argument must be positive".to_string(),
                ));
            }
            let factors = factor_integer(n);
            let mut result = n.clone();
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
            if n <= &Integer::from(0) {
                return Err(EvalError::Error(
                    "MoebiusMu: argument must be positive".to_string(),
                ));
            }
            let factors = factor_integer(n);
            // If any prime factor has exponent > 1, μ(n) = 0
            for (_, e) in &factors {
                if *e > 1 {
                    return Ok(Value::Integer(Integer::from(0)));
                }
            }
            // Otherwise μ(n) = (-1)^(number of distinct prime factors)
            let sign = if factors.len() % 2 == 0 { 1 } else { -1 };
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
        Value::Integer(k) => k.to_u32().ok_or_else(|| {
            EvalError::Error("DivisorSigma: k must be a non-negative integer".to_string())
        })?,
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[0].type_name().to_string(),
            })
        }
    };
    match &args[1] {
        Value::Integer(n) => {
            let n_abs = n.clone().abs();
            if n_abs.is_zero() {
                return Ok(Value::Integer(Integer::from(0)));
            }
            // Sum d^k for all divisors d of n
            let mut sum = Integer::from(0);
            let mut d = Integer::from(1);
            while d.clone() * d.clone() <= n_abs {
                if (n_abs.clone() % d.clone()).is_zero() {
                    let dk = d.clone().pow(k);
                    sum += dk;
                    let other = n_abs.clone() / d.clone();
                    if other != d {
                        sum += other.pow(k);
                    }
                }
                d += Integer::from(1);
            }
            Ok(Value::Integer(sum))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[1].type_name().to_string(),
        }),
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
            Ok(Value::Symbol(if r.is_zero() { "True" } else { "False" }.to_string()))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    }
}

pub fn builtin_coprime_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Err(EvalError::Error(
            "CoprimeQ requires at least 2 arguments".to_string(),
        ));
    }
    for i in 0..args.len() {
        for j in (i + 1)..args.len() {
            match (&args[i], &args[j]) {
                (Value::Integer(a), Value::Integer(b)) => {
                    let g = gcd_int(a.clone().abs(), b.clone().abs());
                    if g != 1 {
                        return Ok(Value::Symbol("False".to_string()));
                    }
                }
                _ => {
                    return Err(EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: args[i].type_name().to_string(),
                    })
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
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "IntegerDigits requires 1 or 2 arguments".to_string(),
        ));
    }
    let base = if args.len() == 2 {
        match &args[1] {
            Value::Integer(b) => b.to_u32().unwrap_or(10),
            _ => 10,
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
            Ok(Value::List(digits.into_iter().map(Value::Integer).collect()))
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
            "ModularInverse requires exactly 2 arguments: ModularInverse[a, m]"
                .to_string(),
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
            "DigitCount requires 1 to 3 arguments: DigitCount[n, base, digit]"
                .to_string(),
        ));
    }
    let base: u32 = if args.len() >= 2 {
        match &args[1] {
            Value::Integer(b) if *b >= Integer::from(2) => {
                b.to_u32().unwrap_or(10)
            }
            _ => {
                return Err(EvalError::Error(
                    "DigitCount: base must be an integer >= 2".to_string(),
                ))
            }
        }
    } else {
        10
    };
    let specific_digit = if args.len() == 3 {
        match &args[2] {
            Value::Integer(d) if *d >= Integer::from(0) && *d < Integer::from(base) =>
            {
                d.to_u32().unwrap()
            }
            _ => {
                return Err(EvalError::Error(
                    "DigitCount: invalid digit for this base".to_string(),
                ))
            }
        }
    } else {
        u32::MAX // sentinel: return all counts
    };

    match &args[0] {
        Value::Integer(n) => {
            if n.is_zero() {
                if specific_digit != u32::MAX {
                    return Ok(Value::Integer(Integer::from(if specific_digit == 0 { 1 } else { 0 })));
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
            "JacobiSymbol requires exactly 2 arguments: JacobiSymbol[a, n]"
                .to_string(),
        ));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(n)) => {
            if n <= &Integer::from(0) || (n.clone() % Integer::from(2)).is_zero() {
                return Err(EvalError::Error(
                    "JacobiSymbol: second argument must be a positive odd integer"
                        .to_string(),
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
            let n_mod_8 = (&n).clone() % Integer::from(8);
            if n_mod_8 == Integer::from(3) || n_mod_8 == Integer::from(5) {
                t = -t;
            }
        }
        // Quadratic reciprocity
        let a_mod_4 = (&a).clone() % Integer::from(4);
        let n_mod_4 = (&n).clone() % Integer::from(4);
        if a_mod_4 == Integer::from(3) && n_mod_4 == Integer::from(3) {
            t = -t;
        }
        let tmp = a.clone();
        a = n % tmp.clone();
        n = tmp;
    }
    if n == Integer::from(1) {
        t
    } else {
        0
    }
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
                    "ChineseRemainder: both lists must have the same non-zero length"
                        .to_string(),
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
                        })
                    }
                }
            }

            // Check pairwise coprimality of moduli
            for i in 0..n_eqns {
                for j in (i + 1)..n_eqns {
                    let g = gcd_int(mod_int[i].clone(), mod_int[j].clone());
                    if g != Integer::from(1) {
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
                let xi = mod_inverse(&ni, &mod_int[i])
                    .expect("pairwise coprime");
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
            if g != Integer::from(1) {
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
                    if pow_val == Integer::from(1) {
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

// ── PrimitiveRoot ─────────────────────────────────────────────────────

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
            // Compute φ(n)
            let phi = {
                let factors = factor_integer(n);
                let mut phi = n.clone();
                for (p, _) in &factors {
                    phi = phi.clone() / p.clone() * (p.clone() - Integer::from(1));
                }
                phi
            };
            // Distinct prime factors of φ
            let phi_factors = factor_integer(&phi);
            let distinct: Vec<Integer> =
                phi_factors.iter().map(|(p, _)| p.clone()).collect();

            // Search for smallest primitive root
            let mut g = Integer::from(2);
            while g < *n {
                if gcd_int(g.clone(), n.clone()) != Integer::from(1) {
                    g += Integer::from(1);
                    continue;
                }
                let mut is_primitive = true;
                for p in &distinct {
                    let exp = phi.clone() / p.clone();
                    let pow_val = compute_power_mod_int(&g, &exp, n);
                    if pow_val == Integer::from(1) {
                        is_primitive = false;
                        break;
                    }
                }
                if is_primitive {
                    return Ok(Value::Integer(g));
                }
                g += Integer::from(1);
            }
            Err(EvalError::Error(
                "PrimitiveRoot: no primitive root exists for this modulus"
                    .to_string(),
            ))
        }
        _ => Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
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
            Ok(Value::Symbol(if perfect { "True" } else { "False" }.to_string()))
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
            let val = if omega % 2 == 0 { 1 } else { -1 };
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
            "DivisorSum requires exactly 2 arguments: DivisorSum[n, form]"
                .to_string(),
        ));
    }
    let n = match &args[0] {
        Value::Integer(n) => n.clone().abs(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[0].type_name().to_string(),
            })
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
                    })
                }
            }
            let other = n.clone() / d.clone();
            if other != d {
                let val2 =
                    apply_function(form, &[Value::Integer(other.clone())], env)?;
                match &val2 {
                    Value::Integer(i) => sum += i.clone(),
                    _ => {
                        return Err(EvalError::TypeError {
                            expected: "Integer".to_string(),
                            got: val2.type_name().to_string(),
                        })
                    }
                }
            }
        }
        d += Integer::from(1);
    }
    Ok(Value::Integer(sum))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    #[test]
    fn test_prime_q() {
        assert_eq!(builtin_prime_q(&[int(2)]).unwrap(), Value::Symbol("True".to_string()));
        assert_eq!(builtin_prime_q(&[int(17)]).unwrap(), Value::Symbol("True".to_string()));
        assert_eq!(builtin_prime_q(&[int(4)]).unwrap(), Value::Symbol("False".to_string()));
        assert_eq!(builtin_prime_q(&[int(1)]).unwrap(), Value::Symbol("False".to_string()));
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
        assert_eq!(builtin_power_mod(&[int(2), int(10), int(100)]).unwrap(), int(24));
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
}
