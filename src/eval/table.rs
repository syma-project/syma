/// Table, Sum, ParallelTable and shared iterator specification evaluation.
use rug::Integer;

use crate::ast::*;
use crate::env::Env;
use crate::value::*;

/// Convert a Value back to an Expr for re-processing by special forms.
fn value_to_expr(v: &Value) -> Expr {
    match v {
        Value::Integer(n) => Expr::Integer(n.clone()),
        Value::Real(r) => Expr::Real(r.clone()),
        Value::Bool(b) => Expr::Bool(*b),
        Value::Str(s) => Expr::Str(s.clone()),
        Value::Symbol(s) => Expr::Symbol(s.clone()),
        Value::Null => Expr::Null,
        Value::List(items) => Expr::List(items.iter().map(value_to_expr).collect()),
        Value::Call { head, args } => Expr::Call {
            head: Box::new(Expr::Symbol(head.clone())),
            args: args.iter().map(value_to_expr).collect(),
        },
        Value::Sequence(items) => Expr::Call {
            head: Box::new(Expr::Symbol("Sequence".to_string())),
            args: items.iter().map(value_to_expr).collect(),
        },
        _ => Expr::Symbol(v.to_string()),
    }
}

/// Table[expr, {i, ...}] / Table[expr, n] — generate lists by iteration.
pub(super) fn eval_table(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    // Handle Sequence splicing: if a single arg evaluates to a Sequence,
    // expand it and retry.
    if args.len() == 1 {
        let evaluated = super::eval(&args[0], env)?;
        if let Value::Sequence(seq) = evaluated {
            let expanded: Vec<Expr> = seq.iter().map(value_to_expr).collect();
            return eval_table(&expanded, env);
        }
    }

    if args.len() < 2 {
        return Err(EvalError::Error(
            "Table requires at least 2 arguments".to_string(),
        ));
    }

    let expr = &args[0];

    // Case 1: Table[expr, n] — n copies of expr (no variable binding)
    if args.len() == 2 {
        // Check if second arg is a plain integer (not a list)
        if let Expr::Integer(_) = &args[1] {
            let n =
                super::eval(&args[1], env)?
                    .to_integer()
                    .ok_or_else(|| EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    })?;
            if n < 0 {
                return Err(EvalError::Error(
                    "Table count must be non-negative".to_string(),
                ));
            }
            let mut result = Vec::new();
            for _ in 0..n {
                result.push(super::eval(expr, env)?);
            }
            return Ok(Value::List(result));
        }
    }

    // Case 2: Table[expr, {i, ...}] or Table[expr, {i, ...}, {j, ...}, ...]
    let iter_specs = &args[1..];
    eval_table_recursive(expr, iter_specs, env, 0)
}

/// Parse an iterator spec list and generate iteration values.
pub(super) fn eval_iterator_spec(
    items: &[Expr],
    env: &Env,
) -> Result<(String, Vec<Value>), EvalError> {
    let var_name = match &items[0] {
        Expr::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::Error(
                "iterator variable must be a symbol".to_string(),
            ));
        }
    };

    match items.len() {
        2 => {
            // {var, n} or {var, {values}}
            if let Expr::List(_) = &items[1] {
                let list_val = super::eval(&items[1], env)?;
                match list_val {
                    Value::List(items) => Ok((var_name, items)),
                    _ => Err(EvalError::TypeError {
                        expected: "List".to_string(),
                        got: list_val.type_name().to_string(),
                    }),
                }
            } else {
                let n = super::eval(&items[1], env)?.to_integer().ok_or_else(|| {
                    EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    }
                })?;
                let values: Vec<Value> =
                    (1..=n).map(|i| Value::Integer(Integer::from(i))).collect();
                Ok((var_name, values))
            }
        }
        3 => {
            // {var, min, max}
            let min =
                super::eval(&items[1], env)?
                    .to_integer()
                    .ok_or_else(|| EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    })?;
            let max =
                super::eval(&items[2], env)?
                    .to_integer()
                    .ok_or_else(|| EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    })?;
            let values: Vec<Value> = (min..=max)
                .map(|i| Value::Integer(Integer::from(i)))
                .collect();
            Ok((var_name, values))
        }
        4 => {
            // {var, min, max, step}
            let min =
                super::eval(&items[1], env)?
                    .to_integer()
                    .ok_or_else(|| EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    })?;
            let max =
                super::eval(&items[2], env)?
                    .to_integer()
                    .ok_or_else(|| EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    })?;
            let step =
                super::eval(&items[3], env)?
                    .to_integer()
                    .ok_or_else(|| EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: "non-Integer".to_string(),
                    })?;
            if step == 0 {
                return Err(EvalError::Error("iterator step cannot be zero".to_string()));
            }
            let mut values = Vec::new();
            if step > 0 {
                let mut i = min;
                while i <= max {
                    values.push(Value::Integer(Integer::from(i)));
                    i += step;
                }
            } else {
                let mut i = min;
                while i >= max {
                    values.push(Value::Integer(Integer::from(i)));
                    i += step;
                }
            }
            Ok((var_name, values))
        }
        _ => Err(EvalError::Error(
            "iterator spec must have 2-4 elements".to_string(),
        )),
    }
}

