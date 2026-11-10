//! list/functional builtins: Ratios, Catenate, FlattenAt, Pick, Operate, Distribute, ArrayReshape, Sow/Reap.
//!
//! Gap-fill module; see the individual builtin docs for semantics.

use crate::env::Env;
use crate::eval::apply_function;
use crate::pattern::{AttributeChecker, MatchResult, match_pattern};
use crate::value::{EvalError, Value};

// ── Shared helpers ──

/// Borrow the elements of a list, or raise a `TypeError`.
fn as_list<'a>(v: &'a Value, name: &str) -> Result<&'a [Value], EvalError> {
    match v {
        Value::List(items) => Ok(items.as_slice()),
        _ => Err(EvalError::TypeError {
            expected: format!("List in {name}"),
            got: v.type_name().to_string(),
        }),
    }
}

/// Normalize a 1-indexed position (negative counts from the end) to 0-based.
fn normalize_index(index: i64, len: usize) -> Result<usize, EvalError> {
    if index == 0 {
        return Err(EvalError::IndexOutOfBounds { index, length: len });
    }
    let idx = if index > 0 {
        (index - 1) as usize
    } else {
        let abs = (-index) as usize;
        if abs > len {
            return Err(EvalError::IndexOutOfBounds { index, length: len });
        }
        len - abs
    };
    if idx >= len {
        return Err(EvalError::IndexOutOfBounds { index, length: len });
    }
    Ok(idx)
}

/// Test whether `value` matches a (possibly held) pattern.
fn value_matches(value: &Value, patt: &Value, env: &Env) -> bool {
    let checker = AttributeChecker::new(env.attributes.clone());
    let pattern = match patt {
        Value::Pattern(expr) => expr.clone(),
        other => crate::eval::table::value_to_expr(other),
    };
    matches!(
        match_pattern(&pattern, value, Some(&checker)),
        MatchResult::Match(_)
    )
}

// ── Ratios ──

/// Ratios[{x1, x2, ...}] → {x2/x1, x3/x2, ...}.
pub fn builtin_ratios(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Ratios requires exactly 1 argument".to_string(),
        ));
    }
    let items = as_list(&args[0], "Ratios")?;
    if items.len() < 2 {
        return Ok(Value::List(Vec::new()));
    }
    let divide = Value::Symbol("Divide".to_string());
    let mut result = Vec::with_capacity(items.len() - 1);
    for i in 1..items.len() {
        result.push(apply_function(
            &divide,
            &[items[i].clone(), items[i - 1].clone()],
            env,
        )?);
    }
    Ok(Value::List(result))
}

// ── Catenate ──

/// Catenate[{{a, b}, {c}, d, ...}] → {a, b, c, d, ...}.
///
/// List elements are spliced one level; non-list elements are appended as-is.
pub fn builtin_catenate(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Catenate requires exactly 1 argument".to_string(),
        ));
    }
    let outer = as_list(&args[0], "Catenate")?;
    let mut result = Vec::new();
    for item in outer {
        match item {
            Value::List(inner) => result.extend(inner.iter().cloned()),
            other => result.push(other.clone()),
        }
    }
    Ok(Value::List(result))
}

// ── FlattenAt ──

/// FlattenAt[expr, n] flattens the subexpression at position `n` one level.
///
/// Supports `List` and `Call` expressions; position 0 (the head) is left as-is.
pub fn builtin_flatten_at(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "FlattenAt requires exactly 2 arguments".to_string(),
        ));
    }
    let n = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
        expected: "Integer".to_string(),
        got: args[1].type_name().to_string(),
    })?;
    match &args[0] {
        Value::List(items) => {
            let mut parts = items.clone();
            flatten_part(&mut parts, n)?;
            Ok(Value::List(parts))
        }
        Value::Call {
            head,
            args: call_args,
        } => {
            let mut parts = call_args.clone();
            flatten_part(&mut parts, n)?;
            Ok(Value::Call {
                head: head.clone(),
                args: parts,
            })
        }
        other => Ok(other.clone()),
    }
}

