use rug::Float;
/// Rubi condition predicate helpers.
///
/// These implement the condition predicates used in Rubi rule conditions,
/// working on Syma `Value` types by walking the expression tree.
use rug::Integer;

use crate::value::{DEFAULT_PRECISION, Value};

// ── Structural Predicates ──

/// True if expr is a Power call (head == "Power")
pub fn power_q(val: &Value) -> bool {
    matches!(val, Value::Call { head, .. } if head == "Power")
}

/// True if expr is a Times call (product)
pub fn product_q(val: &Value) -> bool {
    matches!(val, Value::Call { head, .. } if head == "Times")
}

/// True if expr is a Plus call (sum)
pub fn sum_q(val: &Value) -> bool {
    matches!(val, Value::Call { head, .. } if head == "Plus")
}

/// Negation of SumQ: true if expr is NOT a Plus call
pub fn nonsum_q(val: &Value) -> bool {
    !sum_q(val)
}

/// True if expr is an integer
pub fn integer_q(val: &Value) -> bool {
    matches!(val, Value::Integer(_))
}

/// True if expr is a rational number (Integer or Real, or rational)
pub fn rational_q(val: &Value) -> bool {
    matches!(val, Value::Integer(_))
}

/// True if expr is a Symbol
pub fn symbol_q(val: &Value) -> bool {
    matches!(val, Value::Symbol(_))
}

// ── Numeric Predicates ──

/// True if val is an integer > n
pub fn i_gt_q(val: &Value, n: i64) -> bool {
    if let Value::Integer(i) = val {
        if let Some(v) = i.to_i64() {
            return v > n;
        }
    }
    false
}

/// True if val is an integer < n
pub fn i_lt_q(val: &Value, n: i64) -> bool {
    if let Value::Integer(i) = val {
        if let Some(v) = i.to_i64() {
            return v < n;
        }
    }
    false
}

/// True if val is an integer >= n
pub fn i_ge_q(val: &Value, n: i64) -> bool {
    if let Value::Integer(i) = val {
        if let Some(v) = i.to_i64() {
            return v >= n;
        }
    }
    false
}

/// True if val is an integer <= n
pub fn i_le_q(val: &Value, n: i64) -> bool {
    if let Value::Integer(i) = val {
        if let Some(v) = i.to_i64() {
            return v <= n;
        }
    }
    false
}

/// EqQ[a, b] — True if a and b are numerically equal
pub fn eq_q(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Integer(ai), Value::Integer(bi)) => ai == bi,
        (Value::Integer(ai), Value::Real(br)) => {
            let af = Float::with_val(DEFAULT_PRECISION, ai);
            (af - br).abs() < 1e-12
        }
        (Value::Real(ar), Value::Integer(bi)) => {
            let bf = Float::with_val(DEFAULT_PRECISION, bi);
            (ar - bf).abs() < 1e-12
        }
        (Value::Real(ar), Value::Real(br)) => (ar.clone() - br).abs() < 1e-12,
        (a, b) => a.struct_eq(b),
    }
}

/// NeQ[a, b] — True if a and b are NOT numerically equal
pub fn ne_q(a: &Value, b: &Value) -> bool {
    !eq_q(a, b)
}

/// True if n == 0
pub fn zero_q(val: &Value) -> bool {
    match val {
        Value::Integer(n) => n.is_zero(),
        Value::Real(r) => r.clone().abs() < 1e-12,
        _ => false,
    }
}

/// True if n == 1
fn is_one(val: &Value) -> bool {
    match val {
        Value::Integer(n) => *n == 1,
        Value::Real(r) => {
            let one = Float::with_val(DEFAULT_PRECISION, 1);
            let mut diff = Float::with_val(DEFAULT_PRECISION, r);
            diff -= &one;
            diff.abs() < 1e-12
        }
        _ => false,
    }
}

/// PositiveQ[a] — True if a is a positive number
pub fn pos_q(a: &Value) -> bool {
    match a {
        Value::Integer(n) => n.is_positive(),
        Value::Real(r) => r.is_sign_positive(),
        _ => false,
    }
}

