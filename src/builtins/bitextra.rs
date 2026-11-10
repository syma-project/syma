//! bit ops and number extras: BitAnd/BitOr/BitXor/BitShift/BitLength, RealDigits, Rationalize, ContinuedFraction, NextPrime, IntegerName.
//!
//! Gap-fill module; see the individual builtin docs for semantics.

use crate::env::Env;
use crate::value::{EvalError, Value};
#[allow(unused_imports)]
use rug::Float;
use rug::Integer;
use rug::Rational;
use rug::integer::IsPrime;
use rug::ops::Pow;
use std::collections::HashMap;

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "BitAnd", builtin_bit_and);
    super::register_builtin(env, "BitOr", builtin_bit_or);
    super::register_builtin(env, "BitXor", builtin_bit_xor);
    super::register_builtin(env, "BitNot", builtin_bit_not);
    super::register_builtin(env, "BitShiftLeft", builtin_bit_shift_left);
    super::register_builtin(env, "BitShiftRight", builtin_bit_shift_right);
    super::register_builtin(env, "BitLength", builtin_bit_length);
    super::register_builtin(env, "BitSetQ", builtin_bit_set_q);
    super::register_builtin(env, "BitGet", builtin_bit_get);
    super::register_builtin(env, "BitClear", builtin_bit_clear);
    super::register_builtin(env, "BitSet", builtin_bit_set);

    super::register_builtin(env, "RealDigits", builtin_real_digits);
    super::register_builtin(env, "Rationalize", builtin_rationalize);
    super::register_builtin(env, "Convergents", builtin_convergents);
    super::register_builtin(env, "PrevPrime", builtin_prev_prime);
    // `RandomPrime` is owned by `random` (registered later).
    super::register_builtin(env, "CarmichaelLambda", builtin_carmichael_lambda);
    super::register_builtin(env, "DivisorCount", builtin_divisor_count);
    super::register_builtin(env, "DivisorProduct", builtin_divisor_product);
    super::register_builtin(env, "IntegerName", builtin_integer_name);
    super::register_builtin(env, "NumberName", builtin_integer_name);
    super::register_builtin(env, "LetterNumber", builtin_letter_number);
    super::register_builtin(env, "DigitSum", builtin_digit_sum);
    super::register_builtin(env, "LegendreSymbol", builtin_legendre_symbol);
    super::register_builtin(env, "KroneckerSymbol", builtin_kronecker_symbol);
    super::register_builtin(env, "DiscreteLog", builtin_discrete_log);
    super::register_builtin(
        env,
        "MersennePrimeExponentQ",
        builtin_mersenne_prime_exponent_q,
    );
}

// ── Shared helpers ──────────────────────────────────────────────────────────

/// Build an unevaluated `head[args...]` value.
fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

/// Boolean → `True`/`False` symbol.
fn bool_value(b: bool) -> Value {
    Value::Symbol(if b { "True" } else { "False" }.to_string())
}

/// Extract an `Integer` from a `Value::Integer`, if possible.
fn int_of(v: &Value) -> Option<Integer> {
    match v {
        Value::Integer(n) => Some(n.clone()),
        _ => None,
    }
}

/// Extract an integer from an `Integer` or `Real` value (truncating reals).
fn number_to_int(v: &Value) -> Option<Integer> {
    match v {
        Value::Integer(n) => Some(n.clone()),
        Value::Real(r) => r.to_integer(),
        _ => None,
    }
}

fn is_prime_big(n: &Integer) -> bool {
    if *n < 2 {
        return false;
    }
    n.is_probably_prime(30) != IsPrime::No
}

fn mod_pow_int(base: &Integer, exp: &Integer, modulus: &Integer) -> Integer {
    if modulus == &Integer::from(1) {
        return Integer::from(0);
    }
    let mut result = Integer::from(1);
    let mut base_val = base.clone() % modulus;
    if base_val.is_negative() {
        base_val += modulus;
    }
    let mut exp_val = exp.clone();
    while exp_val > 0 {
        if (exp_val.clone() % 2) != 0 {
            result = result * base_val.clone() % modulus;
        }
        exp_val >>= 1u32;
        base_val = base_val.clone() * base_val.clone() % modulus;
    }
    result
}

fn mod_inverse(a: &Integer, m: &Integer) -> Option<Integer> {
    let mut old_r = ((a.clone() % m.clone()) + m.clone()) % m.clone();
    let mut r = m.clone();
    let mut old_s = Integer::from(1);
    let mut s = Integer::from(0);
    while !r.is_zero() {
        let q = old_r.clone() / r.clone();
        let tmp = r.clone();
        r = old_r.clone() - q.clone() * r.clone();
        old_r = tmp;
        let tmp = s.clone();
        s = old_s.clone() - q * s.clone();
        old_s = tmp;
    }
    if old_r != 1 {
        return None;
    }
    Some(((old_s % m.clone()) + m.clone()) % m.clone())
}

// ── Bitwise operations ──────────────────────────────────────────────────────

/// `BitAnd[n1, n2, ...]` — bitwise AND with two's-complement semantics.
pub fn builtin_bit_and(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Integer(Integer::from(-1)));
    }
    let mut acc = Integer::from(-1);
    for v in args {
        match int_of(v) {
            Some(n) => acc &= n,
            None => return Ok(unevaluated("BitAnd", args)),
        }
    }
    Ok(Value::Integer(acc))
}