/// Replace `parts[idx]` (a sublist) with its elements, flattening one level.
fn flatten_part(parts: &mut Vec<Value>, n: i64) -> Result<(), EvalError> {
    if n == 0 {
        return Ok(());
    }
    let idx = normalize_index(n, parts.len())?;
    if let Value::List(inner) = &parts[idx] {
        let inner = inner.clone();
        parts.splice(idx..=idx, inner);
    }
    Ok(())
}

// ── Pick ──

/// Pick[list, sel] selects elements where the selector is `True`.
/// Pick[list, sel, patt] selects elements where the selector matches `patt`.
pub fn builtin_pick(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "Pick requires 2 or 3 arguments".to_string(),
        ));
    }
    let items = as_list(&args[0], "Pick")?;
    let selectors = as_list(&args[1], "Pick")?;
    let mut result = Vec::new();
    for (item, selector) in items.iter().zip(selectors.iter()) {
        let keep = if args.len() == 3 {
            value_matches(selector, &args[2], env)
        } else {
            matches!(selector, Value::Bool(true))
        };
        if keep {
            result.push(item.clone());
        }
    }
    Ok(Value::List(result))
}

// ── Operate ──

/// Operate[p, expr, n] applies `p` to the head of `expr` at level `n`.
///
/// Level 1 is the head of `expr` (the default); higher levels walk down the
/// head chain. `Operate[p, f[x, y]]` → `p[f][x, y]`.
pub fn builtin_operate(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "Operate requires 2 or 3 arguments".to_string(),
        ));
    }
    let n = match args.get(2) {
        Some(v) => v.to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "Integer".to_string(),
            got: v.type_name().to_string(),
        })?,
        None => 1,
    };
    if n < 1 {
        return Err(EvalError::Error(
            "Operate: level must be a positive integer".to_string(),
        ));
    }
    operate_at(&args[0], &args[1], n, env)
}

/// Recursively descend the head chain to `n`, then apply `p` to that head.
fn operate_at(p: &Value, expr: &Value, n: i64, env: &Env) -> Result<Value, EvalError> {
    let head = head_value(expr, env);
    if n == 1 {
        let new_head = apply_function(p, &[head], env)?;
        Ok(rebuild_with_head(expr, &new_head))
    } else {
        let new_head = operate_at(p, &head, n - 1, env)?;
        Ok(rebuild_with_head(expr, &new_head))
    }
}

/// The head of `expr` as a value (an atom yields `Symbol`).
fn head_value(expr: &Value, env: &Env) -> Value {
    match expr {
        Value::Call { head, .. } => {
            if is_simple_symbol(head) {
                Value::Symbol(head.clone())
            } else {
                parse_value(head, env)
            }
        }
        _ => Value::Symbol("Symbol".to_string()),
    }
}

/// Rebuild an expression with a new head. For atoms the head becomes the value.
fn rebuild_with_head(expr: &Value, new_head: &Value) -> Value {
    match expr {
        Value::Call { args, .. } => Value::Call {
            head: head_string(new_head),
            args: args.clone(),
        },
        _ => new_head.clone(),
    }
}

/// Render a value back into a head string (nested heads are stored textually).
fn head_string(v: &Value) -> String {
    match v {
        Value::Symbol(s) => s.clone(),
        other => other.to_string(),
    }
}

/// Whether `s` is a bare identifier (no brackets/operators).
fn is_simple_symbol(s: &str) -> bool {
    !s.is_empty()
        && s.chars()
            .all(|c| c.is_alphanumeric() || c == '$' || c == '`')
}

/// Parse a head string into a value, falling back to a symbol on failure.
fn parse_value(s: &str, env: &Env) -> Value {
    if let Ok(tokens) = crate::lexer::tokenize(s)
        && let Ok(ast) = crate::parser::parse(tokens)
        && ast.len() == 1
        && let Ok(v) = crate::eval::eval(&ast[0], env)
    {
        return v;
    }
    Value::Symbol(s.to_string())
}