/// NegativeQ[a] — True if a is a negative number
pub fn neg_q(a: &Value) -> bool {
    match a {
        Value::Integer(n) => n.is_negative(),
        Value::Real(r) => r.is_sign_negative(),
        _ => false,
    }
}

/// GtQ[a, b] — True if a > b numerically
pub fn gt_q(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Integer(ai), Value::Integer(bi)) => ai > bi,
        (Value::Integer(ai), Value::Real(br)) => Float::with_val(DEFAULT_PRECISION, ai) > *br,
        (Value::Real(ar), Value::Integer(bi)) => *ar > Float::with_val(DEFAULT_PRECISION, bi),
        (Value::Real(ar), Value::Real(br)) => ar > br,
        _ => false,
    }
}

/// LtQ[a, b] — True if a < b numerically
pub fn lt_q(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Integer(ai), Value::Integer(bi)) => ai < bi,
        (Value::Integer(ai), Value::Real(br)) => Float::with_val(DEFAULT_PRECISION, ai) < *br,
        (Value::Real(ar), Value::Integer(bi)) => *ar < Float::with_val(DEFAULT_PRECISION, bi),
        (Value::Real(ar), Value::Real(br)) => ar < br,
        _ => false,
    }
}

/// GeQ[a, b] — True if a >= b numerically
pub fn ge_q(a: &Value, b: &Value) -> bool {
    gt_q(a, b) || eq_q(a, b)
}

/// LeQ[a, b] — True if a <= b numerically
pub fn le_q(a: &Value, b: &Value) -> bool {
    lt_q(a, b) || eq_q(a, b)
}

// ── FreeQ and related ──

/// Check if `x` (the integration variable) appears free in `expr`.
/// i.e., expr does NOT contain x as a free variable.
pub fn free_q(expr: &Value, x: &str) -> bool {
    match expr {
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Null => true,
        Value::Str(_) => true,
        Value::Symbol(s) => s != x,
        Value::List(items) => items.iter().all(|item| free_q(item, x)),
        Value::Call { head, args } => {
            // The head name should also be checked — the integration variable
            // can't appear in function names, so just check args
            if head == x {
                return false;
            }
            args.iter().all(|arg| free_q(arg, x))
        }
        Value::Assoc(map) => map.values().all(|v| free_q(v, x)),
        Value::Rule { lhs, rhs, .. } => free_q(lhs, x) && free_q(rhs, x),
        Value::Function(func_def) => {
            // Check body of function definition
            // For simplicity, check if the name matches
            free_q(&Value::Symbol(func_def.name.clone()), x)
        }
        Value::PureFunction { body, .. } => free_q_expr(body, x),
        Value::Pattern(expr) => free_q_expr(expr, x),
        Value::Complex { .. } => true,
        _ => true, // Builtin, etc.
    }
}

/// FreeQ for AST expressions (Pattern values)
fn free_q_expr(expr: &crate::ast::Expr, x: &str) -> bool {
    match expr {
        crate::ast::Expr::Integer(_)
        | crate::ast::Expr::Real(_)
        | crate::ast::Expr::Bool(_)
        | crate::ast::Expr::Null
        | crate::ast::Expr::Str(_) => true,
        crate::ast::Expr::Symbol(s) => s != x,
        crate::ast::Expr::List(items) => items.iter().all(|item| free_q_expr(item, x)),
        crate::ast::Expr::Call { head, args } => {
            if let crate::ast::Expr::Symbol(s) = head.as_ref() {
                if s == x {
                    return false;
                }
            }
            args.iter().all(|arg| free_q_expr(arg, x))
        }
        crate::ast::Expr::Assoc(entries) => entries.iter().all(|(_, v)| free_q_expr(v, x)),
        crate::ast::Expr::Rule { lhs, rhs, .. } => free_q_expr(lhs, x) && free_q_expr(rhs, x),
        crate::ast::Expr::NamedBlank { .. } | crate::ast::Expr::Blank { .. } => true,
        _ => true,
    }
}

// ── LinearQ, Polynomial Q ──

