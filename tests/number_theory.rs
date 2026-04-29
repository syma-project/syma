/// Number theory builtin integration tests

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── PrimeQ ──

#[test]
fn test_prime_q_true() {
    let out = syma_eval("PrimeQ[7]");
    assert!(out.contains("True"), "PrimeQ[7] should be True, got: {out}");
}

#[test]
fn test_prime_q_false() {
    let out = syma_eval("PrimeQ[4]");
    assert!(
        out.contains("False"),
        "PrimeQ[4] should be False, got: {out}"
    );
}

#[test]
fn test_prime_q_one() {
    let out = syma_eval("PrimeQ[1]");
    assert!(
        out.contains("False"),
        "PrimeQ[1] should be False, got: {out}"
    );
}

// ── Prime / PrimePi / NextPrime ──

#[test]
fn test_prime_first() {
    let out = syma_eval("Prime[1]");
    assert!(out.contains("2"), "Prime[1] should be 2, got: {out}");
}

#[test]
fn test_prime_fifth() {
    let out = syma_eval("Prime[5]");
    assert!(out.contains("11"), "Prime[5] should be 11, got: {out}");
}

#[test]
fn test_prime_pi_10() {
    let out = syma_eval("PrimePi[10]");
    assert!(out.contains("4"), "PrimePi[10] should be 4, got: {out}");
}

#[test]
fn test_next_prime_after_5() {
    let out = syma_eval("NextPrime[5]");
    assert!(out.contains("7"), "NextPrime[5] should be 7, got: {out}");
}

#[test]
fn test_next_prime_after_10() {
    let out = syma_eval("NextPrime[10]");
    assert!(out.contains("11"), "NextPrime[10] should be 11, got: {out}");
}

// ── FactorInteger ──

#[test]
fn test_factor_integer_12() {
    let out = syma_eval("FactorInteger[12]");
    assert!(
        out.contains("2") || out.contains("3"),
        "FactorInteger[12] should contain 2 and 3, got: {out}"
    );
}

#[test]
fn test_factor_integer_prime() {
    let out = syma_eval("FactorInteger[17]");
    assert!(
        !out.contains("error"),
        "FactorInteger[17] should not error, got: {out}"
    );
}

// ── Divisors ──

#[test]
fn test_divisors_12() {
    let out = syma_eval("Divisors[12]");
    assert!(
        out.contains("1") && out.contains("6"),
        "Divisors[12] should contain 1 and 6, got: {out}"
    );
}

#[test]
fn test_divisors_prime() {
    let out = syma_eval("Divisors[7]");
    assert!(
        !out.contains("error"),
        "Divisors[7] should not error, got: {out}"
    );
}

// ── Divisible / CoprimeQ ──

#[test]
fn test_divisible_true() {
    let out = syma_eval("Divisible[10, 5]");
    assert!(
        out.contains("True"),
        "Divisible[10,5] should be True, got: {out}"
    );
}

#[test]
fn test_divisible_false() {
    let out = syma_eval("Divisible[10, 3]");
    assert!(
        out.contains("False"),
        "Divisible[10,3] should be False, got: {out}"
    );
}

#[test]
fn test_coprime_q_true() {
    let out = syma_eval("CoprimeQ[8, 9]");
    assert!(
        out.contains("True"),
        "CoprimeQ[8,9] should be True, got: {out}"
    );
}

#[test]
fn test_coprime_q_false() {
    let out = syma_eval("CoprimeQ[8, 12]");
    assert!(
        out.contains("False"),
        "CoprimeQ[8,12] should be False, got: {out}"
    );
}

// ── IntegerDigits / DigitCount ──

#[test]
fn test_integer_digits_123() {
    let out = syma_eval("IntegerDigits[123]");
    assert!(
        out.contains("1") && out.contains("2") && out.contains("3"),
        "IntegerDigits[123] should contain 1,2,3, got: {out}"
    );
}

#[test]
fn test_integer_digits_zero() {
    let out = syma_eval("IntegerDigits[0]");
    assert!(
        !out.contains("error"),
        "IntegerDigits[0] should not error, got: {out}"
    );
}

#[test]
fn test_digit_count() {
    let out = syma_eval("DigitCount[1123]");
    assert!(
        !out.contains("error"),
        "DigitCount[1123] should not error, got: {out}"
    );
}

// ── DivisorSigma ──

#[test]
fn test_divisor_sigma_1_6() {
    let out = syma_eval("DivisorSigma[1, 6]");
    assert!(
        out.contains("12"),
        "DivisorSigma[1,6] should be 12, got: {out}"
    );
}

// ── EulerPhi ──

#[test]
fn test_euler_phi_6() {
    let out = syma_eval("EulerPhi[6]");
    assert!(out.contains("2"), "EulerPhi[6] should be 2, got: {out}");
}

#[test]
fn test_euler_phi_prime() {
    let out = syma_eval("EulerPhi[7]");
    assert!(out.contains("6"), "EulerPhi[7] should be 6, got: {out}");
}

// ── MoebiusMu ──

#[test]
fn test_moebius_mu_1() {
    let out = syma_eval("MoebiusMu[1]");
    assert!(out.contains("1"), "MoebiusMu[1] should be 1, got: {out}");
}