// ── Distribute ──

/// Distribute[f[a + b, c]] → f[a, c] + f[b, c].
///
/// An optional second argument gives the head to distribute over (default
/// `Plus`); an optional third argument restricts distribution to `h`-headed
/// subexpressions.
pub fn builtin_distribute(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 3 {
        return Err(EvalError::Error(
            "Distribute requires 1 to 3 arguments".to_string(),
        ));
    }
    let g = match args.get(1) {
        Some(Value::Symbol(s)) => s.clone(),
        Some(other) => other.to_string(),
        None => "Plus".to_string(),
    };
    let h = match args.get(2) {
        Some(Value::Symbol(s)) => Some(s.as_str()),
        _ => None,
    };
    Ok(distribute_value(&args[0], &g, h))
}

fn distribute_value(expr: &Value, g: &str, h: Option<&str>) -> Value {
    let Value::Call { head, args } = expr else {
        return expr.clone();
    };
    if let Some(h) = h
        && head != h
    {
        return expr.clone();
    }

    let mut any_distributed = false;
    let mut groups: Vec<Vec<Value>> = Vec::with_capacity(args.len());
    for arg in args {
        match arg {
            Value::Call {
                head: arg_head,
                args: inner,
            } if arg_head == g => {
                groups.push(inner.clone());
                any_distributed = true;
            }
            _ => groups.push(vec![arg.clone()]),
        }
    }
    if !any_distributed {
        return expr.clone();
    }

    // Cartesian product of the per-argument alternatives.
    let mut combos: Vec<Vec<Value>> = vec![Vec::new()];
    for group in groups {
        let mut next = Vec::with_capacity(combos.len() * group.len());
        for combo in &combos {
            for item in &group {
                let mut extended = combo.clone();
                extended.push(item.clone());
                next.push(extended);
            }
        }
        combos = next;
    }

    let terms = combos
        .into_iter()
        .map(|combo| Value::Call {
            head: head.clone(),
            args: combo,
        })
        .collect();
    Value::Call {
        head: g.to_string(),
        args: terms,
    }
}

// ── ArrayReshape ──

/// ArrayReshape[list, dims] rearranges a flat list into a nested list of shape `dims`.
pub fn builtin_array_reshape(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ArrayReshape requires exactly 2 arguments".to_string(),
        ));
    }
    let flat = as_list(&args[0], "ArrayReshape")?;
    let dim_values = as_list(&args[1], "ArrayReshape")?;
    if dim_values.is_empty() {
        return Err(EvalError::Error(
            "ArrayReshape: dimensions list cannot be empty".to_string(),
        ));
    }
    let mut dims = Vec::with_capacity(dim_values.len());
    for d in dim_values {
        let n = d.to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "Integer".to_string(),
            got: d.type_name().to_string(),
        })?;
        if n <= 0 {
            return Err(EvalError::Error(
                "ArrayReshape: dimension must be positive".to_string(),
            ));
        }
        dims.push(n as usize);
    }
    let total: usize = dims.iter().product();
    if total != flat.len() {
        return Err(EvalError::Error(format!(
            "ArrayReshape: total elements ({}) does not match product of dimensions ({})",
            flat.len(),
            total
        )));
    }

    fn build(flat: &[Value], dims: &[usize], offset: usize) -> Value {
        if dims.len() == 1 {
            Value::List(flat[offset..offset + dims[0]].to_vec())
        } else {
            let stride: usize = dims[1..].iter().product();
            let rows = (0..dims[0])
                .map(|i| build(flat, &dims[1..], offset + i * stride))
                .collect();
            Value::List(rows)
        }
    }
    Ok(build(flat, &dims, 0))
}

// ── Sow / Reap ──

/// Binding name for the collector stack kept on the environment.
const SOW_STACK: &str = "$SowStack";

fn read_frames(env: &Env) -> Vec<Value> {
    match env.get(SOW_STACK) {
        Some(Value::List(frames)) => frames,
        _ => Vec::new(),
    }
}