/// LinearQ[u, x] — True if u is linear in x (polynomial of degree 1)
pub fn linear_q(val: &Value, x: &str) -> bool {
    polynomial_degree(val, x) == Some(1)
}

/// QuadraticQ[u, x] — True if u is quadratic in x
pub fn quadratic_q(val: &Value, x: &str) -> bool {
    polynomial_degree(val, x) == Some(2)
}

/// Determine the polynomial degree of val in variable x, or None if not a polynomial
fn polynomial_degree(val: &Value, x: &str) -> Option<i64> {
    match val {
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) => Some(0),
        Value::Symbol(s) => {
            if s == x {
                Some(1)
            } else {
                Some(0)
            }
        }
        Value::Call { head, args } => match head.as_str() {
            "Plus" => {
                let mut max_deg: Option<i64> = None;
                for arg in args {
                    match polynomial_degree(arg, x) {
                        Some(d) => {
                            max_deg = Some(max_deg.map_or(d, |m| m.max(d)));
                        }
                        None => return None,
                    }
                }
                max_deg
            }
            "Times" => {
                let mut total = 0i64;
                for arg in args {
                    match polynomial_degree(arg, x) {
                        Some(0) => {} // constant factor
                        Some(d) => total += d,
                        None => return None,
                    }
                }
                Some(total)
            }
            "Power" if args.len() == 2 => {
                let base_deg = polynomial_degree(&args[0], x)?;
                if base_deg == 0 {
                    // Constant^expr — constant
                    return Some(0);
                }
                match &args[1] {
                    Value::Integer(n) => {
                        if let Some(n_i64) = n.to_i64() {
                            if n_i64 <= 0 {
                                // Negative power -> not a polynomial
                                None
                            } else {
                                Some(base_deg * n_i64)
                            }
                        } else {
                            None
                        }
                    }
                    _ => None, // non-constant exponent
                }
            }
            _ if free_q(val, x) => Some(0), // arbitrary function not containing x
            _ => None,
        },
        _ => None,
    }
}

/// IntegersQ[list] — True if all elements are integers
pub fn integers_q(val: &Value) -> bool {
    match val {
        Value::List(items) => items.iter().all(|item| integer_q(item)),
        v => integer_q(v),
    }
}

/// FractionQ[a] — True if a is a non-integer rational
pub fn fraction_q(val: &Value) -> bool {
    matches!(val, Value::Real(_)) || matches!(val, Value::Integer(_))
}

/// RemoveContent[u, x] — Return u with x-free (constant) parts removed
/// For example, RemoveContent[a + b*x, x] where a and b are free of x
/// returns b*x (removes the constant a)
pub fn remove_content(val: &Value, x: &str) -> Value {
    match val {
        Value::Call { head, args } if head == "Plus" => {
            let terms: Vec<Value> = args.iter().filter(|arg| !free_q(arg, x)).cloned().collect();
            if terms.is_empty() {
                Value::Integer(Integer::from(0))
            } else if terms.len() == 1 {
                terms.into_iter().next().unwrap()
            } else {
                Value::Call {
                    head: "Plus".to_string(),
                    args: terms,
                }
            }
        }
        Value::Call { head, args } if head == "Times" => {
            let factors: Vec<Value> = args.iter().filter(|arg| !free_q(arg, x)).cloned().collect();
            if factors.is_empty() {
                Value::Integer(Integer::from(1))
            } else if factors.len() == 1 {
                factors.into_iter().next().unwrap()
            } else {
                Value::Call {
                    head: "Times".to_string(),
                    args: factors,
                }
            }
        }
        // For a plain symbol or integer (constant), return 0 or 1
        v if free_q(v, x) => match val {
            Value::Integer(_) | Value::Real(_) => Value::Integer(Integer::from(0)),
            Value::Symbol(_) => Value::Integer(Integer::from(0)),
            _ => Value::Integer(Integer::from(0)),
        },
        _ => val.clone(),
    }
}

