//! interactive: Dynamic, Manipulate, Animate, LocatorPane.
//!
//! Syma has no notebook front-end, so these interactive constructs produce a
//! *static artifact* that a kernel / notebook can display:
//!
//! * `Dynamic[expr]` (HoldAll) stays symbolic and is represented as
//!   `Dynamic[held, current]` — `held` is the unevaluated expression and
//!   `current` is its value right now. A front-end can re-evaluate `held`
//!   whenever a dependency changes.
//! * `Manipulate`, `Animate` and `LocatorPane` return a **self-contained HTML
//!   string** (no external assets and no JavaScript) containing inline SVG
//!   rendered through the existing `graphics` machinery. This is the choice
//!   made for headless rendering: a `Value::Str` whose first characters are
//!   `<!DOCTYPE html>` is directly displayable by a notebook.
//!
//! Control variables are sampled at evenly spaced points (bounded, so the
//! artifact stays small); `Manipulate`/`Animate` vary the *first* control and
//! hold the remaining controls at their initial value. Callers that pass an
//! unparseable control specification get the call back unevaluated.

use std::collections::HashMap;

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, Value};

/// Number of sampled frames for a continuous control.
const N_FRAMES: usize = 6;
/// Upper bound on frames for a discrete (enumerated) control.
const MAX_FRAMES: usize = 8;

// ── Held-value helpers ──────────────────────────────────────────────────────

/// Unwrap a `Value::Pattern` (produced by a Hold*/HoldAll attribute) into a
/// plain value, leaving already-evaluated values untouched.
fn unwrap_held(v: &Value) -> Value {
    match v {
        Value::Pattern(expr) => crate::pattern::unwrap_expr_to_value(expr),
        other => other.clone(),
    }
}

/// Evaluate a possibly-held value in `env` (no-op for concrete values).
fn eval_held(v: &Value, env: &Env) -> Result<Value, EvalError> {
    match v {
        Value::Pattern(expr) => crate::eval::eval(expr, env),
        other => Ok(other.clone()),
    }
}

/// Rebuild the original call with unwrapped arguments (used when inapplicable).
fn original_call(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.iter().map(unwrap_held).collect(),
    }
}

fn set_attributes(env: &Env, name: &str, attrs: &[&str]) {
    env.set_attributes(name, attrs.iter().map(|s| s.to_string()).collect());
}

fn real(v: f64) -> Value {
    super::real(v)
}

/// Coerce a (possibly symbolic) numeric control bound to `f64`, evaluating it
/// numerically (`N[...]`) when a direct conversion is not possible — this makes
/// bounds such as `2 Pi` or `Pi/2` work.
fn numeric_bound(v: &Value, env: &Env) -> Option<f64> {
    if let Some(x) = super::to_f64(v) {
        return Some(x);
    }
    let expr = crate::eval::table::value_to_expr(v);
    let n_call = Expr::Call {
        head: Box::new(Expr::Symbol("N".to_string())),
        args: vec![expr],
    };
    match crate::eval::eval(&n_call, env) {
        Ok(result) => super::to_f64(&result),
        Err(_) => None,
    }
}

// ── HTML helpers ────────────────────────────────────────────────────────────

fn escape_html(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Render an evaluated expression as inline HTML. `Graphics[...]` values are
/// rendered to SVG; SVG strings pass through; everything else is text.
fn render_result_html(v: &Value) -> String {
    match v {
        Value::Call { head, args } if head == "Graphics" && !args.is_empty() => {
            let options = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| Value::Assoc(HashMap::new()));
            match super::graphics::render_svg(&args[0], &options) {
                Ok(svg) => svg,
                Err(_) => format!("<pre>{}</pre>", escape_html(&v.to_string())),
            }
        }
        Value::Str(s) if s.contains("<svg") => s.clone(),
        _ => format!(
            "<span class=\"syma-value\">{}</span>",
            escape_html(&v.to_string())
        ),
    }
}

const STYLE: &str = "<style>\
body{font-family:sans-serif;margin:8px}\
.syma-manipulate,.syma-animate,.syma-locator{border:1px solid #ccc;border-radius:6px;padding:8px;display:inline-block}\
.syma-frames{display:flex;flex-wrap:wrap;gap:8px}\
.syma-frame{border:1px solid #eee;border-radius:4px;padding:4px}\
.syma-frame-label{font-size:11px;color:#555;margin-bottom:2px}\
.syma-controls{margin-top:8px;display:flex;flex-direction:column;gap:4px}\
.syma-controls label{font-size:12px;color:#333}\
</style>";