/// Recursive helper for nested Table iteration.
fn eval_table_recursive(
    expr: &Expr,
    iter_specs: &[Expr],
    env: &Env,
    depth: usize,
) -> Result<Value, EvalError> {
    if depth >= iter_specs.len() {
        // Base case: all iterators processed, evaluate expression
        return super::eval(expr, env);
    }

    // Bare integer — count-only form, equivalent to {n}
    if let Expr::Integer(_) = &iter_specs[depth] {
        let n = super::eval(&iter_specs[depth], env)?
            .to_integer()
            .ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: "non-Integer".to_string(),
            })?;
        if n < 0 {
            return Err(EvalError::Error(
                "Table count must be non-negative".to_string(),
            ));
        }
        let mut result = Vec::new();
        for _ in 0..n {
            result.push(eval_table_recursive(expr, iter_specs, env, depth + 1)?);
        }
        return Ok(Value::List(result));
    }

    let iter_items = match &iter_specs[depth] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Table iterator spec must be a list".to_string(),
            ));
        }
    };

    // {n} — count-only form (no iterator variable)
    if iter_items.len() == 1 {
        let n = super::eval(&iter_items[0], env)?
            .to_integer()
            .ok_or_else(|| EvalError::TypeError {
                expected: "Integer".to_string(),
                got: "non-Integer".to_string(),
            })?;
        if n < 0 {
            return Err(EvalError::Error(
                "Table count must be non-negative".to_string(),
            ));
        }
        let mut result = Vec::new();
        for _ in 0..n {
            result.push(eval_table_recursive(expr, iter_specs, env, depth + 1)?);
        }
        return Ok(Value::List(result));
    }

    let (var_name, values) = eval_iterator_spec(iter_items, env)?;

    // Generate results for this iterator level
    let child_env = env.child();
    let mut result = Vec::new();
    for val in values {
        child_env.set(var_name.clone(), val);
        result.push(eval_table_recursive(
            expr,
            iter_specs,
            &child_env,
            depth + 1,
        )?);
    }

    Ok(Value::List(result))
}

/// Sum[expr, {i, min, max}] — like Table but adds results.
pub(super) fn eval_sum(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Sum requires exactly 2 arguments".to_string(),
        ));
    }
    let iter_items = match &args[1] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Sum iterator spec must be a list".to_string(),
            ));
        }
    };

    let (var_name, values) = eval_iterator_spec(iter_items, env)?;

    let expr = &args[0];
    let child_env = env.child();
    let mut acc = Value::Integer(Integer::from(0));

    for val in values {
        child_env.set(var_name.clone(), val);
        let val = super::eval(expr, &child_env)?;
        acc = crate::builtins::add_values_public(&acc, &val)?;
    }

    Ok(acc)
}

/// Product[expr, {i, min, max}] — like Sum but multiplies results.
pub(super) fn eval_product(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Product requires exactly 2 arguments".to_string(),
        ));
    }
    let iter_items = match &args[1] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "Product iterator spec must be a list".to_string(),
            ));
        }
    };

    let (var_name, values) = eval_iterator_spec(iter_items, env)?;

    let expr = &args[0];
    let child_env = env.child();
    let mut acc = Value::Integer(Integer::from(1));

    for val in values {
        child_env.set(var_name.clone(), val);
        let val = super::eval(expr, &child_env)?;
        acc = crate::builtins::mul_values_public(&acc, &val)?;
    }

    Ok(acc)
}