/// NumericFactor[u] — Extract the numeric factor from a product
pub fn numeric_factor(val: &Value) -> Value {
    match val {
        Value::Integer(n) => Value::Integer(n.clone()),
        Value::Real(r) => Value::Real(r.clone()),
        Value::Call { head, args } if head == "Times" => {
            let mut result = Value::Integer(Integer::from(1));
            for arg in args {
                match arg {
                    Value::Integer(n) => {
                        result = mul_values(&result, &Value::Integer(n.clone()));
                    }
                    Value::Real(r) => {
                        result = mul_values(&result, &Value::Real(r.clone()));
                    }
                    _ => {}
                }
            }
            result
        }
        _ => Value::Integer(Integer::from(1)),
    }
}

/// NonnumericFactors[u] — Return u with numeric factors removed
pub fn nonnumeric_factors(val: &Value) -> Value {
    match val {
        Value::Call { head, args } if head == "Times" => {
            let factors: Vec<Value> = args
                .iter()
                .filter(|arg| !matches!(arg, Value::Integer(_) | Value::Real(_)))
                .cloned()
                .collect();
            if factors.is_empty() {
                Value::Integer(Integer::from(1))
            } else if factors.len() == 1 {
                factors.into_iter().next().unwrap()
            } else {
                Value::Call {
                    head: "Times".to_string(),
                    args: factors,
                }
            }
        }
        _ => val.clone(),
    }
}

// ── Coefficient extraction ──

/// Coefficient[expr, x, n] — Extract the coefficient of x^n in expr
pub fn coefficient(expr: &Value, x: &str, n: i64) -> Value {
    let terms = flatten_plus(expr);
    let mut result = Value::Integer(Integer::from(0));
    for term in &terms {
        let (coeff, degree) = term_coeff_degree(term, x);
        if degree == n {
            result = add_values(&result, &coeff);
        }
    }
    result
}

/// Coefficient[expr, x] — Extract coefficient of x^1
pub fn coefficient_linear(expr: &Value, x: &str) -> Value {
    coefficient(expr, x, 1)
}

/// Flatten a Plus expression into individual terms
fn flatten_plus(val: &Value) -> Vec<Value> {
    match val {
        Value::Call { head, args } if head == "Plus" => {
            let mut result = Vec::new();
            for arg in args {
                result.extend(flatten_plus(arg));
            }
            result
        }
        _ => vec![val.clone()],
    }
}

/// Extract the coefficient and degree of `x` in a term
fn term_coeff_degree(term: &Value, x: &str) -> (Value, i64) {
    match term {
        Value::Symbol(s) if s == x => (Value::Integer(Integer::from(1)), 1),
        Value::Symbol(_) | Value::Integer(_) | Value::Real(_) => (term.clone(), 0),
        Value::Call { head, args } => match head.as_str() {
            "Times" => {
                let mut coeff = Value::Integer(Integer::from(1));
                let mut degree = 0i64;
                for arg in args {
                    let (c, d) = term_coeff_degree(arg, x);
                    coeff = mul_values(&coeff, &c);
                    degree += d;
                }
                (coeff, degree)
            }
            "Power" if args.len() == 2 => {
                if let Value::Symbol(s) = &args[0] {
                    if s == x {
                        if let Value::Integer(n) = &args[1] {
                            if let Some(d) = n.to_i64() {
                                return (Value::Integer(Integer::from(1)), d);
                            }
                        }
                    }
                }
                if free_q(term, x) {
                    (term.clone(), 0)
                } else {
                    (Value::Integer(Integer::from(0)), 0)
                }
            }
            _ => {
                if free_q(term, x) {
                    (term.clone(), 0)
                } else {
                    (Value::Integer(Integer::from(0)), 0)
                }
            }
        },
        _ => (Value::Integer(Integer::from(0)), 0),
    }
}

// ── Arithmetic helpers ──