fn write_frames(env: &Env, frames: Vec<Value>) {
    env.set_propagate(SOW_STACK.to_string(), Value::List(frames));
}

/// Sow[x] / Sow[x, tag] — record `x` in the innermost active `Reap` and return `x`.
pub fn builtin_sow(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Sow requires 1 or 2 arguments".to_string(),
        ));
    }
    let value = args[0].clone();
    let tag = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| Value::Symbol("None".to_string()));

    let mut frames = read_frames(env);
    if let Some(Value::List(entries)) = frames.last_mut() {
        entries.push(Value::List(vec![tag, value.clone()]));
        write_frames(env, frames);
    }
    Ok(value)
}

/// Reap[expr] / Reap[expr, patt] — evaluate `expr`, returning `{expr, {{vals...}}}`
/// where `vals` are the values sown during evaluation whose tag matches `patt`.
pub fn builtin_reap(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "Reap requires 1 or 2 arguments".to_string(),
        ));
    }
    let expr_ast = match &args[0] {
        Value::Pattern(expr) => expr.clone(),
        other => crate::eval::table::value_to_expr(other),
    };
    let patt = args.get(1).cloned();

    // Push a fresh collector frame.
    let mut frames = read_frames(env);
    frames.push(Value::List(Vec::new()));
    write_frames(env, frames);

    let result = crate::eval::eval(&expr_ast, env);

    // Pop our frame (happens even if evaluation errored).
    let mut frames = read_frames(env);
    let frame = frames.pop().unwrap_or_else(|| Value::List(Vec::new()));
    if frames.is_empty() {
        env.remove(SOW_STACK);
    } else {
        write_frames(env, frames);
    }
    let expr_value = result?;

    let entries = match frame {
        Value::List(entries) => entries,
        _ => Vec::new(),
    };
    let mut collected = Vec::new();
    for entry in entries {
        let Value::List(pair) = entry else { continue };
        let [tag, value] = pair.as_slice() else {
            continue;
        };
        let keep = match &patt {
            Some(p) => value_matches(tag, p, env),
            None => true,
        };
        if keep {
            collected.push(value.clone());
        }
    }

    Ok(Value::List(vec![
        expr_value,
        Value::List(vec![Value::List(collected)]),
    ]))
}