/// RecurrenceTable[{eqns, ...}, f, {n, nmin, nmax}] — generate a table from recurrence equations.
///
/// Handles the common pattern: a[1] == init, a[n+1] == expr_involving_a[n].
/// Returns a list of f[n] values for n = nmin .. nmax.
pub(super) fn eval_recurrence_table(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return Err(EvalError::Error(
            "RecurrenceTable requires 3 arguments: RecurrenceTable[eqns, f, {n, nmin, nmax}]"
                .to_string(),
        ));
    }

    // Parse the function name (arg 2)
    let func_name = match &args[1] {
        Expr::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::Error(
                "RecurrenceTable: second argument must be a symbol (function name)".to_string(),
            ));
        }
    };

    // Parse iterator spec (arg 3) — {n, nmin, nmax}
    let iter_items = match &args[2] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "RecurrenceTable: third argument must be an iterator spec list".to_string(),
            ));
        }
    };
    let (var_name, _) = eval_iterator_spec(iter_items, env)?;

    // Evaluate iterator bounds to get nmin and nmax
    let nmin = match iter_items.get(1) {
        Some(expr) => super::eval(expr, env)?.to_integer().ok_or_else(|| {
            EvalError::Error("RecurrenceTable: nmin must be an integer".to_string())
        })?,
        None => 1,
    };
    let nmax = match iter_items.get(2) {
        Some(expr) => super::eval(expr, env)?.to_integer().ok_or_else(|| {
            EvalError::Error("RecurrenceTable: nmax must be an integer".to_string())
        })?,
        None => nmin,
    };

    // Parse equations (arg 1) — a list of equations
    let eqns = match &args[0] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "RecurrenceTable: first argument must be a list of equations".to_string(),
            ));
        }
    };

    // Extract initial conditions and recurrence bodies
    // init_map: index -> value (pre-computed initial values)
    // recurrence_body: the Expr to evaluate at each step (the RHS of the recurrence)
    // recurrence_delta: how many steps forward the recurrence goes (default 1 for a[n+1])
    use std::collections::HashMap;
    let mut init_conds: HashMap<i64, Expr> = HashMap::new();
    let mut recurrence_body: Option<Expr> = None;

    for eqn in eqns {
        // Each equation should be Equal[lhs, rhs]
        let (lhs, rhs) = match eqn {
            Expr::Call { head, args }
                if matches!(head.as_ref(), Expr::Symbol(s) if s == "Equal") && args.len() == 2 =>
            {
                (&args[0], &args[1])
            }
            _ => {
                return Err(EvalError::Error(
                    "RecurrenceTable: each equation must be of the form lhs == rhs".to_string(),
                ));
            }
        };

        // LHS should be func_name[...]
        let lhs_args = match lhs {
            Expr::Call { head, args } if matches!(head.as_ref(), Expr::Symbol(s) if *s == func_name) => {
                args
            }
            _ => {
                return Err(EvalError::Error(format!(
                    "RecurrenceTable: LHS of equation must be {}[...]",
                    func_name
                )));
            }
        };

        if lhs_args.len() != 1 {
            return Err(EvalError::Error(format!(
                "RecurrenceTable: {}[...] takes exactly 1 argument",
                func_name
            )));
        }

        let arg = &lhs_args[0];

        // Check if this is an initial condition: f[k] where k is an integer
        if let Expr::Integer(k) = arg {
            let idx = k.to_i64().ok_or_else(|| {
                EvalError::Error("RecurrenceTable: initial condition index too large".to_string())
            })?;
            init_conds.insert(idx, rhs.clone());
            continue;
        }

        // Check if this is a recurrence: f[var + 1] or f[1 + var]
        let is_recurrence = match arg {
            Expr::Call { head, args }
                if matches!(head.as_ref(), Expr::Symbol(s) if s == "Plus") && args.len() == 2 =>
            {
                let has_var = matches!(&args[0], Expr::Symbol(v) if *v == var_name)
                    || matches!(&args[1], Expr::Symbol(v) if *v == var_name);
                let has_one = matches!(&args[0], Expr::Integer(n) if *n == 1)
                    || matches!(&args[1], Expr::Integer(n) if *n == 1);
                has_var && has_one
            }
            _ => false,
        };

        if is_recurrence {
            recurrence_body = Some(rhs.clone());
        } else {
            return Err(EvalError::Error(
                "RecurrenceTable: unrecognized equation form".to_string(),
            ));
        }
    }

    let body = recurrence_body.ok_or_else(|| {
        EvalError::Error(
            "RecurrenceTable: no recurrence equation found (e.g., f[n+1] == expr)".to_string(),
        )
    })?;

    // Store computed values for substitution into the recurrence body
    let mut computed: HashMap<i64, Value> = HashMap::new();

    // Evaluate initial conditions first
    for (idx, expr) in &init_conds {
        let val = super::eval(expr, env)?;
        computed.insert(*idx, val);
    }

    // Helper: walk an Expr and substitute f[idx] with computed values.
    // Handles both f[k] (integer literal) and f[var_name] (current step).
    fn substitute_fn_refs(
        expr: &Expr,
        func_name: &str,
        var_name: &str,
        current_i: i64,
        computed: &HashMap<i64, Value>,
    ) -> Expr {
        match expr {
            Expr::Call { head, args } if args.len() == 1 => {
                if let Expr::Symbol(s) = head.as_ref()
                    && s == func_name {
                        // Determine the index: integer literal, or the var_name symbol → use current_i
                        let idx = match &args[0] {
                            Expr::Integer(k) => k.to_i64().unwrap_or(0),
                            Expr::Symbol(v) if *v == var_name => current_i,
                            _ => {
                                // Unknown index form — recurse and return a call
                                return Expr::Call {
                                    head: Box::new(substitute_fn_refs(
                                        head, func_name, var_name, current_i, computed,
                                    )),
                                    args: vec![substitute_fn_refs(
                                        &args[0], func_name, var_name, current_i, computed,
                                    )],
                                };
                            }
                        };
                        if let Some(val) = computed.get(&idx) {
                            return match val {
                                Value::Integer(n) => Expr::Integer(n.clone()),
                                Value::Real(r) => Expr::Real(r.clone()),
                                _ => expr.clone(),
                            };
                        }
                    }
                // Not a func_name call — recurse into head and args
                let new_head = Box::new(substitute_fn_refs(
                    head, func_name, var_name, current_i, computed,
                ));
                let new_args: Vec<Expr> = args
                    .iter()
                    .map(|a| substitute_fn_refs(a, func_name, var_name, current_i, computed))
                    .collect();
                Expr::Call {
                    head: new_head,
                    args: new_args,
                }
            }
            Expr::Call { head, args } => {
                let new_head = Box::new(substitute_fn_refs(
                    head, func_name, var_name, current_i, computed,
                ));
                let new_args: Vec<Expr> = args
                    .iter()
                    .map(|a| substitute_fn_refs(a, func_name, var_name, current_i, computed))
                    .collect();
                Expr::Call {
                    head: new_head,
                    args: new_args,
                }
            }
            Expr::List(items) => Expr::List(
                items
                    .iter()
                    .map(|i| substitute_fn_refs(i, func_name, var_name, current_i, computed))
                    .collect(),
            ),
            other => other.clone(),
        }
    }

    // Evaluate iterations
    let child_env = env.child();
    let mut results = Vec::new();

    for i in nmin..=nmax {
        if let Some(val) = computed.get(&i) {
            // Already computed (initial condition)
            results.push(val.clone());
        } else {
            // The recurrence is a[n+1] == body; to compute a[i], evaluate body at n = i-1
            let n_val = i - 1;
            let substituted_body =
                substitute_fn_refs(&body, &func_name, &var_name, n_val, &computed);
            child_env.set(var_name.clone(), Value::Integer(Integer::from(n_val)));
            let val = super::eval(&substituted_body, &child_env)?;
            computed.insert(i, val.clone());
            results.push(val);
        }
    }

    Ok(Value::List(results))
}