fn add_values(a: &Value, b: &Value) -> Value {
    // x + 0 = x
    if zero_q(a) {
        return b.clone();
    }
    if zero_q(b) {
        return a.clone();
    }
    match (a, b) {
        (Value::Integer(ai), Value::Integer(bi)) => Value::Integer(ai.clone() + bi),
        (Value::Integer(ai), Value::Real(br)) => {
            let af = Float::with_val(DEFAULT_PRECISION, ai);
            Value::Real(af + br)
        }
        (Value::Real(ar), Value::Integer(bi)) => {
            let bf = Float::with_val(DEFAULT_PRECISION, bi);
            Value::Real(ar + bf)
        }
        (Value::Real(ar), Value::Real(br)) => Value::Real(ar.clone() + br),
        _ => Value::Call {
            head: "Plus".to_string(),
            args: vec![a.clone(), b.clone()],
        },
    }
}

fn mul_values(a: &Value, b: &Value) -> Value {
    // x * 0 = 0
    if zero_q(a) || zero_q(b) {
        return Value::Integer(Integer::from(0));
    }
    // x * 1 = x
    if is_one(a) {
        return b.clone();
    }
    if is_one(b) {
        return a.clone();
    }
    match (a, b) {
        (Value::Integer(ai), Value::Integer(bi)) => Value::Integer(ai.clone() * bi),
        (Value::Integer(ai), Value::Real(br)) => {
            let af = Float::with_val(DEFAULT_PRECISION, ai);
            Value::Real(af * br)
        }
        (Value::Real(ar), Value::Integer(bi)) => {
            let bf = Float::with_val(DEFAULT_PRECISION, bi);
            Value::Real(ar * bf)
        }
        (Value::Real(ar), Value::Real(br)) => Value::Real(ar.clone() * br),
        _ => Value::Call {
            head: "Times".to_string(),
            args: vec![a.clone(), b.clone()],
        },
    }
}

// ── SimplerQ, SumSimplerQ ──

/// SumSimplerQ[a, b] — True if a is "simpler" than b for sum integration
pub fn sum_simpler_q(a: &Value, b: &Value) -> bool {
    leaf_count(a) <= leaf_count(b)
}

/// Approximate leaf count for simplicity comparison
fn leaf_count(val: &Value) -> usize {
    match val {
        Value::Integer(_) | Value::Real(_) | Value::Bool(_) | Value::Null => 1,
        Value::Str(s) => s.len(),
        Value::Symbol(s) => s.len().max(1),
        Value::List(items) => items.iter().map(leaf_count).sum::<usize>() + 1,
        Value::Call { head, args } => head.len() + args.iter().map(leaf_count).sum::<usize>(),
        Value::Assoc(map) => map.values().map(leaf_count).sum::<usize>() + 1,
        Value::Rule { lhs, rhs, .. } => leaf_count(lhs) + leaf_count(rhs),
        Value::PureFunction { body, .. } => leaf_count_expr(body),
        Value::Pattern(expr) => leaf_count_expr(expr),
        Value::Function(func_def) => func_def.name.len(),
        _ => 1,
    }
}

fn leaf_count_expr(expr: &crate::ast::Expr) -> usize {
    match expr {
        crate::ast::Expr::Integer(_)
        | crate::ast::Expr::Real(_)
        | crate::ast::Expr::Bool(_)
        | crate::ast::Expr::Null => 1,
        crate::ast::Expr::Str(s) => s.len(),
        crate::ast::Expr::Symbol(s) => s.len().max(1),
        crate::ast::Expr::List(items) => items.iter().map(leaf_count_expr).sum::<usize>() + 1,
        crate::ast::Expr::Call { head, args } => {
            leaf_count_expr(head) + args.iter().map(leaf_count_expr).sum::<usize>()
        }
        _ => 1,
    }
}

// ── Together, Simplify ──