/// `BitOr[n1, n2, ...]` — bitwise OR with two's-complement semantics.
pub fn builtin_bit_or(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Integer(Integer::from(0)));
    }
    let mut acc = Integer::from(0);
    for v in args {
        match int_of(v) {
            Some(n) => acc |= n,
            None => return Ok(unevaluated("BitOr", args)),
        }
    }
    Ok(Value::Integer(acc))
}

/// `BitXor[n1, n2, ...]` — bitwise XOR with two's-complement semantics.
pub fn builtin_bit_xor(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(Value::Integer(Integer::from(0)));
    }
    let mut acc = Integer::from(0);
    for v in args {
        match int_of(v) {
            Some(n) => acc ^= n,
            None => return Ok(unevaluated("BitXor", args)),
        }
    }
    Ok(Value::Integer(acc))
}

/// `BitNot[n]` — two's-complement complement (−n − 1).
pub fn builtin_bit_not(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("BitNot", args));
    }
    match int_of(&args[0]) {
        Some(n) => Ok(Value::Integer(!n)),
        None => Ok(unevaluated("BitNot", args)),
    }
}

fn shift_value(name: &str, args: &[Value], left: bool) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated(name, args));
    }
    let (n, k) = match (&args[0], &args[1]) {
        (Value::Integer(n), Value::Integer(k)) => (n, k),
        _ => return Ok(unevaluated(name, args)),
    };
    let kk = match k.to_i64() {
        Some(x) => x,
        None => return Ok(unevaluated(name, args)),
    };
    let go_left = if left { kk >= 0 } else { kk < 0 };
    let amt = kk.unsigned_abs();
    if amt > u32::MAX as u64 {
        return Ok(unevaluated(name, args));
    }
    let amt = amt as u32;
    let out = if go_left {
        n.clone() << amt
    } else {
        n.clone() >> amt
    };
    Ok(Value::Integer(out))
}

/// `BitShiftLeft[n, k]` — shift left by `k` bits (negative `k` shifts right).
pub fn builtin_bit_shift_left(args: &[Value]) -> Result<Value, EvalError> {
    shift_value("BitShiftLeft", args, true)
}

/// `BitShiftRight[n, k]` — arithmetic shift right by `k` bits.
pub fn builtin_bit_shift_right(args: &[Value]) -> Result<Value, EvalError> {
    shift_value("BitShiftRight", args, false)
}

/// `BitLength[n]` — number of significant bits (two's-complement for negatives).
pub fn builtin_bit_length(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("BitLength", args));
    }
    match int_of(&args[0]) {
        Some(n) => {
            let magnitude = if n.is_negative() {
                -n - Integer::from(1)
            } else {
                n
            };
            Ok(Value::Integer(Integer::from(magnitude.significant_bits())))
        }
        None => Ok(unevaluated("BitLength", args)),
    }
}

/// `BitGet[n, k]` — the bit at position `k` (0 or 1).
pub fn builtin_bit_get(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("BitGet", args));
    }
    let (n, k) = match (&args[0], &args[1]) {
        (Value::Integer(n), Value::Integer(k)) => (n, k),
        _ => return Ok(unevaluated("BitGet", args)),
    };
    match bit_index(k) {
        Some(bit) => Ok(Value::Integer(Integer::from(u32::from(n.get_bit(bit))))),
        None => Ok(unevaluated("BitGet", args)),
    }
}

/// `BitSetQ[n, k]` — `True` if bit `k` of `n` is set.
pub fn builtin_bit_set_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("BitSetQ", args));
    }
    let (n, k) = match (&args[0], &args[1]) {
        (Value::Integer(n), Value::Integer(k)) => (n, k),
        _ => return Ok(unevaluated("BitSetQ", args)),
    };
    match bit_index(k) {
        Some(bit) => Ok(bool_value(n.get_bit(bit))),
        None => Ok(unevaluated("BitSetQ", args)),
    }
}

/// `BitClear[n, k]` — clear the bit at position `k`.
pub fn builtin_bit_clear(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("BitClear", args));
    }
    let (n, k) = match (&args[0], &args[1]) {
        (Value::Integer(n), Value::Integer(k)) => (n, k),
        _ => return Ok(unevaluated("BitClear", args)),
    };
    match bit_index(k) {
        Some(bit) => {
            let mut out = n.clone();
            out.set_bit(bit, false);
            Ok(Value::Integer(out))
        }
        None => Ok(unevaluated("BitClear", args)),
    }
}

/// `BitSet[n, k]` — set the bit at position `k`.
pub fn builtin_bit_set(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("BitSet", args));
    }
    let (n, k) = match (&args[0], &args[1]) {
        (Value::Integer(n), Value::Integer(k)) => (n, k),
        _ => return Ok(unevaluated("BitSet", args)),
    };
    match bit_index(k) {
        Some(bit) => {
            let mut out = n.clone();
            out.set_bit(bit, true);
            Ok(Value::Integer(out))
        }
        None => Ok(unevaluated("BitSet", args)),
    }
}

/// Convert a non-negative bit index to `u32`.
fn bit_index(k: &Integer) -> Option<u32> {
    if k.is_negative() {
        return None;
    }
    k.to_u32()
}

// ── RealDigits ──────────────────────────────────────────────────────────────

fn digits_of_abs_int(mut n: Integer, base: &Integer) -> Vec<Integer> {
    if n.is_zero() {
        return vec![Integer::from(0)];
    }
    let mut out = Vec::new();
    while n > 0 {
        let d = n.clone() % base;
        out.push(d);
        n /= base;
    }
    out.reverse();
    out
}