/// Wrap body markup in a minimal self-contained HTML document.
fn html_document(class: &str, inner: &str) -> String {
    format!(
        "<!DOCTYPE html>\n<html><head><meta charset=\"utf-8\">{STYLE}</head>\
<body><div class=\"{class}\">{inner}</div></body></html>\n"
    )
}

// ── Control specifications ──────────────────────────────────────────────────

/// A parsed `{u, ...}` control specification.
enum Control {
    Continuous {
        name: String,
        min: f64,
        max: f64,
        step: f64,
    },
    Discrete {
        name: String,
        values: Vec<Value>,
    },
}

impl Control {
    fn name(&self) -> &str {
        match self {
            Control::Continuous { name, .. } | Control::Discrete { name, .. } => name,
        }
    }

    /// Initial value used when this control is not being varied.
    fn initial(&self) -> Value {
        match self {
            Control::Continuous { min, .. } => real(*min),
            Control::Discrete { values, .. } => values.first().cloned().unwrap_or(Value::Null),
        }
    }

    /// Evenly spaced sample values for this control (bounded by frame caps).
    fn samples(&self) -> Vec<Value> {
        match self {
            Control::Continuous { min, max, .. } => {
                if (max - min).abs() < f64::EPSILON {
                    return vec![real(*min)];
                }
                (0..N_FRAMES)
                    .map(|i| real(min + (max - min) * i as f64 / (N_FRAMES - 1) as f64))
                    .collect()
            }
            Control::Discrete { values, .. } => values.iter().take(MAX_FRAMES).cloned().collect(),
        }
    }

    /// Static HTML widget describing this control.
    fn widget_html(&self) -> String {
        match self {
            Control::Continuous {
                name,
                min,
                max,
                step,
            } => format!(
                "<label>{} <input type=\"range\" min=\"{}\" max=\"{}\" step=\"{}\" value=\"{}\"></label>",
                escape_html(name),
                min,
                max,
                step,
                min,
            ),
            Control::Discrete { name, values } => {
                let mut opts = String::new();
                for (i, v) in values.iter().enumerate() {
                    opts.push_str(&format!(
                        "<option value=\"{}\"{}>{}</option>",
                        escape_html(&v.to_string()),
                        if i == 0 { " selected" } else { "" },
                        escape_html(&v.to_string()),
                    ));
                }
                format!(
                    "<label>{} <select>{}</select></label>",
                    escape_html(name),
                    opts
                )
            }
        }
    }
}

/// Parse `{u, min, max[, step]}` or `{u, {a, b, c}}`.
fn parse_control(v: &Value, env: &Env) -> Option<Control> {
    let items = match v {
        Value::List(items) => items,
        _ => return None,
    };
    let name = match items.first() {
        Some(Value::Symbol(s)) => s.clone(),
        _ => return None,
    };
    if items.len() >= 2
        && let Value::List(values) = &items[1]
    {
        if values.is_empty() {
            return None;
        }
        return Some(Control::Discrete {
            name,
            values: values.clone(),
        });
    }
    if items.len() >= 3 {
        let min = numeric_bound(&items[1], env)?;
        let max = numeric_bound(&items[2], env)?;
        let step = items
            .get(3)
            .and_then(|v| numeric_bound(v, env))
            .filter(|s| *s > 0.0)
            .unwrap_or_else(|| (max - min).abs() / 100.0);
        return Some(Control::Continuous {
            name,
            min,
            max,
            step,
        });
    }
    // `{u}` — default 0..1.
    if items.len() == 1 {
        return Some(Control::Continuous {
            name,
            min: 0.0,
            max: 1.0,
            step: 0.01,
        });
    }
    None
}

// ── Sampling / artifact building ────────────────────────────────────────────

fn eval_with(body: &Value, bindings: &[(String, Value)], env: &Env) -> Result<Value, EvalError> {
    let child = env.child();
    for (name, value) in bindings {
        child.set(name.clone(), value.clone());
    }
    eval_held(body, &child)
}