/// ParallelTable[expr, {i, ...}] — evaluate expr for each iterator value in parallel.
pub(super) fn eval_parallel_table(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ParallelTable requires exactly 2 arguments".to_string(),
        ));
    }

    let expr = &args[0];

    let iter_items = match &args[1] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "ParallelTable iterator spec must be a list".to_string(),
            ));
        }
    };

    let (var_name, values) = eval_iterator_spec(iter_items, env)?;

    // For small iteration counts, sequential is faster
    if values.len() < 4 {
        let child_env = env.child();
        let mut result = Vec::with_capacity(values.len());
        for val in values {
            child_env.set(var_name.clone(), val);
            result.push(super::eval(expr, &child_env)?);
        }
        return Ok(Value::List(result));
    }

    // Parallel evaluation using the thread pool (or sequential fallback)
    let jobs: Vec<Box<dyn FnOnce() -> Result<Value, EvalError> + Send>> = values
        .into_iter()
        .map(|val| {
            let expr = expr.clone();
            let var_name = var_name.clone();
            let env = env.clone();
            Box::new(move || {
                let child_env = env.child();
                child_env.set(var_name, val);
                super::eval(&expr, &child_env)
            }) as Box<dyn FnOnce() -> Result<Value, EvalError> + Send>
        })
        .collect();

    let results = crate::builtins::parallel::parallel_batch(jobs);
    let mut out = Vec::with_capacity(results.len());
    for r in results {
        out.push(r?);
    }
    Ok(Value::List(out))
}