/// `RealDigits[x]`, `RealDigits[x, b]`, `RealDigits[x, b, len]`.
///
/// Returns `{{digit, ...}, exponent}` with `x = ±0.d₁d₂… × b^exp`.
pub fn builtin_real_digits(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 3 {
        return Ok(unevaluated("RealDigits", args));
    }
    let base = match args.get(1) {
        None => Integer::from(10),
        Some(Value::Integer(b)) if *b >= 2 && *b <= 36 => b.clone(),
        Some(_) => return Ok(unevaluated("RealDigits", args)),
    };
    let limit = match args.get(2) {
        None => None,
        Some(Value::Integer(l)) if l.is_positive() => l.to_usize(),
        Some(_) => return Ok(unevaluated("RealDigits", args)),
    };
    let (num, den) = match &args[0] {
        Value::Integer(n) => (n.clone().abs(), Integer::from(1u32)),
        Value::Rational(r) => (r.numer().clone().abs(), r.denom().clone()),
        Value::Real(x) => match x.to_rational() {
            Some(r) => (r.numer().clone().abs(), r.denom().clone()),
            None => return Ok(unevaluated("RealDigits", args)),
        },
        _ => return Ok(unevaluated("RealDigits", args)),
    };
    let cap = limit.unwrap_or(16).max(1);

    if num.is_zero() {
        return Ok(Value::List(vec![
            Value::List(vec![Value::Integer(Integer::from(0))]),
            Value::Integer(Integer::from(0)),
        ]));
    }
    let int_part = num.clone() / &den;
    let mut rem = num % &den;
    let mut digits = if int_part.is_zero() {
        Vec::new()
    } else {
        digits_of_abs_int(int_part, &base)
    };
    let exp = if digits.is_empty() {
        let mut lead = 0i64;
        while !rem.is_zero() && digits.is_empty() {
            rem *= &base;
            let d = rem.clone() / &den;
            rem %= &den;
            if d.is_zero() {
                lead += 1;
            } else {
                digits.push(d);
            }
        }
        -lead
    } else {
        digits.len() as i64
    };
    while !rem.is_zero() && digits.len() < cap {
        rem *= &base;
        let d = rem.clone() / &den;
        rem %= &den;
        digits.push(d);
    }
    Ok(Value::List(vec![
        Value::List(digits.into_iter().map(Value::Integer).collect()),
        Value::Integer(Integer::from(exp)),
    ]))
}

// ── Rationalize ─────────────────────────────────────────────────────────────

fn value_from_frac(h: Integer, k: Integer, neg: bool) -> Value {
    let num = if neg { -h } else { h };
    if k == 1 {
        Value::Integer(num)
    } else {
        Value::Rational(Box::new(Rational::from((num, k))))
    }
}

/// `Rationalize[x]`, `Rationalize[x, dx]` — simplest rational within tolerance.
pub fn builtin_rationalize(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("Rationalize", args));
    }
    let r = match &args[0] {
        Value::Integer(_) => return Ok(args[0].clone()),
        Value::Rational(r) => (**r).clone(),
        Value::Real(f) => match f.to_rational() {
            Some(r) => r,
            None => return Ok(unevaluated("Rationalize", args)),
        },
        _ => return Ok(unevaluated("Rationalize", args)),
    };
    let tol_opt: Option<Rational> = match args.get(1) {
        None => None,
        Some(Value::Integer(i)) => Some(Rational::from(i.clone())),
        Some(Value::Rational(t)) => Some((**t).clone()),
        Some(Value::Real(f)) => f.to_rational(),
        Some(_) => return Ok(unevaluated("Rationalize", args)),
    };
    let tol = match tol_opt {
        Some(t) => t,
        None => {
            // Default tolerance tracks the number of significant bits in the
            // exact rational form of the value (so machine-precision reals get
            // a machine-precision tolerance, while high-precision reals keep
            // their extra digits).
            let bits = r.denom().significant_bits();
            Rational::from((Integer::from(1), Integer::from(1) << bits))
        }
    };
    if r.denom() == &Integer::from(1) {
        return Ok(Value::Integer(r.numer().clone()));
    }

    let neg = r.numer().is_negative();
    let target = r.clone();
    let mut num = r.numer().clone().abs();
    let mut den = r.denom().clone();
    let mut h_prev = Integer::from(1);
    let mut h_prev2 = Integer::from(0);
    let mut k_prev = Integer::from(0);
    let mut k_prev2 = Integer::from(1);
    loop {
        let a = num.clone() / den.clone();
        let h = a.clone() * &h_prev + &h_prev2;
        let k = a.clone() * &k_prev + &k_prev2;
        let diff = (target.clone() - Rational::from((h.clone(), k.clone()))).abs();
        if diff <= tol {
            return Ok(value_from_frac(h, k, neg));
        }
        let rem = num.clone() - a * den.clone();
        if rem.is_zero() {
            return Ok(value_from_frac(h, k, neg));
        }
        num = den;
        den = rem;
        h_prev2 = h_prev;
        h_prev = h;
        k_prev2 = k_prev;
        k_prev = k;
    }
}

// ── Convergents ─────────────────────────────────────────────────────────────

/// `Convergents[list]` — convergents of a continued fraction.
pub fn builtin_convergents(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("Convergents", args));
    }
    let terms: Vec<Integer> = match &args[0] {
        Value::List(items) => items.iter().filter_map(int_of).collect(),
        _ => match super::number_theory::builtin_continued_fraction(args)? {
            Value::List(items) => items.iter().filter_map(int_of).collect(),
            _ => return Ok(unevaluated("Convergents", args)),
        },
    };
    let mut h_prev = Integer::from(1);
    let mut h_prev2 = Integer::from(0);
    let mut k_prev = Integer::from(0);
    let mut k_prev2 = Integer::from(1);
    let mut out = Vec::with_capacity(terms.len());
    for a in &terms {
        let h = a.clone() * &h_prev + &h_prev2;
        let k = a.clone() * &k_prev + &k_prev2;
        out.push(value_from_frac(h.clone(), k.clone(), false));
        h_prev2 = h_prev;
        h_prev = h;
        k_prev2 = k_prev;
        k_prev = k;
    }
    Ok(Value::List(out))
}

