/// Format and display builtins: InputForm, Short, Shallow, NumberForm, Grid, Defer, etc.
///
/// These builtins do not transform their argument; they wrap it in a
/// `Value::Formatted { format, ... }` that controls how the value displays.
/// The actual rendering is implemented in `value.rs`'s Display impl.
use crate::lexer;
use crate::parser;
use crate::value::{EvalError, Format, Value};

// ── Format wrappers ─────────────────────────────────────────────────────────────

/// InputForm[expr] — display expr in infix notation (e.g., `a + b` not `Plus[a, b]`).
pub fn builtin_input_form(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "InputForm requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::InputForm,
        value: Box::new(args[0].clone()),
    })
}

/// FullForm[expr] — display expr in head[arg, ...] notation (default display).
pub fn builtin_full_form(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "FullForm requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::FullForm,
        value: Box::new(args[0].clone()),
    })
}

/// StandardForm[expr] — display expr in StandardForm (infix with SeriesData special display).
pub fn builtin_standard_form(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "StandardForm requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::StandardForm,
        value: Box::new(args[0].clone()),
    })
}

/// OutputForm[expr] — display expr in OutputForm (plain-text, same as StandardForm for terminal).
pub fn builtin_output_form(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "OutputForm requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::OutputForm,
        value: Box::new(args[0].clone()),
    })
}

/// TeXForm[expr] — render expr as TeX source (displayed verbatim).
pub fn builtin_tex_form(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("TeXForm", args, 1)?;
    Ok(Value::Formatted {
        format: Format::Text,
        value: Box::new(Value::Str(render_tex(&args[0]))),
    })
}

// ── TeX rendering for TeXForm ────────────────────────────────────────────────

/// Escape TeX special characters in literal text.
fn escape_tex(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\textbackslash{}"),
            '{' => out.push_str("\\{"),
            '}' => out.push_str("\\}"),
            '_' => out.push_str("\\_"),
            '%' => out.push_str("\\%"),
            '&' => out.push_str("\\&"),
            '#' => out.push_str("\\#"),
            '$' => out.push_str("\\$"),
            '^' => out.push_str("\\^{}"),
            '~' => out.push_str("\\~{}"),
            _ => out.push(c),
        }
    }
    out
}

/// Render a symbol name, mapping a few well-known names to TeX commands.
fn tex_symbol(s: &str) -> String {
    match s {
        "Pi" | "pi" => "\\pi".to_string(),
        "Infinity" => "\\infty".to_string(),
        "Alpha" | "alpha" => "\\alpha".to_string(),
        "Beta" | "beta" => "\\beta".to_string(),
        "Gamma" | "gamma" => "\\gamma".to_string(),
        "Delta" | "delta" => "\\delta".to_string(),
        "Theta" | "theta" => "\\theta".to_string(),
        "Lambda" | "lambda" => "\\lambda".to_string(),
        "Mu" | "mu" => "\\mu".to_string(),
        "Sigma" | "sigma" => "\\sigma".to_string(),
        "Phi" | "phi" => "\\phi".to_string(),
        "Omega" | "omega" => "\\omega".to_string(),
        _ => escape_tex(s),
    }
}

/// Render `v` for TeX, parenthesizing low-precedence subexpressions.
fn tex_operand(v: &Value) -> String {
    let compound = matches!(
        v,
        Value::Call { head, .. }
            if matches!(
                head.as_str(),
                "Plus" | "Equal" | "Unequal" | "Less" | "Greater" | "LessEqual"
                    | "GreaterEqual" | "And" | "Or" | "Rule" | "RuleDelayed"
            )
    );
    if compound {
        format!("\\left({}\\right)", render_tex(v))
    } else {
        render_tex(v)
    }
}

/// Render a value as TeX source.
fn render_tex(v: &Value) -> String {
    match v {
        Value::Integer(n) => n.to_string(),
        Value::Rational(r) => format!("\\frac{{{}}}{{{}}}", r.numer(), r.denom()),
        Value::Real(r) => Value::Real(r.clone()).to_string(),
        Value::Bool(b) => format!("\\text{{{}}}", if *b { "True" } else { "False" }),
        Value::Null => "\\text{Null}".to_string(),
        Value::Str(s) => format!("\\text{{{}}}", escape_tex(s)),
        Value::Symbol(s) => tex_symbol(s),
        Value::List(items) => format!(
            "\\left\\{{{}\\right\\}}",
            items.iter().map(render_tex).collect::<Vec<_>>().join(", ")
        ),
        Value::Formatted { value, .. } => render_tex(value),
        Value::Call { head, args } => render_tex_call(head, args),
        other => escape_tex(&other.to_string()),
    }
}

/// Render an infix call as TeX; unknown heads use function-call notation.
fn render_tex_call(head: &str, args: &[Value]) -> String {
    let join = |sep: &str| -> String { args.iter().map(tex_operand).collect::<Vec<_>>().join(sep) };
    match head {
        "List" => format!(
            "\\left\\{{{}\\right\\}}",
            args.iter().map(render_tex).collect::<Vec<_>>().join(", ")
        ),
        "Plus" if args.len() >= 2 => join(" + "),
        "Times" if args.len() >= 2 => join(" "),
        "Power" if args.len() == 2 => {
            format!("{}^{{{}}}", tex_operand(&args[0]), render_tex(&args[1]))
        }
        "Divide" | "Rational" if args.len() == 2 => format!(
            "\\frac{{{}}}{{{}}}",
            render_tex(&args[0]),
            render_tex(&args[1])
        ),
        "Minus" if args.len() == 1 => format!("-{}", tex_operand(&args[0])),
        "Equal" if args.len() == 2 => join(" = "),
        "Unequal" if args.len() == 2 => join(" \\neq "),
        "Less" if args.len() == 2 => join(" < "),
        "Greater" if args.len() == 2 => join(" > "),
        "LessEqual" if args.len() == 2 => join(" \\leq "),
        "GreaterEqual" if args.len() == 2 => join(" \\geq "),
        "And" if args.len() >= 2 => join(" \\land "),
        "Or" if args.len() >= 2 => join(" \\lor "),
        "Not" if args.len() == 1 => format!("\\lnot {}", tex_operand(&args[0])),
        "Rule" | "RuleDelayed" if args.len() == 2 => join(" \\to "),
        "Sqrt" if args.len() == 1 => format!("\\sqrt{{{}}}", render_tex(&args[0])),
        "Abs" if args.len() == 1 => format!("\\left|{}\\right|", render_tex(&args[0])),
        _ => format!(
            "{}\\left({}\\right)",
            tex_symbol(head),
            args.iter().map(render_tex).collect::<Vec<_>>().join(", ")
        ),
    }
}

