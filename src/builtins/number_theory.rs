use crate::value::{EvalError, Value};
use rug::ops::Pow;
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
    if n.is_multiple_of(2) {
        return false;
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
    if n.is_multiple_of(2) || n.is_multiple_of(3) || n.is_multiple_of(5) {
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
            let sign = if factors.len().is_multiple_of(2) { 1 } else { -1 };
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
}