// ── PrevPrime / RandomPrime ─────────────────────────────────────────────────

/// `PrevPrime[n]` — the largest prime smaller than `n`.
pub fn builtin_prev_prime(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("PrevPrime", args));
    }
    match number_to_int(&args[0]) {
        Some(n) => {
            if n <= 2 {
                Ok(Value::Integer(Integer::from(2)))
            } else {
                Ok(Value::Integer(n.prev_prime()))
            }
        }
        None => Ok(unevaluated("PrevPrime", args)),
    }
}

/// `RandomPrime[max]`, `RandomPrime[{min, max}]`, optionally repeated `n` times.
pub fn builtin_random_prime(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Ok(unevaluated("RandomPrime", args));
    }
    let (lo, hi) = match &args[0] {
        Value::Integer(n) if *n >= 2 => (Integer::from(2), n.clone()),
        Value::List(items) if items.len() == 2 => {
            let lo = match number_to_int(&items[0]) {
                Some(x) => x,
                None => return Ok(unevaluated("RandomPrime", args)),
            };
            let hi = match number_to_int(&items[1]) {
                Some(x) => x,
                None => return Ok(unevaluated("RandomPrime", args)),
            };
            (lo, hi)
        }
        _ => return Ok(unevaluated("RandomPrime", args)),
    };
    let lo = if lo < 2 { Integer::from(2) } else { lo };
    if lo > hi {
        return Ok(unevaluated("RandomPrime", args));
    }
    let count = match args.get(1) {
        None => 1usize,
        Some(Value::Integer(k)) if k.is_positive() => k.to_usize().unwrap_or(1),
        Some(_) => return Ok(unevaluated("RandomPrime", args)),
    };
    let range = hi.clone() - lo.clone() + Integer::from(1);
    let range_u64 = range.to_u64().unwrap_or(u64::MAX);
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let cand = lo.clone() + Integer::from(super::random::next_u64() % range_u64.max(1));
        let p = if is_prime_big(&cand) {
            cand.clone()
        } else {
            cand.clone().next_prime()
        };
        let p = if p > hi { cand.prev_prime() } else { p };
        out.push(Value::Integer(p));
    }
    if count == 1 {
        Ok(out.pop().unwrap())
    } else {
        Ok(Value::List(out))
    }
}

// ── CarmichaelLambda ────────────────────────────────────────────────────────

/// `CarmichaelLambda[n]` — the Carmichael function λ(n).
pub fn builtin_carmichael_lambda(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("CarmichaelLambda", args));
    }
    match int_of(&args[0]) {
        Some(n) if n.is_positive() => {
            if n == 1 {
                return Ok(Value::Integer(Integer::from(1)));
            }
            let mut lambda = Integer::from(1);
            for (p, e) in super::number_theory::factor_integer(&n) {
                let component = if p == 2 {
                    if e == 1 {
                        Integer::from(1)
                    } else if e == 2 {
                        Integer::from(2)
                    } else {
                        Integer::from(1) << (e - 2)
                    }
                } else {
                    p.clone().pow(e - 1) * (p - 1)
                };
                lambda = lambda.lcm(&component);
            }
            Ok(Value::Integer(lambda))
        }
        Some(_) => Ok(unevaluated("CarmichaelLambda", args)),
        None => Ok(unevaluated("CarmichaelLambda", args)),
    }
}

// ── DivisorCount / DivisorProduct ───────────────────────────────────────────

fn divisor_count(n: &Integer) -> u32 {
    let a = n.clone().abs();
    if a.is_zero() {
        return 0;
    }
    if a == 1 {
        return 1;
    }
    super::number_theory::factor_integer(&a)
        .iter()
        .map(|(_, e)| e + 1)
        .product()
}

/// `DivisorCount[n]` — the number of positive divisors of `n`.
pub fn builtin_divisor_count(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("DivisorCount", args));
    }
    match int_of(&args[0]) {
        Some(n) => Ok(Value::Integer(Integer::from(divisor_count(&n)))),
        None => Ok(unevaluated("DivisorCount", args)),
    }
}

/// `DivisorProduct[n]` — the product of the positive divisors of `n`.
pub fn builtin_divisor_product(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("DivisorProduct", args));
    }
    match int_of(&args[0]) {
        Some(n) => {
            let a = n.clone().abs();
            if a.is_zero() {
                return Ok(Value::Integer(Integer::from(0)));
            }
            if a == 1 {
                return Ok(Value::Integer(Integer::from(1)));
            }
            let d = divisor_count(&a);
            let product = if !d.is_multiple_of(2) {
                // Perfect square: pair each non-root divisor with its partner.
                let root = a.clone().sqrt();
                a.clone().pow((d - 1) / 2) * root
            } else {
                a.clone().pow(d / 2)
            };
            Ok(Value::Integer(product))
        }
        None => Ok(unevaluated("DivisorProduct", args)),
    }
}

// ── IntegerName / NumberName ────────────────────────────────────────────────