/// ParallelSum[expr, {i, min, max}] — parallel version of Sum.
pub(super) fn eval_parallel_sum(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ParallelSum requires exactly 2 arguments".to_string(),
        ));
    }

    let iter_items = match &args[1] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "ParallelSum iterator spec must be a list".to_string(),
            ));
        }
    };

    let (var_name, values) = eval_iterator_spec(iter_items, env)?;

    if values.is_empty() {
        return Ok(Value::Integer(Integer::from(0)));
    }

    if values.len() < 8 {
        // Sequential for small iteration counts
        let child_env = env.child();
        let mut acc = Value::Integer(Integer::from(0));
        for val in values {
            child_env.set(var_name.clone(), val);
            let v = super::eval(&args[0], &child_env)?;
            acc = crate::builtins::add_values_public(&acc, &v)?;
        }
        return Ok(acc);
    }

    // Parallel: chunk values by number of workers
    let n_workers = crate::builtins::parallel::pool_size();
    let chunk_size = values.len().div_ceil(n_workers);

    let jobs: Vec<Box<dyn FnOnce() -> Result<Value, EvalError> + Send>> = values
        .chunks(chunk_size)
        .map(|chunk| {
            let expr = args[0].clone();
            let var_name = var_name.clone();
            let chunk_vec = chunk.to_vec();
            let env = env.clone();
            Box::new(move || {
                let child_env = env.child();
                let mut acc = Value::Integer(Integer::from(0));
                for val in chunk_vec {
                    child_env.set(var_name.clone(), val);
                    let v = super::eval(&expr, &child_env)?;
                    acc = crate::builtins::add_values_public(&acc, &v)?;
                }
                Ok(acc)
            }) as Box<dyn FnOnce() -> Result<Value, EvalError> + Send>
        })
        .collect();

    let results = crate::builtins::parallel::parallel_batch(jobs);
    let mut total = Value::Integer(Integer::from(0));
    for r in results {
        let v = r?;
        total = crate::builtins::add_values_public(&total, &v)?;
    }
    Ok(total)
}

/// ParallelEvaluate[expr] — evaluate expr on each worker, collecting results.
pub(super) fn eval_parallel_evaluate(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "ParallelEvaluate requires exactly 1 argument".to_string(),
        ));
    }

    let expr = &args[0];
    let n = crate::builtins::parallel::pool_size();

    if n <= 1 {
        // Single worker: just evaluate once
        return Ok(Value::List(vec![super::eval(expr, env)?]));
    }

    let jobs: Vec<Box<dyn FnOnce() -> Result<Value, EvalError> + Send>> = (0..n)
        .map(|_| {
            let expr = expr.clone();
            let env = env.clone();
            Box::new(move || super::eval(&expr, &env))
                as Box<dyn FnOnce() -> Result<Value, EvalError> + Send>
        })
        .collect();

    let results = crate::builtins::parallel::parallel_batch(jobs);
    let mut out = Vec::with_capacity(results.len());
    for r in results {
        out.push(r?);
    }
    Ok(Value::List(out))
}

/// ParallelTry[list] or ParallelTry[f, list] — evaluate in parallel, return first result.
pub(super) fn eval_parallel_try(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    let (items, f_val_opt) = match args.len() {
        1 => {
            let items = match &args[0] {
                Expr::List(items) => items,
                _ => {
                    return Err(EvalError::Error(
                        "ParallelTry[list] requires a list argument".to_string(),
                    ));
                }
            };
            (items, None)
        }
        2 => {
            let f_val = super::eval(&args[0], env)?;
            let items = match &args[1] {
                Expr::List(items) => items,
                _ => {
                    return Err(EvalError::Error(
                        "ParallelTry[f, list] requires a list as second argument".to_string(),
                    ));
                }
            };
            (items, Some(f_val))
        }
        _ => {
            return Err(EvalError::Error(
                "ParallelTry requires 1 or 2 arguments".to_string(),
            ));
        }
    };

    if items.is_empty() {
        return Err(EvalError::Error(
            "ParallelTry requires a non-empty list".to_string(),
        ));
    }

    // Sequential for small lists
    if items.len() < 4 {
        if let Some(f) = &f_val_opt {
            let first_arg = super::eval(&items[0], env)?;
            return super::apply_function(f, &[first_arg], env);
        }
        return super::eval(&items[0], env);
    }

    // Parallel: evaluate all, return first result
    let jobs: Vec<Box<dyn FnOnce() -> Result<Value, EvalError> + Send>> = items
        .iter()
        .map(|item| {
            let env = env.clone();
            let item = item.clone();
            let f = f_val_opt.clone();
            Box::new(move || {
                if let Some(f) = f {
                    let arg = super::eval(&item, &env)?;
                    super::apply_function(&f, &[arg], &env)
                } else {
                    super::eval(&item, &env)
                }
            }) as Box<dyn FnOnce() -> Result<Value, EvalError> + Send>
        })
        .collect();

    let results = crate::builtins::parallel::parallel_batch(jobs);
    results.into_iter().next().unwrap_or_else(|| {
        Err(EvalError::Error(
            "ParallelTry: no results available".to_string(),
        ))
    })
}

