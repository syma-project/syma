/// Discrete calculus builtin integration tests

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── DiscreteDelta ──

#[test]
fn test_discrete_delta_zero() {
    let out = syma_eval("DiscreteDelta[0]");
    assert!(
        out.contains("1"),
        "DiscreteDelta[0] should be 1, got: {out}"
    );
}

#[test]
fn test_discrete_delta_nonzero() {
    let out = syma_eval("DiscreteDelta[1]");
    assert!(
        out.contains("0"),
        "DiscreteDelta[1] should be 0, got: {out}"
    );
}

#[test]
fn test_discrete_delta_multi_arg() {
    let out = syma_eval("DiscreteDelta[1, 1]");
    assert!(
        !out.contains("error"),
        "DiscreteDelta[1,1] should not error, got: {out}"
    );
}

// ── DiscreteShift ──

#[test]
fn test_discrete_shift() {
    let out = syma_eval("DiscreteShift[f[n], n]");
    assert!(
        !out.contains("error"),
        "DiscreteShift should not error, got: {out}"
    );
}

// ── DiscreteRatio ──

#[test]
fn test_discrete_ratio() {
    let out = syma_eval("DiscreteRatio[f[n], n]");
    assert!(
        !out.contains("error"),
        "DiscreteRatio should not error, got: {out}"
    );
}

// ── FactorialPower ──

#[test]
fn test_factorial_power() {
    let out = syma_eval("FactorialPower[x, 3]");
    assert!(
        !out.contains("error"),
        "FactorialPower should not error, got: {out}"
    );
}

// ── BernoulliB ──

#[test]
fn test_bernoulli_b_zero() {
    let out = syma_eval("BernoulliB[0]");
    assert!(out.contains("1"), "BernoulliB[0] should be 1, got: {out}");
}

#[test]
fn test_bernoulli_b_one() {
    let out = syma_eval("BernoulliB[1]");
    assert!(
        !out.contains("error"),
        "BernoulliB[1] should not error, got: {out}"
    );
}

#[test]
fn test_bernoulli_b_four() {
    let out = syma_eval("BernoulliB[4]");
    assert!(
        !out.contains("error"),
        "BernoulliB[4] should not error, got: {out}"
    );
}

// ── LinearRecurrence ──

#[test]
fn test_linear_recurrence_fib() {
    let out = syma_eval("LinearRecurrence[{1, 1}, {1, 1}, 10]");
    assert!(
        !out.contains("error"),
        "LinearRecurrence should not error, got: {out}"
    );
}

// ── RSolve ──

#[test]
fn test_rsolve_basic() {
    let out = syma_eval("RSolve[a[n+1] == a[n] + 1, a[n], n]");
    assert!(
        !out.contains("error"),
        "RSolve should not error, got: {out}"
    );
}

// ── RecurrenceTable ──

#[test]
fn test_recurrence_table() {
    let out = syma_eval("RecurrenceTable[{a[n+1] == a[n] + 1, a[1] == 1}, a, {n, 1, 5}]");
    assert!(
        !out.contains("error"),
        "RecurrenceTable should not error, got: {out}"
    );
}