const NAME_ONES: [&str; 20] = [
    "",
    "one",
    "two",
    "three",
    "four",
    "five",
    "six",
    "seven",
    "eight",
    "nine",
    "ten",
    "eleven",
    "twelve",
    "thirteen",
    "fourteen",
    "fifteen",
    "sixteen",
    "seventeen",
    "eighteen",
    "nineteen",
];
const NAME_TENS: [&str; 10] = [
    "", "", "twenty", "thirty", "forty", "fifty", "sixty", "seventy", "eighty", "ninety",
];
const NAME_GROUPS: [&str; 12] = [
    "",
    "thousand",
    "million",
    "billion",
    "trillion",
    "quadrillion",
    "quintillion",
    "sextillion",
    "septillion",
    "octillion",
    "nonillion",
    "decillion",
];

fn three_digit_name(v: u32) -> String {
    let hundreds = v / 100;
    let rest = v % 100;
    let mut parts: Vec<String> = Vec::new();
    if hundreds > 0 {
        parts.push(format!("{} hundred", NAME_ONES[hundreds as usize]));
    }
    if rest > 0 {
        if rest < 20 {
            parts.push(NAME_ONES[rest as usize].to_string());
        } else if rest.is_multiple_of(10) {
            parts.push(NAME_TENS[(rest / 10) as usize].to_string());
        } else {
            parts.push(format!(
                "{}-{}",
                NAME_TENS[(rest / 10) as usize],
                NAME_ONES[(rest % 10) as usize]
            ));
        }
    }
    parts.join(" ")
}

fn integer_name(n: &Integer) -> String {
    if n.is_zero() {
        return "zero".to_string();
    }
    let negative = n.is_negative();
    let digits = n.clone().abs().to_string_radix(10);
    let bytes = digits.as_bytes();
    let mut groups: Vec<u32> = Vec::new();
    let mut idx = bytes.len();
    while idx > 0 {
        let start = idx.saturating_sub(3);
        groups.push(digits[start..idx].parse::<u32>().unwrap_or(0));
        idx = start;
    }
    groups.reverse();
    let group_count = groups.len();
    let mut parts: Vec<String> = Vec::new();
    for (gi, g) in groups.iter().enumerate() {
        if *g == 0 {
            continue;
        }
        let name = NAME_GROUPS.get(group_count - 1 - gi).copied().unwrap_or("");
        let mut words = three_digit_name(*g);
        if !name.is_empty() {
            words = format!("{words} {name}");
        }
        parts.push(words);
    }
    let joined = parts.join(" ");
    if negative {
        format!("minus {joined}")
    } else {
        joined
    }
}

/// `IntegerName[n]` / `NumberName[n]` — spell out an integer in English.
pub fn builtin_integer_name(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("IntegerName", args));
    }
    match int_of(&args[0]) {
        Some(n) => Ok(Value::Str(integer_name(&n))),
        None => Ok(unevaluated("IntegerName", args)),
    }
}

// ── LetterNumber / DigitSum ─────────────────────────────────────────────────

/// `LetterNumber[string]` — alphabet position of a single letter (case-insensitive).
pub fn builtin_letter_number(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("LetterNumber", args));
    }
    match &args[0] {
        Value::Str(s) if s.chars().count() == 1 => {
            let c = s.chars().next().unwrap();
            if c.is_ascii_alphabetic() {
                let pos = u32::from(c.to_ascii_lowercase()) - u32::from('a') + 1;
                Ok(Value::Integer(Integer::from(pos)))
            } else {
                Ok(unevaluated("LetterNumber", args))
            }
        }
        _ => Ok(unevaluated("LetterNumber", args)),
    }
}

/// `DigitSum[n]`, `DigitSum[n, b]`, `DigitSum[n, b, k]` — sum of `k`-th powers of digits.
pub fn builtin_digit_sum(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 3 {
        return Ok(unevaluated("DigitSum", args));
    }
    let base = match args.get(1) {
        None => Integer::from(10),
        Some(Value::Integer(b)) if *b >= 2 => b.clone(),
        Some(_) => return Ok(unevaluated("DigitSum", args)),
    };
    let k = match args.get(2) {
        None => 1u32,
        Some(Value::Integer(k)) if !k.is_negative() => match k.to_u32() {
            Some(x) => x,
            None => return Ok(unevaluated("DigitSum", args)),
        },
        Some(_) => return Ok(unevaluated("DigitSum", args)),
    };
    match int_of(&args[0]) {
        Some(n) => {
            let mut m = n.abs();
            let mut sum = Integer::from(0);
            while m > 0 {
                let d = m.clone() % &base;
                sum += d.pow(k);
                m /= &base;
            }
            Ok(Value::Integer(sum))
        }
        None => Ok(unevaluated("DigitSum", args)),
    }
}

// ── Symbol functions ────────────────────────────────────────────────────────

/// Jacobi symbol `(a/n)` for odd positive `n`.
fn jacobi_sym(a: &Integer, n: &Integer) -> i32 {
    let mut a = ((a.clone() % n.clone()) + n.clone()) % n.clone();
    let mut n = n.clone();
    let mut t = 1i32;
    let two = Integer::from(2);
    let four = Integer::from(4);
    let eight = Integer::from(8);
    while !a.is_zero() {
        let mut e = 0u32;
        while (a.clone() % &two).is_zero() {
            a /= 2u32;
            e += 1;
        }
        if !e.is_multiple_of(2) {
            let n_mod_8 = n.clone() % &eight;
            if n_mod_8 == 3 || n_mod_8 == 5 {
                t = -t;
            }
        }
        let a_mod_4 = a.clone() % &four;
        let n_mod_4 = n.clone() % &four;
        if a_mod_4 == 3 && n_mod_4 == 3 {
            t = -t;
        }
        let tmp = a.clone();
        a = n % tmp.clone();
        n = tmp;
    }
    if n == 1 { t } else { 0 }
}

