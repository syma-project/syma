/// Rubi integration engine.
///
/// The engine loads parsed Rubi rules (from `RuleDatabase`), tries them
/// in order against the integrand, and returns the result.
use std::collections::HashMap;

use rug::Float;
use rug::Integer;

use crate::rubi::helpers;
use crate::rubi::wl_ast::{BinOp, IntRule, RuleFile, UnaryOp, WLExpr};
use crate::value::{DEFAULT_PRECISION, EvalError, Value};

/// Variable bindings produced by pattern matching.
pub type Bindings = HashMap<String, Value>;

/// The Rubi integration engine.
pub struct RubiEngine {
    /// Loaded rules in order
    rules: Vec<IntRule>,
    /// Whether rules have been loaded
    loaded: bool,
}

impl RubiEngine {
    pub fn new() -> Self {
        RubiEngine {
            rules: Vec::new(),
            loaded: false,
        }
    }

    /// Load rules from a list of parsed rule files.
    pub fn load_rules(&mut self, files: Vec<RuleFile>) {
        let mut start_index = self.rules.len();
        for file in files {
            for mut rule in file.rules {
                rule.index = start_index;
                start_index += 1;
                self.rules.push(rule);
            }
        }
        self.loaded = true;
    }

    /// Check if rules have been loaded.
    pub fn is_loaded(&self) -> bool {
        self.loaded
    }

    /// Compute the integral of `expr` with respect to variable `var`.
    ///
    /// Tries loaded rules in order. If no rule matches, returns an
    /// unevaluated `Integrate[expr, var]` call.
    pub fn integrate(&mut self, expr: &Value, var: &str) -> Result<Value, EvalError> {
        // First try loaded rules
        // Use index-based loop; clone rule before eval_result to avoid borrow conflict
        for i in 0..self.rules.len() {
            let bindings = self.match_pattern(&self.rules[i].pattern, expr, var);
            if let Some(b) = bindings {
                // Check condition
                if let Some(ref cond) = self.rules[i].condition {
                    if !self.eval_condition(cond, &b, var) {
                        continue;
                    }
                }
                // Clone the result before the mutable borrow
                let result = self.rules[i].result.clone();
                return self.eval_result(&result, &b, var);
            }
        }

        // No rule matched. Fall back to the unevaluated form.
        Ok(Value::Call {
            head: "Integrate".to_string(),
            args: vec![expr.clone(), Value::Symbol(var.to_string())],
        })
    }

    // ── Pattern Matching ──