/// Short[expr] — display with top-level truncation (default 5 items).
/// Short[expr, n] — display with at most n top-level items.
pub fn builtin_short(args: &[Value]) -> Result<Value, EvalError> {
    let n = match args.len() {
        1 => 5usize,
        2 => {
            let v = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "integer".to_string(),
                got: args[1].type_name().to_string(),
            })?;
            if v < 1 {
                return Err(EvalError::Error(
                    "Short[expr, n] requires n >= 1".to_string(),
                ));
            }
            v as usize
        }
        _ => {
            return Err(EvalError::Error(
                "Short requires 1 or 2 arguments".to_string(),
            ));
        }
    };
    Ok(Value::Formatted {
        format: Format::Short(n),
        value: Box::new(args[0].clone()),
    })
}

/// Shallow[expr] — display with limited nesting depth (default 3).
/// Shallow[expr, depth] — display with at most `depth` nesting levels.
pub fn builtin_shallow(args: &[Value]) -> Result<Value, EvalError> {
    let depth = match args.len() {
        1 => 3usize,
        2 => {
            let v = args[1].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: "integer".to_string(),
                got: args[1].type_name().to_string(),
            })?;
            if v < 1 {
                return Err(EvalError::Error(
                    "Shallow[expr, n] requires n >= 1".to_string(),
                ));
            }
            v as usize
        }
        _ => {
            return Err(EvalError::Error(
                "Shallow requires 1 or 2 arguments".to_string(),
            ));
        }
    };
    Ok(Value::Formatted {
        format: Format::Shallow(depth),
        value: Box::new(args[0].clone()),
    })
}

/// Parse the digit-count argument shared by `NumberForm`/`ScientificForm`.
///
/// Accepts an integer `n` or a `{n, f}` pair; only `n` significant digits are
/// currently rendered by the display layer, but validating and unwrapping the
/// pair keeps `NumberForm[expr, {n, f}]` from silently falling back to 6.
fn parse_digit_spec(spec: &Value, name: &str) -> Result<usize, EvalError> {
    let n = match spec {
        Value::List(pair) if !pair.is_empty() => {
            pair[0].to_integer().ok_or_else(|| EvalError::TypeError {
                expected: format!("integer, got {}", pair[0].type_name()),
                got: name.to_string(),
            })?
        }
        other => other.to_integer().ok_or_else(|| EvalError::TypeError {
            expected: "integer or {n, f}".to_string(),
            got: other.type_name().to_string(),
        })?,
    };
    if n < 1 {
        return Err(EvalError::Error(format!("{name} requires n >= 1")));
    }
    Ok(n as usize)
}

/// NumberForm[expr] / NumberForm[expr, n] / NumberForm[expr, {n, f}]
/// — display numbers with `n` significant digits (default 6).
pub fn builtin_number_form(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "NumberForm requires 1 or 2 arguments: NumberForm[expr, n]".to_string(),
        ));
    }
    let digits = if args.len() == 2 {
        parse_digit_spec(&args[1], "NumberForm")?
    } else {
        6
    };
    Ok(Value::Formatted {
        format: Format::NumberForm { digits },
        value: Box::new(args[0].clone()),
    })
}

/// ScientificForm[expr] / ScientificForm[expr, n] / ScientificForm[expr, {n, f}]
/// — display numbers in scientific notation with `n` significant digits.
pub fn builtin_scientific_form(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "ScientificForm requires 1 or 2 arguments: ScientificForm[expr, n]".to_string(),
        ));
    }
    let digits = if args.len() == 2 {
        parse_digit_spec(&args[1], "ScientificForm")?
    } else {
        6
    };
    Ok(Value::Formatted {
        format: Format::ScientificForm { digits },
        value: Box::new(args[0].clone()),
    })
}

/// BaseForm[expr, base] — display a number in the given base (2–36).
pub fn builtin_base_form(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "BaseForm requires exactly 2 arguments: BaseForm[expr, base]".to_string(),
        ));
    }
    let base_val = args[1].to_integer().unwrap_or(10);
    if !(2..=36).contains(&base_val) {
        return Err(EvalError::Error(
            "BaseForm[expr, base] requires base between 2 and 36".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::BaseForm(base_val as u32),
        value: Box::new(args[0].clone()),
    })
}

/// Grid[list] — display a 2D list as an aligned table grid.
pub fn builtin_grid(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Grid requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::Grid,
        value: Box::new(args[0].clone()),
    })
}

/// Defer[expr] — display expr without evaluation.
/// Note: In the current evaluator, arguments are evaluated before the builtin
/// is called, so this acts as a display wrapper rather than a true hold.
/// For a true hold, use Hold[expr] or HoldComplete[expr].
pub fn builtin_defer(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Defer requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Formatted {
        format: Format::Deferred,
        value: Box::new(args[0].clone()),
    })
}