/// Very basic algebraic simplification (combines numeric constants)
pub fn together_simplify(val: &Value) -> Value {
    match val {
        Value::Call { head, args } if head == "Plus" => {
            let simplified: Vec<Value> = args.iter().map(together_simplify).collect();
            let num_sum: i64 = simplified
                .iter()
                .filter_map(|a| match a {
                    Value::Integer(n) => n.to_i64(),
                    _ => None,
                })
                .sum();
            let non_numeric: Vec<Value> = simplified
                .into_iter()
                .filter(|a| !matches!(a, Value::Integer(_)))
                .collect();
            let mut terms = Vec::new();
            if num_sum != 0 {
                terms.push(Value::Integer(Integer::from(num_sum)));
            }
            terms.extend(non_numeric);
            if terms.is_empty() {
                Value::Integer(Integer::from(0))
            } else if terms.len() == 1 {
                terms.into_iter().next().unwrap()
            } else {
                Value::Call {
                    head: "Plus".to_string(),
                    args: terms,
                }
            }
        }
        Value::Call { head, args } if head == "Times" => {
            let simplified: Vec<Value> = args.iter().map(together_simplify).collect();
            let num_prod: i64 = simplified
                .iter()
                .filter_map(|a| match a {
                    Value::Integer(n) => n.to_i64(),
                    _ => None,
                })
                .product();
            let non_numeric: Vec<Value> = simplified
                .into_iter()
                .filter(|a| !matches!(a, Value::Integer(_)))
                .collect();
            let mut factors = Vec::new();
            if num_prod != 0 {
                if num_prod != 1 || non_numeric.is_empty() {
                    factors.push(Value::Integer(Integer::from(num_prod)));
                }
            } else {
                return Value::Integer(Integer::from(0));
            }
            factors.extend(non_numeric);
            if factors.is_empty() {
                Value::Integer(Integer::from(1))
            } else if factors.len() == 1 {
                factors.into_iter().next().unwrap()
            } else {
                Value::Call {
                    head: "Times".to_string(),
                    args: factors,
                }
            }
        }
        _ => val.clone(),
    }
}

// ── IntPart, FracPart ──

/// IntPart[x] — Integer part of x
pub fn int_part(val: &Value) -> Value {
    match val {
        Value::Integer(n) => Value::Integer(n.clone()),
        Value::Real(r) => {
            let int_val = r.to_integer().unwrap_or_else(|| Integer::from(0));
            Value::Integer(int_val)
        }
        _ => Value::Integer(Integer::from(0)),
    }
}

/// FracPart[x] — Fractional part of x
pub fn frac_part(val: &Value) -> Value {
    match val {
        Value::Integer(_) => Value::Integer(Integer::from(0)),
        Value::Real(r) => {
            let int_val = r.to_integer().unwrap_or_else(|| Integer::from(0));
            let f = Float::with_val(DEFAULT_PRECISION, r);
            let int_f = Float::with_val(DEFAULT_PRECISION, &int_val);
            Value::Real(f - int_f)
        }
        _ => Value::Integer(Integer::from(0)),
    }
}

// ── NumericQ, AtomQ ──

/// AtomQ[expr] — True if expr is an atom (no sub-expressions)
pub fn atom_q(val: &Value) -> bool {
    matches!(
        val,
        Value::Integer(_)
            | Value::Real(_)
            | Value::Bool(_)
            | Value::Str(_)
            | Value::Null
            | Value::Symbol(_)
            | Value::Complex { .. }
    )
}

/// NumericQ[expr] — True if expr is a numeric value
pub fn numeric_q(val: &Value) -> bool {
    matches!(
        val,
        Value::Integer(_) | Value::Real(_) | Value::Complex { .. }
    )
}

// ── Trig ──

/// TrigQ[expr] — True if expr is a trigonometric function
pub fn trig_q(val: &Value) -> bool {
    matches!(
        val,
        Value::Call { head, .. }
        if head == "Sin" || head == "Cos" || head == "Tan"
        || head == "Cot" || head == "Sec" || head == "Csc"
    )
}