/// Build the HTML artifact: one frame per sampled value of the first control,
/// then (optionally) static widgets for every control.
fn build_artifact(
    body: &Value,
    controls: &[Control],
    env: &Env,
    class: &str,
    widgets: bool,
) -> Result<String, EvalError> {
    let first = &controls[0];
    let base: Vec<(String, Value)> = controls
        .iter()
        .map(|c| (c.name().to_string(), c.initial()))
        .collect();

    let mut frames = String::from("<div class=\"syma-frames\">\n");
    for sample in first.samples() {
        let mut bindings = base.clone();
        bindings[0].1 = sample.clone();
        let frame_html = match eval_with(body, &bindings, env) {
            Ok(value) => render_result_html(&value),
            Err(e) => format!(
                "<span class=\"syma-error\">{}</span>",
                escape_html(&e.to_string())
            ),
        };
        frames.push_str("<div class=\"syma-frame\">");
        frames.push_str(&format!(
            "<div class=\"syma-frame-label\">{} = {}</div>",
            escape_html(first.name()),
            escape_html(&sample.to_string())
        ));
        frames.push_str(&frame_html);
        frames.push_str("</div>\n");
    }
    frames.push_str("</div>\n");

    if widgets {
        let mut controls_html = String::from("<div class=\"syma-controls\">\n");
        for c in controls {
            controls_html.push_str(&c.widget_html());
            controls_html.push('\n');
        }
        controls_html.push_str("</div>\n");
        frames.push_str(&controls_html);
    }

    Ok(html_document(class, &frames))
}

// ── Builtins ────────────────────────────────────────────────────────────────

/// `Dynamic[expr, ...]` — hold `expr` and represent it as `Dynamic[held, current]`.
fn builtin_dynamic(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Ok(original_call("Dynamic", args));
    }
    let held = unwrap_held(&args[0]);
    let current = eval_held(&args[0], env)?;
    Ok(Value::Call {
        head: "Dynamic".to_string(),
        args: vec![held, current],
    })
}

/// `Manipulate[expr, specs...]` — sample the first control, render frames.
fn builtin_manipulate(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(original_call("Manipulate", args));
    }
    let controls = collect_controls(&args[1..], env);
    if controls.is_empty() {
        return Ok(original_call("Manipulate", args));
    }
    Ok(Value::Str(build_artifact(
        &args[0],
        &controls,
        env,
        "syma-manipulate",
        true,
    )?))
}

/// `Animate[expr, specs...]` — like `Manipulate` but frames only.
fn builtin_animate(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(original_call("Animate", args));
    }
    let controls = collect_controls(&args[1..], env);
    if controls.is_empty() {
        return Ok(original_call("Animate", args));
    }
    Ok(Value::Str(build_artifact(
        &args[0],
        &controls,
        env,
        "syma-animate",
        false,
    )?))
}

fn collect_controls(specs: &[Value], env: &Env) -> Vec<Control> {
    let mut controls = Vec::new();
    for spec in specs {
        match parse_control(&unwrap_held(spec), env) {
            Some(c) => controls.push(c),
            None => return Vec::new(),
        }
    }
    controls
}

/// `LocatorPane[pt, content, ...]` — overlay locator markers on `content`.
///
/// `pt` is `{x, y}` or a list of such points. When `content` is a `Graphics`
/// value the markers are appended as `Point` primitives so they share the
/// graphic's coordinate system; otherwise the content is rendered as text.
fn builtin_locator_pane(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 {
        return Ok(original_call("LocatorPane", args));
    }
    let points = unwrap_held(&args[0]);
    let content = eval_held(&args[1], env)?;

    let marked = match content {
        Value::Call { head, mut args } if head == "Graphics" && !args.is_empty() => {
            let pts: Vec<Value> = match &points {
                Value::List(items)
                    if items.first().map(|v| matches!(v, Value::List(_))) == Some(false) =>
                {
                    vec![points.clone()]
                }
                Value::List(items) => items.clone(),
                _ => Vec::new(),
            };
            if let Value::List(prims) = &mut args[0] {
                prims.push(Value::Call {
                    head: "RGBColor".to_string(),
                    args: vec![real(1.0), real(0.0), real(0.0)],
                });
                prims.push(Value::Call {
                    head: "PointSize".to_string(),
                    args: vec![real(0.035)],
                });
                for p in pts {
                    prims.push(Value::Call {
                        head: "Point".to_string(),
                        args: vec![p],
                    });
                }
            }
            Value::Call { head, args }
        }
        other => other,
    };

    let inner = format!(
        "<div class=\"syma-frames\"><div class=\"syma-frame\">{}</div></div>",
        render_result_html(&marked)
    );
    Ok(Value::Str(html_document("syma-locator", &inner)))
}