    /// Match a WLExpr pattern against a Syma Value.
    /// Returns Some(bindings) on success, None on failure.
    fn match_pattern(&self, pattern: &WLExpr, value: &Value, var: &str) -> Option<Bindings> {
        match pattern {
            // Wildcard: _ matches anything
            WLExpr::Blank => Some(HashMap::new()),

            // Typed blank: _Integer, _Symbol, _Real
            WLExpr::BlankType(type_name) => {
                if value.matches_type(type_name) {
                    Some(HashMap::new())
                } else {
                    None
                }
            }

            // Named blank: x_ matches anything, binds to x
            WLExpr::NamedBlank(name) => {
                let mut bindings = HashMap::new();
                bindings.insert(name.clone(), value.clone());
                Some(bindings)
            }

            // Typed named blank: x_Integer, x_Symbol
            WLExpr::NamedBlankType(name, type_name) => {
                if value.matches_type(type_name) {
                    let mut bindings = HashMap::new();
                    bindings.insert(name.clone(), value.clone());
                    Some(bindings)
                } else {
                    None
                }
            }

            // Optional: x_.  matches x or nothing (defaults)
            WLExpr::Optional(name) => {
                let mut bindings = HashMap::new();
                bindings.insert(name.clone(), value.clone());
                Some(bindings)
            }

            // PatternSequence: x__ (one or more), x___ (zero or more)
            WLExpr::PatternSequence(_, _) => {
                let mut bindings = HashMap::new();
                // For now, treat as matching anything
                bindings.insert("__seq__".to_string(), value.clone());
                Some(bindings)
            }

            // Literal symbol
            WLExpr::Symbol(s) => {
                if let Value::Symbol(v) = value {
                    if s == v { Some(HashMap::new()) } else { None }
                } else {
                    None
                }
            }

            // Integer literal
            WLExpr::Integer(n) => {
                if let Value::Integer(v) = value {
                    if let Some(vi) = v.to_i64() {
                        if *n == vi {
                            return Some(HashMap::new());
                        }
                    }
                }
                None
            }

            // Real literal
            WLExpr::Real(n) => {
                if let Value::Real(v) = value {
                    let target = Float::with_val(DEFAULT_PRECISION, *n);
                    if (v.clone() - &target).abs() < 1e-10 {
                        return Some(HashMap::new());
                    }
                }
                None
            }

            // List: {a, b, c}
            WLExpr::List(items) => {
                if let Value::List(vals) = value {
                    if items.len() != vals.len() {
                        return None;
                    }
                    let mut all_bindings = HashMap::new();
                    for (pat, val) in items.iter().zip(vals.iter()) {
                        match self.match_pattern(pat, val, var) {
                            Some(b) => all_bindings.extend(b),
                            None => return None,
                        }
                    }
                    Some(all_bindings)
                } else {
                    None
                }
            }

            // Call: head[arg1, arg2, ...]
            WLExpr::Call { head, args } => {
                if let Value::Call {
                    head: val_head,
                    args: val_args,
                } = value
                {
                    // Match the head
                    let head_bindings =
                        self.match_pattern(head, &Value::Symbol(val_head.clone()), var)?;

                    // Match arguments
                    if args.len() != val_args.len() {
                        return None;
                    }
                    let mut all_bindings = head_bindings;
                    for (pat, val) in args.iter().zip(val_args.iter()) {
                        match self.match_pattern(pat, val, var) {
                            Some(b) => all_bindings.extend(b),
                            None => return None,
                        }
                    }
                    Some(all_bindings)
                } else {
                    None
                }
            }

            // Binary operations
            WLExpr::BinaryOp { op, lhs, rhs } => {
                // Convert the binary operation to a Call and match
                let call_pattern = self.binop_to_call(op, lhs, rhs);
                self.match_pattern(&call_pattern, value, var)
            }

            // Unary operations
            WLExpr::UnaryOp { op, expr } => {
                let call_pattern = self.unaryop_to_call(op, expr);
                self.match_pattern(&call_pattern, value, var)
            }

            // Rule: lhs -> rhs
            WLExpr::Rule {
                lhs: pat_lhs,
                rhs: pat_rhs,
                ..
            } => {
                // Match the rule structure
                if let Value::Rule {
                    lhs: val_lhs,
                    rhs: val_rhs,
                    ..
                } = value
                {
                    let lhs_b = self.match_pattern(pat_lhs, val_lhs, var)?;
                    let rhs_b = self.match_pattern(pat_rhs, val_rhs, var)?;
                    let mut all = lhs_b;
                    all.extend(rhs_b);
                    Some(all)
                } else {
                    None
                }
            }

            // With, Condition, Slot, Hold — not matched directly
            _ => None,
        }
    }

    // ── Condition Evaluation ──

    /// Evaluate a condition expression given bindings.
    fn eval_condition(&self, cond: &WLExpr, bindings: &Bindings, var: &str) -> bool {
        match cond {
            // `&&`
            WLExpr::BinaryOp {
                op: BinOp::And,
                lhs,
                rhs,
            } => self.eval_condition(lhs, bindings, var) && self.eval_condition(rhs, bindings, var),

            // FreeQ[expr, x]
            WLExpr::Call { head, args } if is_symbol(head, "FreeQ") && args.len() == 2 => {
                let expr_val = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let x_val = self.wl_to_value_with_bindings(&args[1], bindings, var);
                let x_str = value_to_symbol_name(&x_val).unwrap_or_else(|| var.to_string());
                helpers::free_q(&expr_val, &x_str)
            }

            // NeQ[a, b]
            WLExpr::Call { head, args } if is_symbol(head, "NeQ") && args.len() == 2 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let b = self.wl_to_value_with_bindings(&args[1], bindings, var);
                helpers::ne_q(&a, &b)
            }

            // EqQ[a, b]
            WLExpr::Call { head, args } if is_symbol(head, "EqQ") && args.len() == 2 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let b = self.wl_to_value_with_bindings(&args[1], bindings, var);
                helpers::eq_q(&a, &b)
            }