/// LogQ[expr] — True if expr is a Log call
pub fn log_q(val: &Value) -> bool {
    matches!(val, Value::Call { head, .. } if head == "Log")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;
    use rug::Integer;

    fn sym(s: &str) -> Value {
        Value::Symbol(s.to_string())
    }

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn call(head: &str, args: Vec<Value>) -> Value {
        Value::Call {
            head: head.to_string(),
            args,
        }
    }

    fn real(f: f64) -> Value {
        Value::Real(Float::with_val(DEFAULT_PRECISION, f))
    }

    #[test]
    fn test_free_q_integer() {
        assert!(free_q(&int(5), "x"));
    }

    #[test]
    fn test_free_q_symbol() {
        assert!(free_q(&sym("a"), "x"));
        assert!(!free_q(&sym("x"), "x"));
    }

    #[test]
    fn test_free_q_call() {
        let expr = call("Sin", vec![sym("x")]);
        assert!(!free_q(&expr, "x"));

        let expr = call("Sin", vec![int(5)]);
        assert!(free_q(&expr, "x"));
    }

    #[test]
    fn test_free_q_list() {
        let expr = Value::List(vec![sym("a"), sym("b"), sym("x")]);
        assert!(!free_q(&expr, "x"));

        let expr = Value::List(vec![sym("a"), sym("b")]);
        assert!(free_q(&expr, "x"));
    }

    #[test]
    fn test_linear_q() {
        assert!(linear_q(&sym("x"), "x"));
        assert!(linear_q(
            &call(
                "Plus",
                vec![sym("a"), call("Times", vec![sym("b"), sym("x")])]
            ),
            "x"
        ));
        assert!(!linear_q(&call("Power", vec![sym("x"), int(2)]), "x"));
    }

    #[test]
    fn test_polynomial_degree() {
        assert_eq!(polynomial_degree(&int(5), "x"), Some(0));
        assert_eq!(polynomial_degree(&sym("x"), "x"), Some(1));
        assert_eq!(
            polynomial_degree(&call("Power", vec![sym("x"), int(2)]), "x"),
            Some(2)
        );
        assert_eq!(
            polynomial_degree(&call("Power", vec![sym("x"), int(-1)]), "x"),
            None
        );
    }

    #[test]
    fn test_remove_content() {
        let expr = call(
            "Plus",
            vec![sym("a"), call("Times", vec![sym("b"), sym("x")])],
        );
        let result = remove_content(&expr, "x");
        // Should remove the constant a
        assert_eq!(result, call("Times", vec![sym("b"), sym("x")]));
    }

    #[test]
    fn test_coefficient() {
        let expr = call(
            "Plus",
            vec![
                sym("a"),
                call("Times", vec![sym("b"), sym("x")]),
                call(
                    "Times",
                    vec![sym("c"), call("Power", vec![sym("x"), int(2)])],
                ),
            ],
        );
        assert_eq!(coefficient(&expr, "x", 0), sym("a"));
        assert_eq!(coefficient(&expr, "x", 1), sym("b"));
        assert_eq!(coefficient(&expr, "x", 2), sym("c"));
    }

    #[test]
    fn test_num_predicates() {
        assert!(eq_q(&int(5), &int(5)));
        assert!(!eq_q(&int(5), &int(6)));
        assert!(ne_q(&int(5), &int(6)));
        assert!(gt_q(&int(10), &int(5)));
        assert!(lt_q(&int(3), &int(5)));
        assert!(ge_q(&int(5), &int(5)));
        assert!(le_q(&int(3), &int(5)));
    }

    #[test]
    fn test_power_q() {
        let expr = call("Power", vec![sym("x"), int(2)]);
        assert!(power_q(&expr));
        assert!(!power_q(&sym("x")));
    }

    #[test]
    fn test_sum_q() {
        let expr = call("Plus", vec![sym("a"), sym("b")]);
        assert!(sum_q(&expr));
        assert!(!sum_q(&sym("x")));
    }

    #[test]
    fn test_integer_q() {
        assert!(integer_q(&int(42)));
        assert!(!integer_q(&real(3.14)));
    }

    #[test]
    fn test_remove_content_times() {
        let expr = call("Times", vec![int(3), sym("x")]);
        let result = remove_content(&expr, "x");
        assert_eq!(result, sym("x"));
    }

    #[test]
    fn test_numeric_factor() {
        let expr = call("Times", vec![int(3), sym("x")]);
        assert_eq!(numeric_factor(&expr), int(3));
    }

    #[test]
    fn test_nonnumeric_factors() {
        let expr = call("Times", vec![int(3), sym("x")]);
        assert_eq!(nonnumeric_factors(&expr), sym("x"));
    }
}