/// `Slider[v, range]` — leave the head intact (HoldFirst on the value) so a
/// front-end can bind it.
fn builtin_slider(args: &[Value]) -> Result<Value, EvalError> {
    Ok(original_call("Slider", args))
}

/// `Manipulator[v, range]` — see `builtin_slider`.
fn builtin_manipulator(args: &[Value]) -> Result<Value, EvalError> {
    Ok(original_call("Manipulator", args))
}

/// `SetterBar[v, {v1, ...}]` — see `builtin_slider`.
fn builtin_setter_bar(args: &[Value]) -> Result<Value, EvalError> {
    Ok(original_call("SetterBar", args))
}

/// `Checkbox[v]` — see `builtin_slider`.
fn builtin_checkbox(args: &[Value]) -> Result<Value, EvalError> {
    Ok(original_call("Checkbox", args))
}

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    use super::register_builtin;
    use super::register_builtin_env;

    register_builtin_env(env, "Dynamic", builtin_dynamic);
    register_builtin_env(env, "Manipulate", builtin_manipulate);
    register_builtin_env(env, "Animate", builtin_animate);
    register_builtin_env(env, "LocatorPane", builtin_locator_pane);

    register_builtin(env, "Slider", builtin_slider);
    register_builtin(env, "Manipulator", builtin_manipulator);
    register_builtin(env, "SetterBar", builtin_setter_bar);
    register_builtin(env, "Checkbox", builtin_checkbox);

    set_attributes(env, "Dynamic", &["HoldAll", "Protected"]);
    set_attributes(env, "Manipulate", &["HoldAll", "Protected"]);
    set_attributes(env, "Animate", &["HoldAll", "Protected"]);
    set_attributes(env, "LocatorPane", &["Protected"]);
    for name in ["Slider", "Manipulator", "SetterBar", "Checkbox"] {
        set_attributes(env, name, &["HoldFirst", "Protected"]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(src: &str) -> Value {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let tokens = crate::lexer::tokenize(src).expect("tokenize");
        let ast = crate::parser::parse(tokens).expect("parse");
        crate::eval::eval_program(&ast, &env).expect("eval")
    }

    #[test]
    fn test_dynamic_holds_expression() {
        let v = ev("Dynamic[1 + 1]");
        match &v {
            Value::Call { head, args } if head == "Dynamic" => {
                assert_eq!(args.len(), 2);
                match &args[0] {
                    Value::Call { head, args: a } => {
                        assert_eq!(head, "Plus");
                        assert_eq!(a.len(), 2);
                    }
                    other => panic!("expected held Plus call, got {other:?}"),
                }
                assert_eq!(args[1].to_string(), "2");
            }
            other => panic!("expected Dynamic call, got {other:?}"),
        }
    }

    #[test]
    fn test_dynamic_evaluates_in_environment() {
        let v = ev("x = 5; Dynamic[x^2]");
        match &v {
            Value::Call { head, args } if head == "Dynamic" => {
                assert_eq!(args[1].to_string(), "25");
            }
            other => panic!("expected Dynamic call, got {other:?}"),
        }
    }

    #[test]
    fn test_dynamic_no_args_unevaluated() {
        let v = ev("Dynamic[]");
        match &v {
            Value::Call { head, args } if head == "Dynamic" => assert!(args.is_empty()),
            other => panic!("expected Dynamic call, got {other:?}"),
        }
    }

    #[test]
    fn test_manipulate_emits_html_with_frames_and_widgets() {
        let v = ev("Manipulate[u^2, {u, 0, 2, 1}]");
        let html = match &v {
            Value::Str(s) => s.clone(),
            other => panic!("expected Str, got {other:?}"),
        };
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("class=\"syma-manipulate\""));
        assert_eq!(html.matches("class=\"syma-frame\"").count(), N_FRAMES);
        assert!(html.contains("type=\"range\""));
        assert!(html.contains("u = 0"));
    }

    #[test]
    fn test_manipulate_discrete_control() {
        let v = ev("Manipulate[u, {u, {1, 2, 3}}]");
        let html = match &v {
            Value::Str(s) => s.clone(),
            other => panic!("expected Str, got {other:?}"),
        };
        assert!(html.contains("<select>"));
        assert_eq!(html.matches("class=\"syma-frame\"").count(), 3);
    }

    #[test]
    fn test_manipulate_plot_renders_svg() {
        let v = ev("Manipulate[Plot[u x, {x, 0, 1}], {u, 0, 2}]");
        let html = match &v {
            Value::Str(s) => s.clone(),
            other => panic!("expected Str, got {other:?}"),
        };
        assert!(html.contains("<svg"), "expected inline SVG");
        assert!(html.contains("</svg>"));
    }

    #[test]
    fn test_animate_emits_frames() {
        let v = ev("Animate[Sin[u], {u, 0, 2 Pi}]");
        let html = match &v {
            Value::Str(s) => s.clone(),
            other => panic!("expected Str, got {other:?}"),
        };
        assert!(html.contains("class=\"syma-animate\""));
        assert_eq!(html.matches("class=\"syma-frame\"").count(), N_FRAMES);
    }

    #[test]
    fn test_locator_pane_overlays_point() {
        let v = ev("LocatorPane[{0.5, 0.5}, Graphics[{Line[{{0, 0}, {1, 1}}]}]]");
        let html = match &v {
            Value::Str(s) => s.clone(),
            other => panic!("expected Str, got {other:?}"),
        };
        assert!(html.contains("class=\"syma-locator\""));
        assert!(html.contains("<svg"));
        assert!(html.contains("<circle"), "expected locator marker");
    }

    #[test]
    fn test_manipulate_inapplicable_returns_call() {
        let v = ev("Manipulate[1]");
        match &v {
            Value::Call { head, .. } if head == "Manipulate" => {}
            other => panic!("expected unevaluated Manipulate, got {other:?}"),
        }
    }

    #[test]
    fn test_control_placeholders_hold_first_argument() {
        match ev("Slider[x, {0, 1}]") {
            Value::Call { head, args } => {
                assert_eq!(head, "Slider");
                assert_eq!(args[0], Value::Symbol("x".to_string()));
            }
            other => panic!("expected Slider call, got {other:?}"),
        }
    }

    #[test]
    fn test_setterbar_and_checkbox_round_trip() {
        for src in ["SetterBar[x, {a, b}]", "Checkbox[x]"] {
            match ev(src) {
                Value::Call { head, .. } => assert!(head == "SetterBar" || head == "Checkbox"),
                other => panic!("expected control call for {src}, got {other:?}"),
            }
        }
    }

    #[test]
    fn test_parse_control_continuous_and_discrete() {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let c = parse_control(
            &Value::List(vec![
                Value::Symbol("u".to_string()),
                Value::Integer(rug::Integer::from(0)),
                Value::Integer(rug::Integer::from(4)),
                Value::Integer(rug::Integer::from(2)),
            ]),
            &env,
        )
        .expect("continuous");
        assert!(matches!(c, Control::Continuous { .. }));
        assert_eq!(c.samples().len(), N_FRAMES);

        let d = parse_control(
            &Value::List(vec![
                Value::Symbol("u".to_string()),
                Value::List(vec![Value::Integer(rug::Integer::from(1))]),
            ]),
            &env,
        )
        .expect("discrete");
        assert!(matches!(d, Control::Discrete { .. }));
    }

    #[test]
    fn test_parse_control_symbolic_bounds() {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        // `2 Pi` is symbolic; `numeric_bound` must resolve it.
        let v = ev("Manipulate[Sin[u], {u, 0, 2 Pi}]");
        assert!(matches!(v, Value::Str(_)), "symbolic bound should parse");
    }

    #[test]
    fn test_escape_html() {
        assert_eq!(escape_html("a<b>&\"c"), "a&lt;b&gt;&amp;&quot;c");
    }
}