            // GtQ[a, b]
            WLExpr::Call { head, args } if is_symbol(head, "GtQ") && args.len() == 2 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let b = self.wl_to_value_with_bindings(&args[1], bindings, var);
                helpers::gt_q(&a, &b)
            }

            // LtQ[a, b]
            WLExpr::Call { head, args } if is_symbol(head, "LtQ") && args.len() == 2 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let b = self.wl_to_value_with_bindings(&args[1], bindings, var);
                helpers::lt_q(&a, &b)
            }

            // GeQ[a, b]
            WLExpr::Call { head, args } if is_symbol(head, "GeQ") && args.len() == 2 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let b = self.wl_to_value_with_bindings(&args[1], bindings, var);
                helpers::ge_q(&a, &b)
            }

            // LeQ[a, b]
            WLExpr::Call { head, args } if is_symbol(head, "LeQ") && args.len() == 2 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let b = self.wl_to_value_with_bindings(&args[1], bindings, var);
                helpers::le_q(&a, &b)
            }

            // IGtQ[a, n] — Integer > n
            WLExpr::Call { head, args } if is_symbol(head, "IGtQ") && args.len() == 2 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let n = self.wl_to_value_with_bindings(&args[1], bindings, var);
                if let (Some(ai), Some(ni)) = (a.to_integer(), n.to_integer()) {
                    ai > ni
                } else {
                    false
                }
            }

            // ILtQ[a, n]
            WLExpr::Call { head, args } if is_symbol(head, "ILtQ") && args.len() == 2 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let n = self.wl_to_value_with_bindings(&args[1], bindings, var);
                if let (Some(ai), Some(ni)) = (a.to_integer(), n.to_integer()) {
                    ai < ni
                } else {
                    false
                }
            }

            // IGeQ[a, n]
            WLExpr::Call { head, args } if is_symbol(head, "IGeQ") && args.len() == 2 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let n = self.wl_to_value_with_bindings(&args[1], bindings, var);
                if let (Some(ai), Some(ni)) = (a.to_integer(), n.to_integer()) {
                    ai >= ni
                } else {
                    false
                }
            }

            // ILeQ[a, n]
            WLExpr::Call { head, args } if is_symbol(head, "ILeQ") && args.len() == 2 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let n = self.wl_to_value_with_bindings(&args[1], bindings, var);
                if let (Some(ai), Some(ni)) = (a.to_integer(), n.to_integer()) {
                    ai <= ni
                } else {
                    false
                }
            }

            // IntegerQ[a]
            WLExpr::Call { head, args } if is_symbol(head, "IntegerQ") && args.len() == 1 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                helpers::integer_q(&a)
            }

            // RationalQ[a]
            WLExpr::Call { head, args } if is_symbol(head, "RationalQ") && args.len() == 1 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                helpers::rational_q(&a)
            }

            // IntegersQ[a]
            WLExpr::Call { head, args } if is_symbol(head, "IntegersQ") && args.len() == 1 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                helpers::integers_q(&a)
            }

            // PosQ[a]
            WLExpr::Call { head, args } if is_symbol(head, "PosQ") && args.len() == 1 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                helpers::pos_q(&a)
            }

            // NegQ[a]
            WLExpr::Call { head, args } if is_symbol(head, "NegQ") && args.len() == 1 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                helpers::neg_q(&a)
            }

            // LinearQ[u, x]
            WLExpr::Call { head, args } if is_symbol(head, "LinearQ") && args.len() == 2 => {
                let u = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let x = self.wl_to_value_with_bindings(&args[1], bindings, var);
                let x_str = value_to_symbol_name(&x).unwrap_or_else(|| var.to_string());
                helpers::linear_q(&u, &x_str)
            }

            // PowerQ[u]
            WLExpr::Call { head, args } if is_symbol(head, "PowerQ") && args.len() == 1 => {
                let u = self.wl_to_value_with_bindings(&args[0], bindings, var);
                helpers::power_q(&u)
            }

            // SumQ[u]
            WLExpr::Call { head, args } if is_symbol(head, "SumQ") && args.len() == 1 => {
                let u = self.wl_to_value_with_bindings(&args[0], bindings, var);
                helpers::sum_q(&u)
            }

            // NonsumQ[u]
            WLExpr::Call { head, args } if is_symbol(head, "NonsumQ") && args.len() == 1 => {
                let u = self.wl_to_value_with_bindings(&args[0], bindings, var);
                helpers::nonsum_q(&u)
            }

            // ProductQ[u]
            WLExpr::Call { head, args } if is_symbol(head, "ProductQ") && args.len() == 1 => {
                let u = self.wl_to_value_with_bindings(&args[0], bindings, var);
                helpers::product_q(&u)
            }

            // IntegerPowerQ[u]
            WLExpr::Call { head, args } if is_symbol(head, "IntegerPowerQ") && args.len() == 1 => {
                let u = self.wl_to_value_with_bindings(&args[0], bindings, var);
                matches!(&u, Value::Call { head: h, args: a } if h == "Power" && a.len() == 2 && matches!(&a[1], Value::Integer(_)))
            }

            // Not[...]
            WLExpr::UnaryOp {
                op: UnaryOp::Not,
                expr,
            } => !self.eval_condition(expr, bindings, var),

            // Not as a Call
            WLExpr::Call { head, args } if is_symbol(head, "Not") && args.len() == 1 => {
                !self.eval_condition(&args[0], bindings, var)
            }

            // AtomQ[a]
            WLExpr::Call { head, args } if is_symbol(head, "AtomQ") && args.len() == 1 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                helpers::atom_q(&a)
            }

            // NumericQ[a]
            WLExpr::Call { head, args } if is_symbol(head, "NumericQ") && args.len() == 1 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                helpers::numeric_q(&a)
            }

            // FractionQ[a]
            WLExpr::Call { head, args } if is_symbol(head, "FractionQ") && args.len() == 1 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                helpers::fraction_q(&a)
            }

            // SymbolQ[a]
            WLExpr::Call { head, args } if is_symbol(head, "SymbolQ") && args.len() == 1 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                helpers::symbol_q(&a)
            }

            // QuadraticQ[u, x]
            WLExpr::Call { head, args } if is_symbol(head, "QuadraticQ") && args.len() == 2 => {
                let u = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let x = self.wl_to_value_with_bindings(&args[1], bindings, var);
                let x_str = value_to_symbol_name(&x).unwrap_or_else(|| var.to_string());
                helpers::quadratic_q(&u, &x_str)
            }

            // SumSimplerQ[a, b]
            WLExpr::Call { head, args } if is_symbol(head, "SumSimplerQ") && args.len() == 2 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let b = self.wl_to_value_with_bindings(&args[1], bindings, var);
                helpers::sum_simpler_q(&a, &b)
            }

            // Unevaluated predicate — known false
            _ => false,
        }
    }

    // ── Result Evaluation ──

    /// Evaluate a Wolfram result expression with bindings, producing a Syma Value.
    fn eval_result(
        &mut self,
        expr: &WLExpr,
        bindings: &Bindings,
        var: &str,
    ) -> Result<Value, EvalError> {
        match expr {
            WLExpr::Symbol(s) => {
                // Check if it's a bound variable
                if let Some(val) = bindings.get(s) {
                    Ok(val.clone())
                } else {
                    Ok(Value::Symbol(s.clone()))
                }
            }
            WLExpr::Integer(n) => Ok(Value::Integer(Integer::from(*n))),
            WLExpr::Real(n) => Ok(Value::Real(Float::with_val(DEFAULT_PRECISION, *n))),
            WLExpr::Str(s) => Ok(Value::Str(s.clone())),
            WLExpr::List(items) => {
                let vals: Result<Vec<Value>, _> = items
                    .iter()
                    .map(|item| self.eval_result(item, bindings, var))
                    .collect();
                Ok(Value::List(vals?))
            }

            WLExpr::Call { head, args } => {
                let head_str = if let WLExpr::Symbol(s) = head.as_ref() {
                    s.clone()
                } else {
                    // Evaluate the head expression
                    let h = self.eval_result(head, bindings, var)?;
                    value_to_symbol_name(&h).unwrap_or_default()
                };

                // Special case: Int[subexpr, x] — recursive integration
                if head_str == "Int" && args.len() == 2 {
                    let subexpr = self.eval_result(&args[0], bindings, var)?;
                    let x_val = self.eval_result(&args[1], bindings, var)?;
                    let x_str = value_to_symbol_name(&x_val).unwrap_or_else(|| var.to_string());
                    // Recursively integrate
                    return self.integrate(&subexpr, &x_str);
                }

                // Special case: Subst[expr, x, replacement] — substitution
                if head_str == "Subst" && args.len() >= 2 {
                    return self.eval_subst(&args, bindings, var);
                }

                // Special case: With[{bindings}, body]
                if head_str == "With" && args.len() >= 1 {
                    return self.eval_with(&args, bindings, var);
                }

                // Special case: FreeQ, NeQ, etc. — condition predicates used in result
                if let Some(result) = self.eval_predicate_in_result(&head_str, &args, bindings, var)
                {
                    return Ok(result);
                }

                // Evaluate arguments
                let arg_vals: Result<Vec<Value>, _> = args
                    .iter()
                    .map(|arg| self.eval_result(arg, bindings, var))
                    .collect();
                let arg_vals = arg_vals?;

                // Try to use Syma's builtins for known functions
                let result = Value::Call {
                    head: head_str,
                    args: arg_vals,
                };

                Ok(result)
            }

            WLExpr::BinaryOp { op, lhs, rhs } => {
                let lv = self.eval_result(lhs, bindings, var)?;
                let rv = self.eval_result(rhs, bindings, var)?;
                self.eval_binary_op(*op, &lv, &rv)
            }

            WLExpr::UnaryOp { op, expr } => {
                let val = self.eval_result(expr, bindings, var)?;
                match op {
                    UnaryOp::Neg => Ok(Value::Call {
                        head: "Times".to_string(),
                        args: vec![Value::Integer(Integer::from(-1)), val],
                    }),
                    UnaryOp::Not => Ok(Value::Call {
                        head: "Not".to_string(),
                        args: vec![val],
                    }),
                }
            }

            WLExpr::Condition { expr: e, .. } => {
                // Strip the condition wrapper, just evaluate the expression
                self.eval_result(e, bindings, var)
            }

            WLExpr::With {
                bindings: wl_bindings,
                body,
            } => self.eval_with_expr(wl_bindings, body, bindings, var),

            WLExpr::Rule { lhs, rhs, delayed } => {
                let lv = self.eval_result(lhs, bindings, var)?;
                let rv = self.eval_result(rhs, bindings, var)?;
                Ok(Value::Rule {
                    lhs: Box::new(lv),
                    rhs: Box::new(rv),
                    delayed: *delayed,
                })
            }

            _ => Ok(Value::Null),
        }
    }

    // ── Subst Evaluation ──

    /// Evaluate Subst[expr, x, replacement] — substitution
    fn eval_subst(
        &mut self,
        args: &[WLExpr],
        bindings: &Bindings,
        var: &str,
    ) -> Result<Value, EvalError> {
        // Subst[expr, x, replacement]
        // Replace x with replacement in expr
        if args.len() == 3 {
            let expr = self.eval_result(&args[0], bindings, var)?;
            let x_val = self.wl_to_value_with_bindings(&args[1], bindings, var);
            let repl = self.wl_to_value_with_bindings(&args[2], bindings, var);
            let x_str = value_to_symbol_name(&x_val).unwrap_or_else(|| var.to_string());

            Ok(self.substitute_in_value(&expr, &x_str, &repl))
        }
        // Subst[expr, x] — just substitute the integration variable
        else if args.len() == 2 {
            let expr = self.eval_result(&args[0], bindings, var)?;
            let x_val = self.wl_to_value_with_bindings(&args[1], bindings, var);
            let x_str = value_to_symbol_name(&x_val).unwrap_or_else(|| var.to_string());
            let var_sym = Value::Symbol(var.to_string());

            Ok(self.substitute_in_value(&expr, &x_str, &var_sym))
        } else {
            Ok(Value::Call {
                head: "Subst".to_string(),
                args: vec![],
            })
        }
    }

    /// Substitute `from` with `to` recursively in a Value.
    fn substitute_in_value(&self, val: &Value, from: &str, to: &Value) -> Value {
        match val {
            Value::Symbol(s) if s == from => to.clone(),
            Value::Symbol(_)
            | Value::Integer(_)
            | Value::Real(_)
            | Value::Bool(_)
            | Value::Str(_)
            | Value::Null => val.clone(),
            Value::List(items) => Value::List(
                items
                    .iter()
                    .map(|item| self.substitute_in_value(item, from, to))
                    .collect(),
            ),
            Value::Call { head, args } => {
                let new_args: Vec<Value> = args
                    .iter()
                    .map(|arg| self.substitute_in_value(arg, from, to))
                    .collect();
                Value::Call {
                    head: head.clone(),
                    args: new_args,
                }
            }
            Value::Assoc(map) => {
                let new_map: std::collections::HashMap<_, _> = map
                    .iter()
                    .map(|(k, v)| (k.clone(), self.substitute_in_value(v, from, to)))
                    .collect();
                Value::Assoc(new_map)
            }
            Value::Rule { lhs, rhs, delayed } => Value::Rule {
                lhs: Box::new(self.substitute_in_value(lhs, from, to)),
                rhs: Box::new(self.substitute_in_value(rhs, from, to)),
                delayed: *delayed,
            },
            Value::Complex { re, im } => Value::Complex { re: *re, im: *im },
            Value::Pattern(expr) => {
                let new_expr = self.substitute_in_expr(expr, from, to);
                Value::Pattern(new_expr)
            }
            _ => val.clone(),
        }
    }

    fn substitute_in_expr(
        &self,
        expr: &crate::ast::Expr,
        from: &str,
        to: &Value,
    ) -> crate::ast::Expr {
        match expr {
            crate::ast::Expr::Symbol(s) if s == from => {
                // Convert Value back to Expr (limited)
                value_to_expr(to)
            }
            crate::ast::Expr::Symbol(_)
            | crate::ast::Expr::Integer(_)
            | crate::ast::Expr::Real(_)
            | crate::ast::Expr::Bool(_)
            | crate::ast::Expr::Str(_)
            | crate::ast::Expr::Null => expr.clone(),
            crate::ast::Expr::List(items) => crate::ast::Expr::List(
                items
                    .iter()
                    .map(|item| self.substitute_in_expr(item, from, to))
                    .collect(),
            ),
            crate::ast::Expr::Call { head, args } => crate::ast::Expr::Call {
                head: Box::new(self.substitute_in_expr(head, from, to)),
                args: args
                    .iter()
                    .map(|arg| self.substitute_in_expr(arg, from, to))
                    .collect(),
            },
            _ => expr.clone(),
        }
    }

    // ── With Evaluation ──

    /// Evaluate With[{bindings}, body]
    fn eval_with(
        &mut self,
        args: &[WLExpr],
        bindings: &Bindings,
        var: &str,
    ) -> Result<Value, EvalError> {
        match &args[0] {
            WLExpr::List(items) => {
                let mut local_bindings: Vec<(String, WLExpr)> = Vec::new();
                for item in items {
                    match item {
                        WLExpr::Rule { lhs, rhs, .. } => {
                            if let WLExpr::Symbol(name) = lhs.as_ref() {
                                local_bindings.push((name.clone(), *rhs.clone()));
                            }
                        }
                        _ => {}
                    }
                }
                let body = &args[1];
                self.eval_with_expr(&local_bindings, body, bindings, var)
            }
            _ => self.eval_result(&args[0], bindings, var),
        }
    }

    fn eval_with_expr(
        &mut self,
        wl_bindings: &[(String, WLExpr)],
        body: &WLExpr,
        bindings: &Bindings,
        var: &str,
    ) -> Result<Value, EvalError> {
        // Evaluate the with bindings
        let mut extended_bindings = bindings.clone();
        for (name, wl_expr) in wl_bindings {
            let val = self.eval_result(wl_expr, &extended_bindings, var)?;
            extended_bindings.insert(name.clone(), val);
        }
        self.eval_result(body, &extended_bindings, var)
    }

    // ── Helpers ──

    /// Convert a WLExpr + bindings to a Syma Value.
    fn wl_to_value_with_bindings(&self, expr: &WLExpr, bindings: &Bindings, var: &str) -> Value {
        match expr {
            WLExpr::Symbol(s) => bindings.get(s).cloned().unwrap_or(Value::Symbol(s.clone())),
            WLExpr::Integer(n) => Value::Integer(Integer::from(*n)),
            WLExpr::Real(n) => Value::Real(Float::with_val(DEFAULT_PRECISION, *n)),
            WLExpr::Str(s) => Value::Str(s.clone()),
            WLExpr::List(items) => Value::List(
                items
                    .iter()
                    .map(|i| self.wl_to_value_with_bindings(i, bindings, var))
                    .collect(),
            ),
            WLExpr::Call { head, args } => {
                let head_val = self.wl_to_value_with_bindings(head, bindings, var);
                let head_str = value_to_symbol_name(&head_val).unwrap_or_default();
                let arg_vals: Vec<Value> = args
                    .iter()
                    .map(|a| self.wl_to_value_with_bindings(a, bindings, var))
                    .collect();
                Value::Call {
                    head: head_str,
                    args: arg_vals,
                }
            }
            WLExpr::BinaryOp { op, lhs, rhs } => {
                let lv = self.wl_to_value_with_bindings(lhs, bindings, var);
                let rv = self.wl_to_value_with_bindings(rhs, bindings, var);
                self.eval_binary_op(*op, &lv, &rv).unwrap_or(Value::Null)
            }
            WLExpr::NamedBlank(name) => bindings.get(name).cloned().unwrap_or(Value::Null),
            WLExpr::Blank => Value::Null,
            _ => Value::Null,
        }
    }

    /// Evaluate a binary operation on Syma Values.
    fn eval_binary_op(&self, op: BinOp, lhs: &Value, rhs: &Value) -> Result<Value, EvalError> {
        match op {
            BinOp::Plus => Ok(Value::Call {
                head: "Plus".to_string(),
                args: vec![lhs.clone(), rhs.clone()],
            }),
            BinOp::Minus => Ok(Value::Call {
                head: "Plus".to_string(),
                args: vec![
                    lhs.clone(),
                    Value::Call {
                        head: "Times".to_string(),
                        args: vec![Value::Integer(Integer::from(-1)), rhs.clone()],
                    },
                ],
            }),
            BinOp::Times => Ok(Value::Call {
                head: "Times".to_string(),
                args: vec![lhs.clone(), rhs.clone()],
            }),
            BinOp::Divide => Ok(Value::Call {
                head: "Times".to_string(),
                args: vec![
                    lhs.clone(),
                    Value::Call {
                        head: "Power".to_string(),
                        args: vec![rhs.clone(), Value::Integer(Integer::from(-1))],
                    },
                ],
            }),
            BinOp::Power => Ok(Value::Call {
                head: "Power".to_string(),
                args: vec![lhs.clone(), rhs.clone()],
            }),
            // Comparison operators
            BinOp::Equal => Ok(Value::Call {
                head: "Equal".to_string(),
                args: vec![lhs.clone(), rhs.clone()],
            }),
            BinOp::Unequal => Ok(Value::Call {
                head: "Unequal".to_string(),
                args: vec![lhs.clone(), rhs.clone()],
            }),
            BinOp::Less => Ok(Value::Call {
                head: "Less".to_string(),
                args: vec![lhs.clone(), rhs.clone()],
            }),
            BinOp::Greater => Ok(Value::Call {
                head: "Greater".to_string(),
                args: vec![lhs.clone(), rhs.clone()],
            }),
            BinOp::LessEqual => Ok(Value::Call {
                head: "LessEqual".to_string(),
                args: vec![lhs.clone(), rhs.clone()],
            }),
            BinOp::GreaterEqual => Ok(Value::Call {
                head: "GreaterEqual".to_string(),
                args: vec![lhs.clone(), rhs.clone()],
            }),
            BinOp::And => Ok(Value::Call {
                head: "And".to_string(),
                args: vec![lhs.clone(), rhs.clone()],
            }),
            BinOp::Or => Ok(Value::Call {
                head: "Or".to_string(),
                args: vec![lhs.clone(), rhs.clone()],
            }),
            // Rule operators
            BinOp::Rule => Ok(Value::Rule {
                lhs: Box::new(lhs.clone()),
                rhs: Box::new(rhs.clone()),
                delayed: false,
            }),
            BinOp::RuleDelayed => Ok(Value::Rule {
                lhs: Box::new(lhs.clone()),
                rhs: Box::new(rhs.clone()),
                delayed: true,
            }),
            _ => Ok(Value::Call {
                head: "UnknownOp".to_string(),
                args: vec![lhs.clone(), rhs.clone()],
            }),
        }
    }

    /// Evaluate a condition predicate call in a result expression.
    fn eval_predicate_in_result(
        &mut self,
        head: &str,
        args: &[WLExpr],
        bindings: &Bindings,
        var: &str,
    ) -> Option<Value> {
        let result = match head {
            "FreeQ" if args.len() == 2 => {
                let expr = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let x_val = self.wl_to_value_with_bindings(&args[1], bindings, var);
                let x_str = value_to_symbol_name(&x_val).unwrap_or_else(|| var.to_string());
                Value::Bool(helpers::free_q(&expr, &x_str))
            }
            "EqQ" if args.len() == 2 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let b = self.wl_to_value_with_bindings(&args[1], bindings, var);
                Value::Bool(helpers::eq_q(&a, &b))
            }
            "NeQ" if args.len() == 2 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                let b = self.wl_to_value_with_bindings(&args[1], bindings, var);
                Value::Bool(helpers::ne_q(&a, &b))
            }
            "IntegerQ" if args.len() == 1 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                Value::Bool(helpers::integer_q(&a))
            }
            "PosQ" if args.len() == 1 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                Value::Bool(helpers::pos_q(&a))
            }
            "NegQ" if args.len() == 1 => {
                let a = self.wl_to_value_with_bindings(&args[0], bindings, var);
                Value::Bool(helpers::neg_q(&a))
            }
            _ => return None,
        };
        Some(result)
    }

    /// Convert a binary operator pattern to a Call pattern.
    fn binop_to_call(&self, op: &BinOp, lhs: &WLExpr, rhs: &WLExpr) -> WLExpr {
        let head = match op {
            BinOp::Plus => "Plus",
            BinOp::Minus => "Plus", // a - b is interpreted as Plus[a, Times[-1, b]]
            BinOp::Times => "Times",
            BinOp::Divide => "Times", // a/b is Times[a, Power[b, -1]]
            BinOp::Power => "Power",
            _ => {
                return WLExpr::BinaryOp {
                    op: *op,
                    lhs: Box::new(lhs.clone()),
                    rhs: Box::new(rhs.clone()),
                };
            }
        };

        // Handle special cases
        match op {
            BinOp::Minus => WLExpr::Call {
                head: Box::new(WLExpr::Symbol("Plus".to_string())),
                args: vec![
                    lhs.clone(),
                    WLExpr::Call {
                        head: Box::new(WLExpr::Symbol("Times".to_string())),
                        args: vec![WLExpr::Integer(-1), rhs.clone()],
                    },
                ],
            },
            BinOp::Divide => WLExpr::Call {
                head: Box::new(WLExpr::Symbol("Times".to_string())),
                args: vec![
                    lhs.clone(),
                    WLExpr::Call {
                        head: Box::new(WLExpr::Symbol("Power".to_string())),
                        args: vec![rhs.clone(), WLExpr::Integer(-1)],
                    },
                ],
            },
            _ => WLExpr::Call {
                head: Box::new(WLExpr::Symbol(head.to_string())),
                args: vec![lhs.clone(), rhs.clone()],
            },
        }
    }

    /// Convert a unary operator pattern to a Call pattern.
    fn unaryop_to_call(&self, op: &UnaryOp, expr: &WLExpr) -> WLExpr {
        match op {
            UnaryOp::Neg => WLExpr::Call {
                head: Box::new(WLExpr::Symbol("Times".to_string())),
                args: vec![WLExpr::Integer(-1), expr.clone()],
            },
            UnaryOp::Not => WLExpr::Call {
                head: Box::new(WLExpr::Symbol("Not".to_string())),
                args: vec![expr.clone()],
            },
        }
    }
}