// ── Registration ──

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    super::register_builtin(env, "Catenate", builtin_catenate);
    super::register_builtin(env, "FlattenAt", builtin_flatten_at);
    super::register_builtin(env, "Distribute", builtin_distribute);
    super::register_builtin(env, "ArrayReshape", builtin_array_reshape);
    super::register_builtin_env(env, "Ratios", builtin_ratios);
    super::register_builtin_env(env, "Pick", builtin_pick);
    super::register_builtin_env(env, "Operate", builtin_operate);
    super::register_builtin_env(env, "Sow", builtin_sow);
    super::register_builtin_env(env, "Reap", builtin_reap);

    env.set_attributes("Reap", vec!["HoldAll".to_string()]);
    for name in [
        "Ratios",
        "Catenate",
        "FlattenAt",
        "Pick",
        "Operate",
        "Distribute",
        "ArrayReshape",
        "Sow",
        "Reap",
    ] {
        let mut attrs = env.get_attributes(name);
        if !attrs.contains(&"Protected".to_string()) {
            attrs.push("Protected".to_string());
        }
        env.set_attributes(name, attrs);
    }
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;
    use rug::Integer;

    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }

    fn list(vals: Vec<Value>) -> Value {
        Value::List(vals)
    }

    fn sym(s: &str) -> Value {
        Value::Symbol(s.to_string())
    }

    fn call(head: &str, args: Vec<Value>) -> Value {
        Value::Call {
            head: head.to_string(),
            args,
        }
    }

    fn test_env() -> Env {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        env
    }

    fn eval_str(input: &str) -> Value {
        let env = test_env();
        let tokens = crate::lexer::tokenize(input).unwrap();
        let ast = crate::parser::parse(tokens).unwrap_or_else(|e| panic!("parse {input}: {e:?}"));
        crate::eval::eval_program(&ast, &env).unwrap_or_else(|e| panic!("eval {input}: {e:?}"))
    }

    // ── Ratios ──

    #[test]
    fn test_ratios_basic() {
        let env = test_env();
        let result = builtin_ratios(&[list(vec![int(1), int(2), int(4)])], &env).unwrap();
        assert_eq!(result, list(vec![int(2), int(2)]));
    }

    #[test]
    fn test_ratios_symbolic() {
        let env = test_env();
        let result = builtin_ratios(&[list(vec![sym("a"), sym("b"), sym("c")])], &env).unwrap();
        assert_eq!(
            result,
            list(vec![
                call("Divide", vec![sym("b"), sym("a")]),
                call("Divide", vec![sym("c"), sym("b")]),
            ])
        );
    }

    #[test]
    fn test_ratios_short() {
        let env = test_env();
        assert_eq!(
            builtin_ratios(&[list(vec![int(5)])], &env).unwrap(),
            list(vec![])
        );
        assert_eq!(builtin_ratios(&[list(vec![])], &env).unwrap(), list(vec![]));
    }

    // ── Catenate ──

    #[test]
    fn test_catenate_basic() {
        let result =
            builtin_catenate(&[list(vec![list(vec![int(1), int(2)]), list(vec![int(3)])])])
                .unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3)]));
    }

    #[test]
    fn test_catenate_leaves() {
        let result =
            builtin_catenate(&[list(vec![int(1), list(vec![int(2), int(3)]), int(4)])]).unwrap();
        assert_eq!(result, list(vec![int(1), int(2), int(3), int(4)]));
    }

    #[test]
    fn test_catenate_non_list() {
        assert!(builtin_catenate(&[int(0)]).is_err());
        assert!(builtin_catenate(&[]).is_err());
    }

    // ── FlattenAt ──

    #[test]
    fn test_flatten_at_call() {
        let expr = call("f", vec![sym("a"), list(vec![sym("b"), sym("c")])]);
        let result = builtin_flatten_at(&[expr, int(2)]).unwrap();
        assert_eq!(result, call("f", vec![sym("a"), sym("b"), sym("c")]));
    }

    #[test]
    fn test_flatten_at_list_negative() {
        let expr = list(vec![list(vec![int(1), int(2)]), list(vec![int(3), int(4)])]);
        let result = builtin_flatten_at(&[expr, int(-1)]).unwrap();
        assert_eq!(
            result,
            list(vec![list(vec![int(1), int(2)]), int(3), int(4)])
        );
    }

    #[test]
    fn test_flatten_at_non_list_part() {
        let expr = call("f", vec![sym("a"), sym("b")]);
        assert_eq!(builtin_flatten_at(&[expr.clone(), int(1)]).unwrap(), expr);
    }

    // ── Pick ──

    #[test]
    fn test_pick_true_only() {
        let env = test_env();
        let sel = list(vec![
            Value::Bool(true),
            Value::Bool(false),
            Value::Bool(true),
        ]);
        let result = builtin_pick(&[list(vec![int(10), int(20), int(30)]), sel], &env).unwrap();
        assert_eq!(result, list(vec![int(10), int(30)]));
    }

    #[test]
    fn test_pick_with_pattern() {
        let env = test_env();
        let sel = list(vec![int(1), sym("a"), int(2)]);
        let result = builtin_pick(
            &[
                list(vec![sym("x"), sym("y"), sym("z")]),
                sel,
                sym("_Integer"),
            ],
            &env,
        )
        .unwrap();
        assert_eq!(result, list(vec![sym("x"), sym("z")]));
    }

    // ── Operate ──

    #[test]
    fn test_operate_level1() {
        let env = test_env();
        let expr = call("f", vec![sym("x"), sym("y")]);
        let result = builtin_operate(&[sym("Framed"), expr], &env).unwrap();
        assert_eq!(result, call("Framed[f]", vec![sym("x"), sym("y")]));
    }

    #[test]
    fn test_operate_explicit_level() {
        let env = test_env();
        let expr = call("f", vec![sym("x")]);
        let result = builtin_operate(&[sym("Framed"), expr, int(1)], &env).unwrap();
        assert_eq!(result, call("Framed[f]", vec![sym("x")]));
    }

    // ── Distribute ──

    #[test]
    fn test_distribute_basic() {
        let expr = call("f", vec![call("Plus", vec![sym("a"), sym("b")]), sym("c")]);
        let result = builtin_distribute(&[expr]).unwrap();
        assert_eq!(
            result,
            call(
                "Plus",
                vec![
                    call("f", vec![sym("a"), sym("c")]),
                    call("f", vec![sym("b"), sym("c")]),
                ]
            )
        );
    }

    #[test]
    fn test_distribute_multiple() {
        let expr = call(
            "f",
            vec![
                call("Plus", vec![sym("a"), sym("b")]),
                call("Plus", vec![sym("c"), sym("d")]),
            ],
        );
        let result = builtin_distribute(&[expr]).unwrap();
        if let Value::Call { head, args } = &result {
            assert_eq!(head, "Plus");
            assert_eq!(args.len(), 4);
        } else {
            panic!("expected Plus call, got {result:?}");
        }
    }

    #[test]
    fn test_distribute_custom_head() {
        let expr = call("f", vec![call("g", vec![sym("a"), sym("b")]), sym("c")]);
        let result = builtin_distribute(&[expr, sym("g")]).unwrap();
        assert_eq!(
            result,
            call(
                "g",
                vec![
                    call("f", vec![sym("a"), sym("c")]),
                    call("f", vec![sym("b"), sym("c")]),
                ]
            )
        );
    }

    #[test]
    fn test_distribute_no_op() {
        let expr = call("f", vec![sym("a"), sym("b")]);
        assert_eq!(builtin_distribute(&[expr.clone()]).unwrap(), expr);
    }

    // ── ArrayReshape ──

    #[test]
    fn test_array_reshape() {
        let flat = list((1..=6).map(int).collect());
        let result = builtin_array_reshape(&[flat, list(vec![int(2), int(3)])]).unwrap();
        assert_eq!(
            result,
            list(vec![
                list(vec![int(1), int(2), int(3)]),
                list(vec![int(4), int(5), int(6)]),
            ])
        );
    }

    #[test]
    fn test_array_reshape_mismatch() {
        let flat = list(vec![int(1), int(2), int(3)]);
        assert!(builtin_array_reshape(&[flat, list(vec![int(2), int(2)])]).is_err());
    }

    // ── Sow / Reap ──

    #[test]
    fn test_reap_collects_sown() {
        let result = eval_str("Reap[{Sow[1], Sow[2]}]");
        assert_eq!(
            result,
            list(vec![
                list(vec![int(1), int(2)]),
                list(vec![list(vec![int(1), int(2)])]),
            ])
        );
    }

    #[test]
    fn test_reap_tag_filter() {
        let result = eval_str("Reap[{Sow[10, \"a\"], Sow[20, \"b\"], Sow[30, \"a\"]}, \"a\"]");
        assert_eq!(
            result,
            list(vec![
                list(vec![int(10), int(20), int(30)]),
                list(vec![list(vec![int(10), int(30)])]),
            ])
        );
    }

    #[test]
    fn test_sow_without_reap_returns_value() {
        assert_eq!(eval_str("Sow[7]"), int(7));
    }

    #[test]
    fn test_nested_reap() {
        // The inner Reap sees only its own Sow; the outer frame stays balanced.
        let result = eval_str("Reap[{Sow[1], Reap[{Sow[2]}], Sow[3]}]");
        assert_eq!(
            result,
            list(vec![
                list(vec![
                    int(1),
                    list(vec![list(vec![int(2)]), list(vec![list(vec![int(2)])])]),
                    int(3)
                ]),
                list(vec![list(vec![int(1), int(3)])]),
            ])
        );
    }
}