fn kronecker_sym(a: &Integer, n: &Integer) -> i32 {
    if n.is_zero() {
        return if a.clone().abs() == 1 { 1 } else { 0 };
    }
    let mut result = 1i32;
    let mut nn = n.clone();
    if nn.is_negative() {
        nn = -nn;
        if a.is_negative() {
            result = -result;
        }
    }
    let two = Integer::from(2);
    let eight = Integer::from(8);
    let mut e = 0u32;
    while (nn.clone() % &two).is_zero() {
        nn /= 2u32;
        e += 1;
    }
    if e > 0 {
        if (a.clone() % &two).is_zero() {
            return 0;
        }
        let a_mod_8 = ((a.clone() % &eight) + &eight) % &eight;
        let s = if a_mod_8 == 1 || a_mod_8 == 7 { 1 } else { -1 };
        if !e.is_multiple_of(2) {
            result *= s;
        }
    }
    if nn > 1 {
        result *= jacobi_sym(a, &nn);
    }
    result
}

/// `LegendreSymbol[a, p]` — Legendre symbol for odd prime `p`.
pub fn builtin_legendre_symbol(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("LegendreSymbol", args));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(p))
            if p.is_positive() && !(p.clone() % 2u32).is_zero() =>
        {
            Ok(Value::Integer(Integer::from(jacobi_sym(a, p))))
        }
        _ => Ok(unevaluated("LegendreSymbol", args)),
    }
}

/// `KroneckerSymbol[a, n]` — Kronecker symbol for any integer `n`.
pub fn builtin_kronecker_symbol(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Ok(unevaluated("KroneckerSymbol", args));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(a), Value::Integer(n)) => {
            Ok(Value::Integer(Integer::from(kronecker_sym(a, n))))
        }
        _ => Ok(unevaluated("KroneckerSymbol", args)),
    }
}

// ── DiscreteLog ─────────────────────────────────────────────────────────────

/// `DiscreteLog[k, g, p]` — `x` with `g^x ≡ k (mod p)` via baby-step giant-step.
pub fn builtin_discrete_log(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Ok(unevaluated("DiscreteLog", args));
    }
    let (k, g, p) = match (&args[0], &args[1], &args[2]) {
        (Value::Integer(k), Value::Integer(g), Value::Integer(p)) => (k, g, p),
        _ => return Ok(unevaluated("DiscreteLog", args)),
    };
    if *p <= 1 {
        return Ok(Value::Integer(Integer::from(0)));
    }
    let g = ((g.clone() % p.clone()) + p.clone()) % p.clone();
    let k = ((k.clone() % p.clone()) + p.clone()) % p.clone();
    let inv_g = match mod_inverse(&g, p) {
        Some(x) => x,
        None => return Ok(unevaluated("DiscreteLog", args)),
    };
    let m = p.clone().sqrt() + Integer::from(1);
    let mm = m.to_u64().unwrap_or(u64::MAX);
    let mut table: HashMap<Integer, u64> = HashMap::new();
    let mut cur = Integer::from(1) % p;
    for j in 0..mm {
        table.entry(cur.clone()).or_insert(j);
        cur = cur * g.clone() % p;
    }
    let factor = mod_pow_int(&inv_g, &m, p);
    let mut gamma = k;
    for i in 0..=mm {
        if let Some(&j) = table.get(&gamma) {
            return Ok(Value::Integer(Integer::from(i * mm + j)));
        }
        gamma = gamma * factor.clone() % p;
    }
    Ok(unevaluated("DiscreteLog", args))
}

// ── MersennePrimeExponentQ ──────────────────────────────────────────────────