// ── Utility functions ──

/// Check if a WLExpr head is a specific symbol.
fn is_symbol(head: &WLExpr, name: &str) -> bool {
    matches!(head, WLExpr::Symbol(s) if s == name)
}

/// Extract a symbol name from a Value.
pub fn value_to_symbol_name(val: &Value) -> Option<String> {
    match val {
        Value::Symbol(s) => Some(s.clone()),
        _ => None,
    }
}

/// Convert a Value back to an Expr (limited representation).
fn value_to_expr(val: &Value) -> crate::ast::Expr {
    use crate::ast::Expr;
    match val {
        Value::Integer(n) => Expr::Integer(n.clone()),
        Value::Real(r) => Expr::Real(r.clone()),
        Value::Bool(b) => Expr::Bool(*b),
        Value::Str(s) => Expr::Str(s.clone()),
        Value::Null => Expr::Null,
        Value::Symbol(s) => Expr::Symbol(s.clone()),
        Value::List(items) => Expr::List(items.iter().map(value_to_expr).collect()),
        Value::Call { head, args } => Expr::Call {
            head: Box::new(Expr::Symbol(head.clone())),
            args: args.iter().map(value_to_expr).collect(),
        },
        _ => Expr::Symbol("Null".to_string()),
    }
}

impl Default for RubiEngine {
    fn default() -> Self {
        Self::new()
    }
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