/// SyntaxQ[expr] — returns True if expr is valid Syma syntax, False otherwise.
/// Performs lex + parse only (no evaluation).
pub fn builtin_syntax_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "SyntaxQ requires exactly 1 argument".to_string(),
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => s.clone(),
        other => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };
    match lexer::tokenize(&s) {
        Ok(tokens) => match parser::parse(tokens) {
            Ok(_) => Ok(Value::Bool(true)),
            Err(_) => Ok(Value::Bool(false)),
        },
        Err(_) => Ok(Value::Bool(false)),
    }
}

/// SyntaxLength[expr] — returns the position (as an Integer) of the first syntax error,
/// or the length of the expression string if it is valid.
pub fn builtin_syntax_length(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "SyntaxLength requires exactly 1 argument".to_string(),
        ));
    }
    let s = match &args[0] {
        Value::Str(s) => s.clone(),
        other => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };
    let len = s.len();
    match lexer::tokenize(&s) {
        Ok(tokens) => match parser::parse(tokens) {
            Ok(_) => Ok(Value::Integer(rug::Integer::from(len))),
            Err(e) => {
                // ParseError carries a line/column span; report the column
                // (1-based) of the failure, or 0 when unavailable.
                let pos = e.span.as_ref().map(|sp| sp.col as i64).unwrap_or(0);
                Ok(Value::Integer(rug::Integer::from(pos)))
            }
        },
        Err(e) => {
            // Lexer error includes position info
            Ok(Value::Integer(rug::Integer::from(e.pos)))
        }
    }
}

/// Render a value as a single-line cell string for table layout.
fn cell_string(v: &Value) -> String {
    format!("{v}")
}