/// ParallelProduct[expr, {i, min, max}] — parallel version of product.
pub(super) fn eval_parallel_product(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ParallelProduct requires exactly 2 arguments".to_string(),
        ));
    }

    let iter_items = match &args[1] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "ParallelProduct iterator spec must be a list".to_string(),
            ));
        }
    };

    let (var_name, values) = eval_iterator_spec(iter_items, env)?;

    if values.is_empty() {
        return Ok(Value::Integer(Integer::from(1)));
    }

    if values.len() < 8 {
        // Sequential for small iteration counts
        let child_env = env.child();
        let mut acc = Value::Integer(Integer::from(1));
        for val in values {
            child_env.set(var_name.clone(), val);
            let v = super::eval(&args[0], &child_env)?;
            acc = crate::builtins::mul_values_public(&acc, &v)?;
        }
        return Ok(acc);
    }

    // Parallel: chunk values by number of workers
    let n_workers = crate::builtins::parallel::pool_size();
    let chunk_size = values.len().div_ceil(n_workers);

    let jobs: Vec<Box<dyn FnOnce() -> Result<Value, EvalError> + Send>> = values
        .chunks(chunk_size)
        .map(|chunk| {
            let expr = args[0].clone();
            let var_name = var_name.clone();
            let chunk_vec = chunk.to_vec();
            let env = env.clone();
            Box::new(move || {
                let child_env = env.child();
                let mut acc = Value::Integer(Integer::from(1));
                for val in chunk_vec {
                    child_env.set(var_name.clone(), val);
                    let v = super::eval(&expr, &child_env)?;
                    acc = crate::builtins::mul_values_public(&acc, &v)?;
                }
                Ok(acc)
            }) as Box<dyn FnOnce() -> Result<Value, EvalError> + Send>
        })
        .collect();

    let results = crate::builtins::parallel::parallel_batch(jobs);
    let mut total = Value::Integer(Integer::from(1));
    for r in results {
        let v = r?;
        total = crate::builtins::mul_values_public(&total, &v)?;
    }
    Ok(total)
}

/// ParallelDo[expr, {i, ...}] — evaluate expr for each iterator value in parallel,
/// discarding results and returning Null.
pub(super) fn eval_parallel_do(args: &[Expr], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "ParallelDo requires exactly 2 arguments".to_string(),
        ));
    }

    let expr = &args[0];

    let iter_items = match &args[1] {
        Expr::List(items) => items,
        _ => {
            return Err(EvalError::Error(
                "ParallelDo iterator spec must be a list".to_string(),
            ));
        }
    };

    let (var_name, values) = eval_iterator_spec(iter_items, env)?;

    // For small iteration counts, sequential is faster
    if values.len() < 4 {
        let child_env = env.child();
        for val in values {
            child_env.set(var_name.clone(), val);
            super::eval(expr, &child_env)?;
        }
        return Ok(Value::Null);
    }

    // Parallel evaluation — discard results, check errors
    let jobs: Vec<Box<dyn FnOnce() -> Result<Value, EvalError> + Send>> = values
        .into_iter()
        .map(|val| {
            let expr = expr.clone();
            let var_name = var_name.clone();
            let env = env.clone();
            Box::new(move || {
                let child_env = env.child();
                child_env.set(var_name, val);
                super::eval(&expr, &child_env)
            }) as Box<dyn FnOnce() -> Result<Value, EvalError> + Send>
        })
        .collect();

    let results = crate::builtins::parallel::parallel_batch(jobs);
    for r in results {
        r?; // propagate any error
    }
    Ok(Value::Null)
}