    #[test]
    fn test_match_blank() {
        let engine = RubiEngine::new();
        let pattern = WLExpr::Blank;
        assert!(engine.match_pattern(&pattern, &int(42), "x").is_some());
        assert!(engine.match_pattern(&pattern, &sym("y"), "x").is_some());
    }

    #[test]
    fn test_match_named_blank() {
        let engine = RubiEngine::new();
        let pattern = WLExpr::NamedBlank("a".to_string());
        let result = engine.match_pattern(&pattern, &int(42), "x");
        assert!(result.is_some());
        assert_eq!(result.unwrap().get("a"), Some(&int(42)));
    }

    #[test]
    fn test_match_symbol() {
        let engine = RubiEngine::new();
        let pattern = WLExpr::Symbol("x".to_string());
        assert!(engine.match_pattern(&pattern, &sym("x"), "x").is_some());
        assert!(engine.match_pattern(&pattern, &sym("y"), "x").is_none());
    }

    #[test]
    fn test_match_call() {
        let engine = RubiEngine::new();
        let pattern = WLExpr::Call {
            head: Box::new(WLExpr::Symbol("Power".to_string())),
            args: vec![
                WLExpr::NamedBlank("base".to_string()),
                WLExpr::NamedBlank("exp".to_string()),
            ],
        };
        let value = call("Power", vec![sym("x"), int(2)]);
        let result = engine.match_pattern(&pattern, &value, "x");
        assert!(result.is_some());
        let b = result.unwrap();
        assert_eq!(b.get("base"), Some(&sym("x")));
        assert_eq!(b.get("exp"), Some(&int(2)));
    }