/// `MersennePrimeExponentQ[n]` — `True` if `2^n − 1` is a Mersenne prime.
pub fn builtin_mersenne_prime_exponent_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Ok(unevaluated("MersennePrimeExponentQ", args));
    }
    match int_of(&args[0]) {
        Some(n) if n >= 2 => {
            let nu = match n.to_u32() {
                Some(x) => x,
                None => return Ok(unevaluated("MersennePrimeExponentQ", args)),
            };
            if !super::number_theory::is_prime_u64(u64::from(nu)) {
                return Ok(bool_value(false));
            }
            let m = (Integer::from(1) << nu) - Integer::from(1);
            Ok(bool_value(is_prime_big(&m)))
        }
        Some(_) => Ok(bool_value(false)),
        None => Ok(unevaluated("MersennePrimeExponentQ", args)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::DEFAULT_PRECISION;

    fn i(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn r(n: i64, d: i64) -> Value {
        Value::Rational(Box::new(Rational::from((n, d))))
    }

    fn real(x: f64) -> Value {
        Value::Real(Float::with_val(DEFAULT_PRECISION, x))
    }

    fn lst(items: Vec<Value>) -> Value {
        Value::List(items)
    }

    #[test]
    fn test_bit_and_or_xor() {
        assert_eq!(builtin_bit_and(&[i(0b1100), i(0b1010)]).unwrap(), i(0b1000));
        assert_eq!(builtin_bit_or(&[i(0b1100), i(0b1010)]).unwrap(), i(0b1110));
        assert_eq!(builtin_bit_xor(&[i(0b1100), i(0b1010)]).unwrap(), i(0b0110));
        // Two's-complement semantics for negatives.
        assert_eq!(builtin_bit_and(&[i(-1), i(42)]).unwrap(), i(42));
        assert_eq!(builtin_bit_or(&[i(-2), i(1)]).unwrap(), i(-1));
        assert_eq!(builtin_bit_xor(&[i(-1), i(-1)]).unwrap(), i(0));
        assert_eq!(builtin_bit_not(&[i(0)]).unwrap(), i(-1));
        assert_eq!(builtin_bit_not(&[i(-3)]).unwrap(), i(2));
    }

    #[test]
    fn test_bit_shifts() {
        assert_eq!(builtin_bit_shift_left(&[i(3), i(4)]).unwrap(), i(48));
        assert_eq!(builtin_bit_shift_right(&[i(48), i(4)]).unwrap(), i(3));
        // Negative shift amount reverses direction.
        assert_eq!(builtin_bit_shift_left(&[i(3), i(-1)]).unwrap(), i(1));
        assert_eq!(builtin_bit_shift_right(&[i(-1), i(1)]).unwrap(), i(-1));
    }

    #[test]
    fn test_bit_length_and_bits() {
        assert_eq!(builtin_bit_length(&[i(0)]).unwrap(), i(0));
        assert_eq!(builtin_bit_length(&[i(1)]).unwrap(), i(1));
        assert_eq!(builtin_bit_length(&[i(255)]).unwrap(), i(8));
        assert_eq!(builtin_bit_length(&[i(-1)]).unwrap(), i(0));
        assert_eq!(builtin_bit_length(&[i(-2)]).unwrap(), i(1));
        assert_eq!(builtin_bit_get(&[i(0b1010), i(1)]).unwrap(), i(1));
        assert_eq!(builtin_bit_get(&[i(0b1010), i(0)]).unwrap(), i(0));
        assert_eq!(
            builtin_bit_set_q(&[i(0b1010), i(3)]).unwrap(),
            bool_value(true)
        );
        assert_eq!(builtin_bit_clear(&[i(0b1111), i(1)]).unwrap(), i(0b1101));
        assert_eq!(builtin_bit_set(&[i(0b1000), i(0)]).unwrap(), i(0b1001));
    }

    #[test]
    fn test_real_digits() {
        assert_eq!(
            builtin_real_digits(&[i(123)]).unwrap(),
            lst(vec![lst(vec![i(1), i(2), i(3)]), i(3)])
        );
        assert_eq!(
            builtin_real_digits(&[i(0)]).unwrap(),
            lst(vec![lst(vec![i(0)]), i(0)])
        );
        assert_eq!(
            builtin_real_digits(&[r(1, 8)]).unwrap(),
            lst(vec![lst(vec![i(1), i(2), i(5)]), i(0)])
        );
        assert_eq!(
            builtin_real_digits(&[r(1, 2)]).unwrap(),
            lst(vec![lst(vec![i(5)]), i(0)])
        );
        // Repeating decimal: 1/3 → sixteen 3's.
        let threes = vec![i(3); 16];
        assert_eq!(
            builtin_real_digits(&[r(1, 3)]).unwrap(),
            lst(vec![lst(threes), i(0)])
        );
        // Small magnitude exponent.
        assert_eq!(
            builtin_real_digits(&[r(1, 1000)]).unwrap(),
            lst(vec![lst(vec![i(1)]), i(-2)])
        );
        // Base 2.
        assert_eq!(
            builtin_real_digits(&[i(5), i(2)]).unwrap(),
            lst(vec![lst(vec![i(1), i(0), i(1)]), i(3)])
        );
    }

    #[test]
    fn test_rationalize() {
        assert_eq!(builtin_rationalize(&[r(2, 3)]).unwrap(), r(2, 3));
        assert_eq!(builtin_rationalize(&[real(0.1)]).unwrap(), r(1, 10));
        assert_eq!(builtin_rationalize(&[real(0.25)]).unwrap(), r(1, 4));
        assert_eq!(
            builtin_rationalize(&[real(0.5), real(0.01)]).unwrap(),
            r(1, 2)
        );
    }

    #[test]
    fn test_convergents() {
        assert_eq!(
            builtin_convergents(&[lst(vec![i(0), i(2), i(2)])]).unwrap(),
            lst(vec![i(0), r(1, 2), r(2, 5)])
        );
        assert_eq!(
            builtin_convergents(&[lst(vec![i(1), i(1), i(1)])]).unwrap(),
            lst(vec![i(1), i(2), r(3, 2)])
        );
    }

    #[test]
    fn test_prev_prime() {
        assert_eq!(builtin_prev_prime(&[i(10)]).unwrap(), i(7));
        assert_eq!(builtin_prev_prime(&[i(3)]).unwrap(), i(2));
        assert_eq!(builtin_prev_prime(&[i(100)]).unwrap(), i(97));
    }

    #[test]
    fn test_random_prime() {
        let p = builtin_random_prime(&[i(100)]).unwrap();
        match p {
            Value::Integer(n) => {
                assert!(n <= 100 && is_prime_big(&n));
            }
            other => panic!("expected integer, got {other:?}"),
        }
        let ps = builtin_random_prime(&[lst(vec![i(10), i(50)]), i(5)]).unwrap();
        match ps {
            Value::List(items) => {
                assert_eq!(items.len(), 5);
                for it in items {
                    match it {
                        Value::Integer(n) => {
                            assert!((10..=50).contains(&n) && is_prime_big(&n))
                        }
                        other => panic!("expected integer, got {other:?}"),
                    }
                }
            }
            other => panic!("expected list, got {other:?}"),
        }
    }

    #[test]
    fn test_carmichael_lambda() {
        assert_eq!(builtin_carmichael_lambda(&[i(1)]).unwrap(), i(1));
        assert_eq!(builtin_carmichael_lambda(&[i(2)]).unwrap(), i(1));
        assert_eq!(builtin_carmichael_lambda(&[i(8)]).unwrap(), i(2));
        assert_eq!(builtin_carmichael_lambda(&[i(9)]).unwrap(), i(6));
        assert_eq!(builtin_carmichael_lambda(&[i(15)]).unwrap(), i(4));
        assert_eq!(builtin_carmichael_lambda(&[i(21)]).unwrap(), i(6));
    }

    #[test]
    fn test_divisor_count_and_product() {
        assert_eq!(builtin_divisor_count(&[i(1)]).unwrap(), i(1));
        assert_eq!(builtin_divisor_count(&[i(12)]).unwrap(), i(6));
        assert_eq!(builtin_divisor_count(&[i(36)]).unwrap(), i(9));
        assert_eq!(builtin_divisor_product(&[i(1)]).unwrap(), i(1));
        assert_eq!(builtin_divisor_product(&[i(12)]).unwrap(), i(1728));
        assert_eq!(builtin_divisor_product(&[i(9)]).unwrap(), i(27));
    }

    #[test]
    fn test_integer_name() {
        assert_eq!(
            builtin_integer_name(&[i(0)]).unwrap(),
            Value::Str("zero".into())
        );
        assert_eq!(
            builtin_integer_name(&[i(123)]).unwrap(),
            Value::Str("one hundred twenty-three".into())
        );
        assert_eq!(
            builtin_integer_name(&[i(1000000)]).unwrap(),
            Value::Str("one million".into())
        );
        assert_eq!(
            builtin_integer_name(&[i(-5)]).unwrap(),
            Value::Str("minus five".into())
        );
        assert_eq!(
            builtin_integer_name(&[i(1234567)]).unwrap(),
            Value::Str(
                "one million two hundred thirty-four thousand five hundred sixty-seven".into()
            )
        );
    }

    #[test]
    fn test_letter_number() {
        assert_eq!(
            builtin_letter_number(&[Value::Str("a".into())]).unwrap(),
            i(1)
        );
        assert_eq!(
            builtin_letter_number(&[Value::Str("Z".into())]).unwrap(),
            i(26)
        );
        assert_eq!(
            builtin_letter_number(&[Value::Str("m".into())]).unwrap(),
            i(13)
        );
    }

    #[test]
    fn test_digit_sum() {
        assert_eq!(builtin_digit_sum(&[i(12345)]).unwrap(), i(15));
        assert_eq!(builtin_digit_sum(&[i(255), i(2)]).unwrap(), i(8));
        assert_eq!(builtin_digit_sum(&[i(123), i(10), i(2)]).unwrap(), i(14));
        assert_eq!(builtin_digit_sum(&[i(0)]).unwrap(), i(0));
    }

    #[test]
    fn test_symbols() {
        assert_eq!(builtin_legendre_symbol(&[i(2), i(7)]).unwrap(), i(1));
        assert_eq!(builtin_legendre_symbol(&[i(3), i(7)]).unwrap(), i(-1));
        assert_eq!(builtin_jacobi_via_kronecker(4, 9), 1);
        assert_eq!(builtin_kronecker_symbol(&[i(2), i(15)]).unwrap(), i(1));
        assert_eq!(builtin_kronecker_symbol(&[i(3), i(15)]).unwrap(), i(0));
        assert_eq!(builtin_kronecker_symbol(&[i(-1), i(-1)]).unwrap(), i(-1));
    }

    fn builtin_jacobi_via_kronecker(a: i64, n: i64) -> i32 {
        match builtin_kronecker_symbol(&[i(a), i(n)]).unwrap() {
            Value::Integer(v) => v.to_i32().unwrap(),
            other => panic!("expected integer, got {other:?}"),
        }
    }

    #[test]
    fn test_discrete_log() {
        // 2^6 = 64 ≡ 9 (mod 11)
        assert_eq!(builtin_discrete_log(&[i(9), i(2), i(11)]).unwrap(), i(6));
        // 3^4 = 81 ≡ 5 (mod 19)
        assert_eq!(builtin_discrete_log(&[i(5), i(3), i(19)]).unwrap(), i(4));
    }

    #[test]
    fn test_mersenne_prime_exponent_q() {
        assert_eq!(
            builtin_mersenne_prime_exponent_q(&[i(2)]).unwrap(),
            bool_value(true)
        );
        assert_eq!(
            builtin_mersenne_prime_exponent_q(&[i(3)]).unwrap(),
            bool_value(true)
        );
        assert_eq!(
            builtin_mersenne_prime_exponent_q(&[i(5)]).unwrap(),
            bool_value(true)
        );
        assert_eq!(
            builtin_mersenne_prime_exponent_q(&[i(11)]).unwrap(),
            bool_value(false)
        );
        assert_eq!(
            builtin_mersenne_prime_exponent_q(&[i(1)]).unwrap(),
            bool_value(false)
        );
        assert_eq!(
            builtin_mersenne_prime_exponent_q(&[i(13)]).unwrap(),
            bool_value(true)
        );
    }

    #[test]
    fn test_unevaluated_forms() {
        assert!(matches!(
            builtin_bit_and(&[Value::Str("x".into())]).unwrap(),
            Value::Call { .. }
        ));
        assert!(matches!(
            builtin_real_digits(&[Value::Symbol("x".into())]).unwrap(),
            Value::Call { .. }
        ));
        assert!(matches!(
            builtin_carmichael_lambda(&[i(0)]).unwrap(),
            Value::Call { .. }
        ));
    }
}