#[test]
fn test_moebius_mu_squarefree() {
    let out = syma_eval("MoebiusMu[6]");
    assert!(
        !out.contains("error"),
        "MoebiusMu[6] should not error, got: {out}"
    );
}

#[test]
fn test_moebius_mu_with_square() {
    let out = syma_eval("MoebiusMu[4]");
    assert!(out.contains("0"), "MoebiusMu[4] should be 0, got: {out}");
}

// ── PowerMod / ModularInverse ──

#[test]
fn test_power_mod() {
    let out = syma_eval("PowerMod[2, 3, 5]");
    assert!(out.contains("3"), "PowerMod[2,3,5] should be 3, got: {out}");
}

#[test]
fn test_modular_inverse() {
    let out = syma_eval("ModularInverse[3, 7]");
    assert!(
        out.contains("5"),
        "ModularInverse[3,7] should be 5, got: {out}"
    );
}

// ── PrimeOmega / PrimeNu ──

#[test]
fn test_prime_omega_12() {
    let out = syma_eval("PrimeOmega[12]");
    assert!(out.contains("3"), "PrimeOmega[12] should be 3, got: {out}");
}

#[test]
fn test_prime_nu_12() {
    let out = syma_eval("PrimeNu[12]");
    assert!(out.contains("2"), "PrimeNu[12] should be 2, got: {out}");
}

// ── SquareFreeQ / CompositeQ / PrimePowerQ ──

#[test]
fn test_square_free_q_true() {
    let out = syma_eval("SquareFreeQ[6]");
    assert!(
        out.contains("True"),
        "SquareFreeQ[6] should be True, got: {out}"
    );
}

#[test]
fn test_square_free_q_false() {
    let out = syma_eval("SquareFreeQ[12]");
    assert!(
        out.contains("False"),
        "SquareFreeQ[12] should be False, got: {out}"
    );
}

#[test]
fn test_composite_q_true() {
    let out = syma_eval("CompositeQ[4]");
    assert!(
        out.contains("True"),
        "CompositeQ[4] should be True, got: {out}"
    );
}

#[test]
fn test_composite_q_false() {
    let out = syma_eval("CompositeQ[7]");
    assert!(
        out.contains("False"),
        "CompositeQ[7] should be False, got: {out}"
    );
}

#[test]
fn test_prime_power_q_true() {
    let out = syma_eval("PrimePowerQ[9]");
    assert!(
        out.contains("True"),
        "PrimePowerQ[9] should be True, got: {out}"
    );
}

#[test]
fn test_prime_power_q_false() {
    let out = syma_eval("PrimePowerQ[6]");
    assert!(
        out.contains("False"),
        "PrimePowerQ[6] should be False, got: {out}"
    );
}

// ── PerfectNumberQ ──

#[test]
fn test_perfect_number_q_6() {
    let out = syma_eval("PerfectNumberQ[6]");
    assert!(
        out.contains("True"),
        "PerfectNumberQ[6] should be True, got: {out}"
    );
}

#[test]
fn test_perfect_number_q_28() {
    let out = syma_eval("PerfectNumberQ[28]");
    assert!(
        out.contains("True"),
        "PerfectNumberQ[28] should be True, got: {out}"
    );
}

#[test]
fn test_perfect_number_q_false() {
    let out = syma_eval("PerfectNumberQ[10]");
    assert!(
        out.contains("False"),
        "PerfectNumberQ[10] should be False, got: {out}"
    );
}

// ── FromDigits / ContinuedFraction ──

#[test]
fn test_from_digits() {
    let out = syma_eval("FromDigits[{1, 2, 3}]");
    assert!(
        out.contains("123"),
        "FromDigits[{{1,2,3}}] should be 123, got: {out}"
    );
}

#[test]
fn test_continued_fraction() {
    let out = syma_eval("ContinuedFraction[Pi, 5]");
    assert!(
        !out.contains("error"),
        "ContinuedFraction[Pi,5] should not error, got: {out}"
    );
}

#[test]
fn test_from_continued_fraction() {
    let out = syma_eval("FromContinuedFraction[{2}]");
    assert!(
        !out.contains("error"),
        "FromContinuedFraction[{{2}}] should not error, got: {out}"
    );
}

// ── IntegerExponent ──

#[test]
fn test_integer_exponent() {
    let out = syma_eval("IntegerExponent[100, 10]");
    assert!(
        out.contains("2"),
        "IntegerExponent[100,10] should be 2, got: {out}"
    );
}

// ── NumberExpand ──

#[test]
fn test_number_expand() {
    let out = syma_eval("NumberExpand[123]");
    assert!(
        !out.contains("error"),
        "NumberExpand[123] should not error, got: {out}"
    );
}

// ── JacobiSymbol ──

#[test]
fn test_jacobi_symbol() {
    let out = syma_eval("JacobiSymbol[2, 7]");
    assert!(
        !out.contains("error"),
        "JacobiSymbol[2,7] should not error, got: {out}"
    );
}

// ── ChineseRemainder ──

#[test]
fn test_chinese_remainder() {
    let out = syma_eval("ChineseRemainder[{2, 3}, {3, 5}]");
    assert!(
        !out.contains("error"),
        "ChineseRemainder should not error, got: {out}"
    );
}