    #[test]
    fn test_binop_plus_to_call() {
        let engine = RubiEngine::new();
        let pattern = engine.binop_to_call(
            &BinOp::Plus,
            &WLExpr::NamedBlank("a".to_string()),
            &WLExpr::NamedBlank("b".to_string()),
        );
        match pattern {
            WLExpr::Call { head, args } => {
                assert!(is_symbol(&head, "Plus"));
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_eval_binary_op_plus() {
        let engine = RubiEngine::new();
        let result = engine.eval_binary_op(BinOp::Plus, &int(1), &int(2));
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), call("Plus", vec![int(1), int(2)]));
    }

    #[test]
    fn test_eval_binary_op_minus() {
        let engine = RubiEngine::new();
        let result = engine.eval_binary_op(BinOp::Minus, &int(5), &int(3));
        assert!(result.is_ok());
        let v = result.unwrap();
        // a - b = Plus[a, Times[-1, b]]
        assert_eq!(
            v,
            call("Plus", vec![int(5), call("Times", vec![int(-1), int(3)])])
        );
    }

    #[test]
    fn test_eval_binary_op_divide() {
        let engine = RubiEngine::new();
        let result = engine.eval_binary_op(BinOp::Divide, &int(6), &int(3));
        assert!(result.is_ok());
        let v = result.unwrap();
        // a / b = Times[a, Power[b, -1]]
        assert_eq!(
            v,
            call("Times", vec![int(6), call("Power", vec![int(3), int(-1)])])
        );
    }
}