/// Left- or right-align a grid of cell strings, two spaces between columns.
/// Trailing padding on each line is trimmed.
fn align_columns(rows: &[Vec<String>], right: bool) -> String {
    let max_cols = rows.iter().map(|r| r.len()).max().unwrap_or(0);
    let mut widths = vec![0usize; max_cols];
    for row in rows {
        for (j, cell) in row.iter().enumerate() {
            widths[j] = widths[j].max(cell.chars().count());
        }
    }
    rows.iter()
        .map(|row| {
            let mut line = String::new();
            for (j, cell) in row.iter().enumerate() {
                if j > 0 {
                    line.push_str("  ");
                }
                let pad = widths
                    .get(j)
                    .copied()
                    .unwrap_or(0)
                    .saturating_sub(cell.chars().count());
                if right {
                    line.push_str(&" ".repeat(pad));
                    line.push_str(cell);
                } else {
                    line.push_str(cell);
                    line.push_str(&" ".repeat(pad));
                }
            }
            line.trim_end().to_string()
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// TableForm[list] — format a list as a table.
///
/// A 1D list prints one element per line; a 2D list (list of lists) left-aligns
/// columns with two spaces between them.
pub fn builtin_table_form(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("TableForm", args, 1)?;
    let text = match &args[0] {
        Value::List(rows) => {
            let row_strs: Vec<Vec<String>> = rows
                .iter()
                .map(|row| match row {
                    Value::List(cols) => cols.iter().map(cell_string).collect(),
                    other => vec![cell_string(other)],
                })
                .collect();
            if row_strs.iter().all(|r| r.len() == 1) {
                row_strs
                    .iter()
                    .map(|r| r[0].clone())
                    .collect::<Vec<_>>()
                    .join("\n")
            } else {
                align_columns(&row_strs, false)
            }
        }
        other => cell_string(other),
    };
    Ok(Value::Formatted {
        format: Format::Text,
        value: Box::new(Value::Str(text)),
    })
}

/// MatrixForm[list] — format a 2D list as a matrix with brackets.
/// Each row on its own line, numeric columns right-aligned.
pub fn builtin_matrix_form(args: &[Value]) -> Result<Value, EvalError> {
    super::require_args("MatrixForm", args, 1)?;
    let text = match &args[0] {
        Value::List(rows) => {
            let row_strs: Vec<Vec<String>> = rows
                .iter()
                .map(|row| match row {
                    Value::List(cols) => cols.iter().map(cell_string).collect(),
                    other => vec![cell_string(other)],
                })
                .collect();
            let body = align_columns(&row_strs, true);
            let lines: Vec<&str> = body.split('\n').collect();
            let max_width = lines.iter().map(|l| l.chars().count()).max().unwrap_or(0);
            let n_rows = lines.len();
            let mut result = String::new();
            for (i, line) in lines.iter().enumerate() {
                let pad = max_width - line.chars().count();
                let padded = format!("{line}{}", " ".repeat(pad));
                if n_rows == 1 {
                    result.push_str(&format!("( {padded} )"));
                } else if i == 0 {
                    result.push_str(&format!("\u{23a1} {padded} \u{23a4}"));
                } else if i == n_rows - 1 {
                    result.push_str(&format!("\u{23a3} {padded} \u{23a6}"));
                } else {
                    result.push_str(&format!("\u{23a6} {padded} \u{23a9}"));
                }
                if i < n_rows - 1 {
                    result.push('\n');
                }
            }
            result
        }
        other => cell_string(other),
    };
    Ok(Value::Formatted {
        format: Format::Text,
        value: Box::new(Value::Str(text)),
    })
}

/// PaddedForm[expr, n] — format number padded to n digits total.
/// PaddedForm[expr, {n, f}] — n digits total, f after decimal.
pub fn builtin_padded_form(args: &[Value]) -> Result<Value, EvalError> {
    super::require_min_args("PaddedForm", args, 2)?;
    let val = &args[0];
    let (total_width, frac_digits) = match &args[1] {
        Value::Integer(n) => {
            let w = n.to_i32().unwrap_or(6);
            (w as usize, None)
        }
        Value::List(pair) if pair.len() == 2 => {
            let w = super::require_f64(&pair[0], "PaddedForm", 2)? as usize;
            let f = super::require_f64(&pair[1], "PaddedForm", 2)? as usize;
            (w, Some(f))
        }
        _ => {
            return Err(EvalError::TypeError {
                expected: "integer or {n, f}".to_string(),
                got: args[1].type_name().to_string(),
            });
        }
    };

    let formatted = match (val, frac_digits) {
        (Value::Integer(n), Some(f)) => {
            let s = n.to_string();
            if f == 0 {
                format!("{s:>total_width$}")
            } else {
                let frac_part = "0".repeat(f);
                let full = format!("{s}.{frac_part}");
                format!("{full:>total_width$}")
            }
        }
        (Value::Integer(n), None) => {
            let s = n.to_string();
            format!("{s:>total_width$}")
        }
        (Value::Real(r), Some(f)) => {
            format!("{r:>total_width$.f$}")
        }
        (Value::Real(r), None) => {
            let s = format!("{}", r.to_f64());
            format!("{s:>total_width$}")
        }
        (Value::Rational(r), Some(f)) => {
            let v = r.to_f64();
            format!("{v:>total_width$.f$}")
        }
        (Value::Rational(r), None) => {
            let s = format!("{}", r.to_f64());
            format!("{s:>total_width$}")
        }
        (other, None) => {
            let s = format!("{other}");
            format!("{s:>total_width$}")
        }
        (other, Some(_)) => {
            let s = format!("{other}");
            format!("{s:>total_width$}")
        }
    };
    Ok(Value::Formatted {
        format: Format::Text,
        value: Box::new(Value::Str(formatted)),
    })
}

/// StringForm["template", args...] — printf-style string formatting.
/// Replaces `1`, `2`, etc. in the template with corresponding arguments.
pub fn builtin_string_form(args: &[Value]) -> Result<Value, EvalError> {
    super::require_min_args("StringForm", args, 1)?;
    let template = match &args[0] {
        Value::Str(s) => s.clone(),
        other => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: other.type_name().to_string(),
            });
        }
    };

    let mut result = String::new();
    let chars: Vec<char> = template.chars().collect();
    let len = chars.len();
    let mut i = 0;
    while i < len {
        if chars[i] == '`' && i + 1 < len {
            // Try to parse a numbered slot: `1`, `2`, etc.
            let mut num_str = String::new();
            let mut j = i + 1;
            while j < len && chars[j].is_ascii_digit() {
                num_str.push(chars[j]);
                j += 1;
            }
            if !num_str.is_empty() && j < len && chars[j] == '`' {
                // Valid slot like `1`
                if let Ok(idx) = num_str.parse::<usize>() {
                    if idx >= 1 && idx < args.len() {
                        // For strings, insert the raw content without quotes;
                        // for other types, use standard Display formatting.
                        match &args[idx] {
                            Value::Str(s) => result.push_str(s),
                            other => result.push_str(&format!("{}", other)),
                        }
                    } else {
                        // Out of range: leave the slot as-is and warn.
                        crate::messages::emit("StringForm::sfr", &[num_str.clone()]);
                        result.push('`');
                        result.push_str(&num_str);
                        result.push('`');
                    }
                }
                i = j + 1; // skip past closing backtick
            } else {
                // Not a valid slot, just output the backtick
                result.push(chars[i]);
                i += 1;
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    Ok(Value::Formatted {
        format: Format::Text,
        value: Box::new(Value::Str(result)),
    })
}

// ── Symbol list for lazy package loading ─────────────────────────────────────────

/// All symbols provided by this module.
pub const SYMBOLS: &[&str] = &[
    "InputForm",
    "FullForm",
    "StandardForm",
    "OutputForm",
    "Short",
    "Shallow",
    "NumberForm",
    "ScientificForm",
    "BaseForm",
    "Grid",
    "Defer",
    "SyntaxQ",
    "SyntaxLength",
    "TableForm",
    "MatrixForm",
    "PaddedForm",
    "StringForm",
    "TeXForm",
];

/// Register all format builtins in the environment.
pub fn register(env: &crate::env::Env) {
    use super::register_builtin;
    register_builtin(env, "InputForm", builtin_input_form);
    register_builtin(env, "FullForm", builtin_full_form);
    register_builtin(env, "StandardForm", builtin_standard_form);
    register_builtin(env, "OutputForm", builtin_output_form);
    register_builtin(env, "Short", builtin_short);
    register_builtin(env, "Shallow", builtin_shallow);
    register_builtin(env, "NumberForm", builtin_number_form);
    register_builtin(env, "ScientificForm", builtin_scientific_form);
    register_builtin(env, "BaseForm", builtin_base_form);
    register_builtin(env, "Grid", builtin_grid);
    register_builtin(env, "Defer", builtin_defer);
    register_builtin(env, "SyntaxQ", builtin_syntax_q);
    register_builtin(env, "SyntaxLength", builtin_syntax_length);
    register_builtin(env, "TableForm", builtin_table_form);
    register_builtin(env, "MatrixForm", builtin_matrix_form);
    register_builtin(env, "PaddedForm", builtin_padded_form);
    register_builtin(env, "StringForm", builtin_string_form);
    register_builtin(env, "TeXForm", builtin_tex_form);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::DEFAULT_PRECISION;
    use rug::Float;

    #[allow(dead_code)]
    fn eval_format(expr: &str) -> Value {
        let env = crate::env::Env::new();
        crate::builtins::register_builtins(&env);
        crate::eval_input(expr, &env).unwrap_or(Value::Null)
    }

    // ── InputForm round-trip helpers ──

    fn sym(s: &str) -> Value {
        Value::Symbol(s.to_string())
    }

    fn int(n: i64) -> Value {
        Value::Integer(rug::Integer::from(n))
    }

    fn call(head: &str, args: Vec<Value>) -> Value {
        Value::Call {
            head: head.to_string(),
            args,
        }
    }

    fn render_as(format: Format, v: &Value) -> String {
        format!(
            "{}",
            Value::Formatted {
                format,
                value: Box::new(v.clone()),
            }
        )
    }

    /// Evaluate a source string without the stderr noise of `eval_input`.
    fn eval_str(src: &str) -> Value {
        let tokens = crate::lexer::tokenize(src).expect("lex");
        let exprs = crate::parser::parse(tokens).expect("parse");
        let env = crate::env::Env::new();
        crate::builtins::register_builtins(&env);
        let mut last = Value::Null;
        for e in &exprs {
            last = crate::eval::eval(e, &env).expect("eval");
        }
        last
    }

    /// Assert that `v` renders exactly as `expected` and that re-parsing that
    /// rendering evaluates to the same value as re-parsing `FullForm[v]`.
    fn assert_input_form(v: Value, expected: &str) {
        assert_renders(v.clone(), expected);
        let rendered = render_as(Format::InputForm, &v);
        let via_input = eval_str(&rendered);
        let via_full = eval_str(&render_as(Format::FullForm, &v));
        assert_eq!(
            via_input.to_string(),
            via_full.to_string(),
            "InputForm `{rendered}` did not round-trip"
        );
    }

    /// Exact-string assertion only (used where evaluation leaves `Pattern[…]`
    /// wrappers that make the eval comparison representation-dependent).
    fn assert_renders(v: Value, expected: &str) {
        assert_eq!(
            render_as(Format::InputForm, &v),
            expected,
            "InputForm mismatch"
        );
    }

    // ── InputForm precedence / round-trip ──

    #[test]
    fn test_input_form_power_precedence() {
        // Parenthesize a `Power` base but not a `Power` exponent.
        assert_input_form(
            call(
                "Power",
                vec![call("Power", vec![sym("x"), sym("y")]), sym("z")],
            ),
            "(x^y)^z",
        );
        assert_input_form(
            call(
                "Power",
                vec![sym("x"), call("Power", vec![sym("y"), sym("z")])],
            ),
            "x^y^z",
        );
        assert_input_form(
            call(
                "Power",
                vec![call("Plus", vec![sym("a"), sym("b")]), int(2)],
            ),
            "(a + b)^2",
        );
        assert_input_form(
            call(
                "Power",
                vec![sym("x"), call("Plus", vec![sym("a"), sym("b")])],
            ),
            "x^(a + b)",
        );
    }

    #[test]
    fn test_input_form_times_plus_precedence() {
        assert_input_form(
            call(
                "Times",
                vec![call("Plus", vec![sym("a"), sym("b")]), sym("c")],
            ),
            "(a + b) c",
        );
        assert_input_form(
            call(
                "Times",
                vec![sym("a"), call("Plus", vec![sym("b"), sym("c")])],
            ),
            "a (b + c)",
        );
        // A nested negative factor must be parenthesized (`a -b` would be a sum).
        assert_input_form(
            call(
                "Times",
                vec![sym("a"), call("Times", vec![int(-1), sym("b")])],
            ),
            "a (-b)",
        );
    }

    #[test]
    fn test_input_form_negative_signs() {
        assert_input_form(call("Times", vec![int(-1), sym("x")]), "-x");
        assert_input_form(call("Times", vec![int(-2), sym("x"), sym("y")]), "-2 x y");
        assert_input_form(
            call(
                "Plus",
                vec![sym("a"), call("Times", vec![int(-1), sym("b")])],
            ),
            "a - b",
        );
        assert_input_form(
            call("Plus", vec![int(1), call("Times", vec![int(-2), sym("x")])]),
            "1 - 2 x",
        );
    }

    #[test]
    fn test_input_form_not_and_logic_precedence() {
        assert_input_form(call("Not", vec![sym("a")]), "!a");
        assert_input_form(
            call("Not", vec![call("Equal", vec![sym("a"), sym("b")])]),
            "!(a == b)",
        );
        assert_input_form(
            call("Not", vec![call("Plus", vec![sym("a"), sym("b")])]),
            "!(a + b)",
        );
        assert_input_form(
            call("And", vec![call("Or", vec![sym("a"), sym("b")]), sym("c")]),
            "(a || b) && c",
        );
        assert_input_form(
            call("Or", vec![sym("a"), call("And", vec![sym("b"), sym("c")])]),
            "a || b && c",
        );
        assert_input_form(
            call(
                "Times",
                vec![call("Equal", vec![sym("a"), sym("b")]), sym("c")],
            ),
            "(a == b) c",
        );
    }

    #[test]
    fn test_input_form_rule_divide_part_precedence() {
        assert_renders(
            call(
                "Rule",
                vec![call("Rule", vec![sym("a"), sym("b")]), sym("c")],
            ),
            "(a -> b) -> c",
        );
        assert_renders(
            call("Rule", vec![sym("a"), call("Or", vec![sym("b"), sym("c")])]),
            "a -> b || c",
        );
        assert_input_form(
            call(
                "Divide",
                vec![call("Divide", vec![sym("x"), sym("y")]), sym("z")],
            ),
            "x/y/z",
        );
        assert_input_form(
            call(
                "Divide",
                vec![sym("x"), call("Divide", vec![sym("y"), sym("z")])],
            ),
            "x/(y/z)",
        );
        assert_input_form(
            call("Part", vec![call("Plus", vec![sym("a"), sym("b")]), int(1)]),
            "(a + b)[[1]]",
        );
    }

    #[test]
    fn test_input_form_symmetric_difference_regression() {
        // Regression: `Minus[Plus[a,b]]` used to render as `-a + b`.
        let v = call("Minus", vec![call("Plus", vec![sym("a"), sym("b")])]);
        assert_eq!(render_as(Format::InputForm, &v), "-(a + b)");
    }

    // ── NumberForm / ScientificForm specs ──

    #[test]
    fn test_number_form_default_and_pair_spec() {
        let pi = Value::Real(Float::with_val(DEFAULT_PRECISION, std::f64::consts::PI));
        // Single-argument form defaults to 6 significant digits.
        assert!(builtin_number_form(&[pi.clone()]).is_ok());
        // Pair spec uses its first element as the significant-digit count.
        let spec = Value::List(vec![int(3), int(2)]);
        let s = builtin_number_form(&[pi.clone(), spec])
            .unwrap()
            .to_string();
        assert!(
            s.starts_with("3.14"),
            "expected 3 significant digits, got {s}"
        );
        // Invalid specs error rather than silently defaulting.
        assert!(builtin_number_form(&[pi.clone(), sym("x")]).is_err());
        assert!(builtin_number_form(&[pi, int(0)]).is_err());
    }

    #[test]
    fn test_scientific_form_pair_spec() {
        let v = Value::Real(Float::with_val(DEFAULT_PRECISION, 123.456));
        let spec = Value::List(vec![int(4), int(2)]);
        let s = builtin_scientific_form(&[v, spec]).unwrap().to_string();
        assert!(
            s.contains('\u{b7}'),
            "scientific form should use ·, got {s}"
        );
        assert!(
            s.contains("10^"),
            "scientific form should use base-10 power, got {s}"
        );
    }

    #[test]
    fn test_scientific_form_normalizes() {
        // Regression: rug's `e` exponent used to leak through (`1.23e3·10^0`).
        let big = Value::Real(Float::with_val(DEFAULT_PRECISION, 1234.5));
        assert_eq!(
            builtin_scientific_form(&[big, int(3)]).unwrap().to_string(),
            "1.23\u{b7}10^3"
        );
        let small = Value::Real(Float::with_val(DEFAULT_PRECISION, 0.000123));
        assert_eq!(
            builtin_scientific_form(&[small, int(3)])
                .unwrap()
                .to_string(),
            "1.23\u{b7}10^-4"
        );
        // Integer rounding carries into the exponent.
        assert_eq!(
            builtin_scientific_form(&[int(999), int(2)])
                .unwrap()
                .to_string(),
            "1.0\u{b7}10^3"
        );
        assert_eq!(
            builtin_scientific_form(&[int(-150), int(3)])
                .unwrap()
                .to_string(),
            "-1.50\u{b7}10^2"
        );
    }

    // ── TableForm / MatrixForm ──

    #[test]
    fn test_table_form_aligns_ragged_columns() {
        let v = Value::List(vec![
            Value::List(vec![int(1), int(100)]),
            Value::List(vec![int(20), int(3)]),
        ]);
        let s = builtin_table_form(&[v]).unwrap().to_string();
        let lines: Vec<&str> = s.split('\n').collect();
        assert_eq!(lines.len(), 2);
        // Column start positions line up across rows.
        assert_eq!(lines[0].find("100"), lines[1].find('3'));
    }

    #[test]
    fn test_matrix_form_returns_text_without_quotes() {
        let v = Value::List(vec![
            Value::List(vec![int(1), int(2)]),
            Value::List(vec![int(3), int(4)]),
        ]);
        let out = builtin_matrix_form(&[v]).unwrap();
        assert!(matches!(
            out,
            Value::Formatted {
                format: Format::Text,
                ..
            }
        ));
        let s = out.to_string();
        assert!(
            !s.starts_with('"'),
            "MatrixForm should be plain text: {s:?}"
        );
        assert!(s.contains('\u{23a1}'));
    }

    #[test]
    fn test_matrix_form_single_row_parenthesized() {
        let v = Value::List(vec![Value::List(vec![int(1), int(2), int(3)])]);
        let s = builtin_matrix_form(&[v]).unwrap().to_string();
        assert!(s.starts_with('(') && s.ends_with(')'), "got {s:?}");
    }

    // ── Argument validation ──

    #[test]
    fn test_short_shallow_require_integer() {
        let list = Value::List(vec![int(1), int(2), int(3)]);
        assert!(builtin_short(&[list.clone(), sym("x")]).is_err());
        assert!(builtin_shallow(&[list.clone(), sym("x")]).is_err());
        assert!(builtin_short(&[list.clone(), int(1)]).is_ok());
        assert!(builtin_shallow(&[list, int(2)]).is_ok());
    }

    #[test]
    fn test_base_form_range() {
        assert!(builtin_base_form(&[int(255), int(1)]).is_err());
        assert!(builtin_base_form(&[int(255), int(37)]).is_err());
        assert!(builtin_base_form(&[int(255), int(2)]).is_ok());
        assert_eq!(
            builtin_base_form(&[int(255), int(16)]).unwrap().to_string(),
            "ff(base 16)"
        );
    }

    #[test]
    fn test_string_form_out_of_range_is_left_verbatim() {
        let out = builtin_string_form(&[Value::Str("a=`2`".to_string()), int(1)]).unwrap();
        assert_eq!(out.to_string(), "a=`2`");
    }

    #[test]
    fn test_short_builtin() {
        // Short on a long list
        let v = Value::List(
            (0..20)
                .map(|i| Value::Integer(rug::Integer::from(i)))
                .collect(),
        );
        let result = builtin_short(&[v]).unwrap();
        let s = result.to_string();
        assert!(s.contains("<<"));
        assert!(s.contains("15>>")); // 20 - 5 = 15 truncated
    }

    #[test]
    fn test_short_with_n() {
        let v = Value::List(
            (0..20)
                .map(|i| Value::Integer(rug::Integer::from(i)))
                .collect(),
        );
        let n = Value::Integer(rug::Integer::from(3));
        let result = builtin_short(&[v, n]).unwrap();
        let s = result.to_string();
        assert!(s.contains("<<17>>"));
    }

    #[test]
    fn test_input_form_plus() {
        // InputForm should show infix notation
        let v = Value::Call {
            head: "Plus".to_string(),
            args: vec![
                Value::Integer(rug::Integer::from(1)),
                Value::Integer(rug::Integer::from(2)),
                Value::Integer(rug::Integer::from(3)),
            ],
        };
        let result = builtin_input_form(&[v]).unwrap();
        let s = result.to_string();
        assert_eq!(s, "1 + 2 + 3");
    }

    #[test]
    fn test_input_form_times() {
        let v = Value::Call {
            head: "Times".to_string(),
            args: vec![
                Value::Integer(rug::Integer::from(2)),
                Value::Symbol("x".to_string()),
            ],
        };
        let result = builtin_input_form(&[v]).unwrap();
        let s = result.to_string();
        assert_eq!(s, "2 x");
    }

    #[test]
    fn test_input_form_power() {
        let v = Value::Call {
            head: "Power".to_string(),
            args: vec![
                Value::Symbol("x".to_string()),
                Value::Integer(rug::Integer::from(2)),
            ],
        };
        let result = builtin_input_form(&[v]).unwrap();
        let s = result.to_string();
        assert_eq!(s, "x^2");
    }

    #[test]
    fn test_syntax_q() {
        let valid = Value::Str("1 + 2".to_string());
        assert_eq!(builtin_syntax_q(&[valid]).unwrap(), Value::Bool(true));

        let invalid = Value::Str("1 + ".to_string());
        assert_eq!(builtin_syntax_q(&[invalid]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_syntax_length() {
        let valid = Value::Str("1 + 2".to_string());
        let result = builtin_syntax_length(&[valid]).unwrap();
        assert_eq!(result.to_integer(), Some(5));

        // Invalid syntax: parse errors return 0 (no position info in ParseError)
        let invalid = Value::Str("1 + ".to_string());
        let result = builtin_syntax_length(&[invalid]).unwrap();
        // Should return some position (even 0) without panicking
        let _pos = result.to_integer().unwrap_or(0);
    }

    #[test]
    fn test_number_form() {
        let v = Value::Real(Float::with_val(DEFAULT_PRECISION, std::f64::consts::PI));
        let n = Value::Integer(rug::Integer::from(4));
        let result = builtin_number_form(&[v, n]).unwrap();
        let s = result.to_string();
        assert!(
            s.starts_with("3.14"),
            "NumberForm[Pi, 4] should start with 3.14, got: {}",
            s
        );
    }

    #[test]
    fn test_base_form() {
        let v = Value::Integer(rug::Integer::from(255));
        let base = Value::Integer(rug::Integer::from(16));
        let result = builtin_base_form(&[v, base]).unwrap();
        let s = result.to_string();
        assert_eq!(s, "ff(base 16)");
    }

    #[test]
    fn test_grid() {
        let v = Value::List(vec![
            Value::List(vec![
                Value::Integer(rug::Integer::from(1)),
                Value::Integer(rug::Integer::from(2)),
                Value::Integer(rug::Integer::from(3)),
            ]),
            Value::List(vec![
                Value::Integer(rug::Integer::from(10)),
                Value::Integer(rug::Integer::from(20)),
                Value::Integer(rug::Integer::from(30)),
            ]),
        ]);
        let result = builtin_grid(&[v]).unwrap();
        let s = result.to_string();
        // Grid should contain aligned columns (1 2 3 on one line, 10 20 30 on another)
        assert!(s.contains("1"));
        assert!(s.contains("10"));
        assert!(s.contains("20"));
        assert!(s.contains("30"));
    }

    #[test]
    fn test_input_form_compound() {
        // Test that InputForm renders 1 + 2*x correctly
        let v = Value::Call {
            head: "Plus".to_string(),
            args: vec![
                Value::Integer(rug::Integer::from(1)),
                Value::Call {
                    head: "Times".to_string(),
                    args: vec![
                        Value::Integer(rug::Integer::from(2)),
                        Value::Symbol("x".to_string()),
                    ],
                },
            ],
        };
        let result = builtin_input_form(&[v]).unwrap();
        let s = result.to_string();
        assert_eq!(s, "1 + 2 x");
    }

    #[test]
    fn test_shallow_builtin() {
        // 4 levels of nesting, Shallow default depth 3 → innermost truncated
        let inner = Value::List(vec![Value::List(vec![Value::List(vec![Value::List(
            vec![Value::Integer(rug::Integer::from(42))],
        )])])]);
        let result = builtin_shallow(&[inner]).unwrap();
        let s = result.to_string();
        // At depth 3 of 4, should show <<...>> for innermost
        assert!(
            s.contains("<<...>>"),
            "Shallow output should contain <<...>>, got: {s}"
        );
    }

    // ── TableForm tests ──

    #[test]
    fn test_table_form_1d() {
        let v = Value::List(vec![
            Value::Integer(rug::Integer::from(1)),
            Value::Integer(rug::Integer::from(2)),
            Value::Integer(rug::Integer::from(3)),
        ]);
        let result = builtin_table_form(&[v]).unwrap();
        let s = result.to_string();
        assert!(s.contains("1"));
        assert!(s.contains("2"));
        assert!(s.contains("3"));
        // Each on its own line
        assert!(s.contains('\n'));
    }

    #[test]
    fn test_table_form_2d() {
        let v = Value::List(vec![
            Value::List(vec![
                Value::Integer(rug::Integer::from(1)),
                Value::Integer(rug::Integer::from(2)),
            ]),
            Value::List(vec![
                Value::Integer(rug::Integer::from(3)),
                Value::Integer(rug::Integer::from(4)),
            ]),
        ]);
        let result = builtin_table_form(&[v]).unwrap();
        let s = result.to_string();
        // Rows are joined by newlines and columns by aligned spaces.
        assert!(s.contains('\n'));
        assert!(s.contains("1  2"), "columns should be aligned, got: {s:?}");
        assert!(s.contains("3  4"), "columns should be aligned, got: {s:?}");
        assert!(!s.contains('\t'), "TableForm no longer uses tabs: {s:?}");
    }

    #[test]
    fn test_table_form_non_list() {
        let v = Value::Integer(rug::Integer::from(42));
        let result = builtin_table_form(&[v]).unwrap();
        let s = result.to_string();
        assert_eq!(s, "42");
    }

    // ── MatrixForm tests ──

    #[test]
    fn test_matrix_form_basic() {
        let v = Value::List(vec![
            Value::List(vec![
                Value::Integer(rug::Integer::from(1)),
                Value::Integer(rug::Integer::from(2)),
            ]),
            Value::List(vec![
                Value::Integer(rug::Integer::from(3)),
                Value::Integer(rug::Integer::from(4)),
            ]),
        ]);
        let result = builtin_matrix_form(&[v]).unwrap();
        let s = result.to_string();
        // Should contain bracket characters
        assert!(
            s.contains('\u{23a1}') || s.contains('('),
            "MatrixForm should have brackets, got: {s}"
        );
        assert!(s.contains("1"));
        assert!(s.contains("4"));
    }

    #[test]
    fn test_matrix_form_single_row() {
        let v = Value::List(vec![Value::List(vec![
            Value::Integer(rug::Integer::from(1)),
            Value::Integer(rug::Integer::from(2)),
            Value::Integer(rug::Integer::from(3)),
        ])]);
        let result = builtin_matrix_form(&[v]).unwrap();
        let s = result.to_string();
        // Single row uses ( ) notation
        assert!(s.contains('('));
        assert!(s.contains(')'));
    }

    // ── PaddedForm tests ──

    #[test]
    fn test_padded_form_integer() {
        let v = Value::Integer(rug::Integer::from(42));
        let n = Value::Integer(rug::Integer::from(10));
        let result = builtin_padded_form(&[v, n]).unwrap();
        let s = result.to_string();
        assert_eq!(s.len(), 10);
        assert!(s.contains("42"));
    }

    #[test]
    fn test_padded_form_with_frac() {
        let v = Value::Integer(rug::Integer::from(42));
        let spec = Value::List(vec![
            Value::Integer(rug::Integer::from(10)),
            Value::Integer(rug::Integer::from(3)),
        ]);
        let result = builtin_padded_form(&[v, spec]).unwrap();
        let s = result.to_string();
        assert!(
            s.contains("42.000"),
            "PaddedForm should show 42.000, got: {s}"
        );
    }

    // ── StringForm tests ──

    #[test]
    fn test_string_form_basic() {
        let result = builtin_string_form(&[
            Value::Str("Hello `1`, you are `2` years old.".to_string()),
            Value::Str("Alice".to_string()),
            Value::Integer(rug::Integer::from(30)),
        ])
        .unwrap();
        let s = result.to_string();
        assert_eq!(s, "Hello Alice, you are 30 years old.");
    }

    #[test]
    fn test_string_form_no_slots() {
        let result = builtin_string_form(&[Value::Str("Hello world".to_string())]).unwrap();
        let s = result.to_string();
        assert_eq!(s, "Hello world");
    }

    #[test]
    fn test_string_form_out_of_range() {
        let result = builtin_string_form(&[
            Value::Str("val = `5`".to_string()),
            Value::Integer(rug::Integer::from(1)),
        ])
        .unwrap();
        let s = result.to_string();
        // Out of range slot is left as-is
        assert_eq!(s, "val = `5`");
    }

    // ── TeXForm ──

    #[test]
    fn test_tex_form_rendering() {
        assert_eq!(render_tex(&call("Plus", vec![sym("a"), sym("b")])), "a + b");
        assert_eq!(render_tex(&call("Power", vec![sym("x"), int(2)])), "x^{2}");
        assert_eq!(
            render_tex(&call("Divide", vec![sym("a"), sym("b")])),
            "\\frac{a}{b}"
        );
        assert_eq!(
            render_tex(&Value::Rational(Box::new(rug::Rational::from((1, 2))))),
            "\\frac{1}{2}"
        );
        assert_eq!(render_tex(&call("Sqrt", vec![int(2)])), "\\sqrt{2}");
        assert_eq!(render_tex(&sym("Pi")), "\\pi");
        assert_eq!(
            render_tex(&call(
                "Power",
                vec![call("Plus", vec![sym("a"), sym("b")]), int(2)]
            )),
            "\\left(a + b\\right)^{2}"
        );
    }

    #[test]
    fn test_tex_form_builtin_wraps_text() {
        let out = builtin_tex_form(&[call("Plus", vec![sym("a"), sym("b")])]).unwrap();
        assert!(matches!(
            out,
            Value::Formatted {
                format: Format::Text,
                ..
            }
        ));
        assert_eq!(out.to_string(), "a + b");
        assert!(builtin_tex_form(&[]).is_err());
        assert!(builtin_tex_form(&[int(1), int(2)]).is_err());
    }

    #[test]
    fn test_standard_and_output_form_render_infix() {
        let v = call("Plus", vec![sym("a"), sym("b")]);
        assert_eq!(
            builtin_standard_form(&[v.clone()]).unwrap().to_string(),
            "a + b"
        );
        assert_eq!(builtin_output_form(&[v]).unwrap().to_string(), "a + b");
    }
}
