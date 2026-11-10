/// Syma message system — analogous to Wolfram Language's Message/MessageName.
///
/// Messages are stored as `"Symbol::tag"` → template string.
/// In templates, `` `1` ``, `` `2` `` etc. are positional argument placeholders.
///
/// Messages can be captured into a buffer (instead of printing to stderr) by
/// calling [`with_buffer`]. The kernel uses this to include messages in JSON
/// responses.
use std::cell::RefCell;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Mutex, MutexGuard, OnceLock};

use crate::value::Value;

thread_local! {
    static MESSAGE_BUFFER: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
    /// Full message names (`Sym::tag`) emitted on this thread, in order. Backs
    /// `$MessageList` and lets `Check`/`Quiet` observe messages without the
    /// evaluator having to treat a message as a hard error.
    static MESSAGE_LIST: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    /// Active `Quiet[...]` scopes on this thread (innermost last).
    static QUIET_STACK: RefCell<Vec<QuietSpec>> = const { RefCell::new(Vec::new()) };
    /// Message specs turned off with `Off[...]` on this thread.
    static MESSAGES_OFF: RefCell<BTreeSet<String>> = const { RefCell::new(BTreeSet::new()) };
}

static MESSAGES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

/// A `Quiet[...]` scope: either everything, or a set of message specs.
#[derive(Clone, Debug)]
enum QuietSpec {
    All,
    Named(HashSet<String>),
}

/// Split a full message name `"Sym::tag"` into `("Sym", "tag")`.
///
/// A name without `::` is treated as a tag on the `General` symbol, matching
/// Wolfram Language's convention for general messages.
pub fn split_tag(tag: &str) -> (String, String) {
    match tag.split_once("::") {
        Some((sym, name)) => (sym.to_string(), name.to_string()),
        None => ("General".to_string(), tag.to_string()),
    }
}

/// Build a full message name `"Sym::tag"` from its parts.
pub fn full_name(sym: &str, tag: &str) -> String {
    format!("{}::{}", sym, tag)
}

/// Extract a suppress-spec string from a value used by `Off`/`On`/`Check`:
/// a `MessageName[sym, "tag"]` call, a bare symbol, or a string. Lists yield
/// `None` (callers handle them explicitly).
pub fn value_to_spec(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.clone()),
        Value::Symbol(s) => Some(s.clone()),
        Value::Call { head, args } if head == "MessageName" && args.len() == 2 => {
            let sym = match &args[0] {
                Value::Symbol(s) | Value::Str(s) => s.clone(),
                other => other.to_string(),
            };
            let name = match &args[1] {
                Value::Str(s) => s.clone(),
                other => other.to_string(),
            };
            Some(full_name(&sym, &name))
        }
        Value::Formatted { value, .. } => value_to_spec(value),
        Value::Pattern(e) => value_to_spec(&crate::pattern::unwrap_expr_to_value(e)),
        _ => None,
    }
}

/// Does the suppress-spec `spec` cover the full message `tag`?
///
/// Supported forms: an exact `"Sym::tag"`, a bare symbol `"Sym"` (matches all
/// of that symbol's messages), a group prefix `"Sym::"`, a trailing-`*`
/// pattern `"Sym::spell*"`, and `"All"`.
pub fn spec_matches(spec: &str, tag: &str) -> bool {
    if spec == "All" || spec == "All::All" {
        return true;
    }
    if let Some(prefix) = spec.strip_suffix("::") {
        return tag.starts_with(&format!("{prefix}::"));
    }
    if let Some(prefix) = spec.strip_suffix('*') {
        return tag.starts_with(prefix);
    }
    if !spec.contains("::") {
        return tag.starts_with(&format!("{spec}::"));
    }
    spec == tag
}

/// Turn `Off[spec]` on for the given message spec.
pub fn set_off(spec: impl Into<String>) {
    MESSAGES_OFF.with(|o| {
        o.borrow_mut().insert(spec.into());
    });
}

/// Turn `On[spec]` on (i.e. stop suppressing) for the given message spec.
pub fn set_on(spec: impl Into<String>) {
    let spec = spec.into();
    MESSAGES_OFF.with(|o| {
        o.borrow_mut().remove(&spec);
    });
}

/// True unless `tag` has been turned off (and is not covered by a `Quiet` scope).
pub fn is_message_on(tag: &str) -> bool {
    if QUIET_STACK.with(|q| {
        q.borrow().iter().any(|spec| match spec {
            QuietSpec::All => true,
            QuietSpec::Named(names) => names.iter().any(|n| spec_matches(n, tag)),
        })
    }) {
        return false;
    }
    !MESSAGES_OFF.with(|o| o.borrow().iter().any(|spec| spec_matches(spec, tag)))
}

/// Push a `Quiet[...]` scope. `None` suppresses everything; `Some(specs)`
/// suppresses only the listed messages.
pub fn push_quiet(specs: Option<Vec<String>>) {
    let spec = match specs {
        None => QuietSpec::All,
        Some(s) => QuietSpec::Named(s.into_iter().collect()),
    };
    QUIET_STACK.with(|q| q.borrow_mut().push(spec));
}

/// Pop the innermost `Quiet[...]` scope.
pub fn pop_quiet() {
    QUIET_STACK.with(|q| {
        q.borrow_mut().pop();
    });
}

/// Full names of messages emitted so far on this thread (the `$MessageList`).
pub fn message_list() -> Vec<String> {
    MESSAGE_LIST.with(|l| l.borrow().clone())
}

/// Clear the per-thread `$MessageList`.
pub fn clear_message_list() {
    MESSAGE_LIST.with(|l| l.borrow_mut().clear());
}

/// Record a mark for [`emitted_since`]; returns the current list length.
pub fn mark() -> usize {
    MESSAGE_LIST.with(|l| l.borrow().len())
}

/// True if any message was recorded since the matching [`mark`].
pub fn emitted_since(mark: usize) -> bool {
    MESSAGE_LIST.with(|l| l.borrow().len()) > mark
}

/// Full names of messages recorded since the matching [`mark`].
pub fn messages_since(mark: usize) -> Vec<String> {
    MESSAGE_LIST.with(|l| {
        let l = l.borrow();
        if mark >= l.len() {
            Vec::new()
        } else {
            l[mark..].to_vec()
        }
    })
}

/// Reset in-memory message state: re-seed the built-in templates, clear the
/// `Off` set, the `Quiet` stack and `$MessageList`. User templates registered
/// with [`set_template`] are preserved.
pub fn reset_state() {
    seed_defaults();
    MESSAGES_OFF.with(|o| o.borrow_mut().clear());
    QUIET_STACK.with(|q| q.borrow_mut().clear());
    clear_message_list();
}

/// Render a value the way a message template should display it: `HoldForm` is
/// unwrapped so the held expression appears verbatim, and `Formatted`
/// wrappers render their formatted representation.
pub fn render_arg(v: &Value) -> String {
    match v {
        Value::Call { head, args } if head == "HoldForm" && args.len() == 1 => render_arg(&args[0]),
        Value::Formatted { .. } => v.to_string(),
        other => other.to_string(),
    }
}

/// Run `f` with a message buffer active. Messages emitted during `f` are
/// collected into the returned `Vec` instead of printed to stderr.
/// Returns `(result_of_f, messages)`.
pub fn with_buffer<T>(f: impl FnOnce() -> T) -> (T, Vec<String>) {
    MESSAGE_BUFFER.with(|buf| {
        let prev = buf.replace(Some(Vec::new()));
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(f));
        let messages = buf.replace(prev).unwrap_or_default();
        match result {
            Ok(val) => (val, messages),
            Err(e) => std::panic::resume_unwind(e),
        }
    })
}

fn registry() -> &'static Mutex<HashMap<String, String>> {
    MESSAGES.get_or_init(|| {
        let mut m: HashMap<String, String> = HashMap::new();
        seed(&mut m);
        Mutex::new(m)
    })
}

/// Insert the built-in message templates into `m`.
fn seed(m: &mut HashMap<String, String>) {
    // Power messages
    m.insert(
        "Power::infy".to_string(),
        "Infinite expression `1` encountered.".to_string(),
    );

    // Infinity messages
    m.insert(
        "Infinity::indet".to_string(),
        "Indeterminate expression `1` encountered.".to_string(),
    );

    // General numeric messages
    m.insert(
        "General::zero".to_string(),
        "The argument `1` should be nonzero.".to_string(),
    );

    // General fall-through message for bare `Message[General::foo]`.
    m.insert(
        "General::name".to_string(),
        "`1` is not a known message name.".to_string(),
    );

    // Assertion failures
    m.insert(
        "Assert::asrt".to_string(),
        "Assertion `1` failed.".to_string(),
    );

    // Argument-count diagnostics
    m.insert(
        "General::argx".to_string(),
        "`1` called with `2` arguments; `3` argument(s) expected.".to_string(),
    );
    m.insert(
        "General::argt".to_string(),
        "`1` called with `2` arguments; between `3` and `4` arguments expected.".to_string(),
    );

    // Image messages
    m.insert(
        "Image::imgarray".to_string(),
        "The specified argument `1` should be an array of rank 2 or 3 with machine-sized numbers."
            .to_string(),
    );

    // Formatting messages
    m.insert(
        "StringForm::sfr".to_string(),
        "StringForm: item `1` is out of range.".to_string(),
    );
}

/// Re-insert built-in templates (used by [`reset_state`]).
fn seed_defaults() {
    let mut guard = lock_registry();
    seed(&mut guard);
}

/// Lock the registry, recovering from a poisoned mutex rather than panicking
/// so a single panicking thread cannot disable the message system.
fn lock_registry() -> MutexGuard<'static, HashMap<String, String>> {
    registry().lock().unwrap_or_else(|e| e.into_inner())
}

/// Look up the template for a message tag (e.g. `"Power::infy"`).
pub fn get_template(tag: &str) -> Option<String> {
    lock_registry().get(tag).cloned()
}

/// Register a user-defined message template.
pub fn set_template(tag: impl Into<String>, template: impl Into<String>) {
    lock_registry().insert(tag.into(), template.into());
}

/// Format a message template by substituting positional args.
/// `` `1` `` → args[0], `` `2` `` → args[1], etc.
///
/// Substitution is single-pass so an argument that itself contains `` `2` ``
/// is not re-substituted by a later placeholder.
pub fn format_message(template: &str, args: &[String]) -> String {
    let mut result = String::with_capacity(template.len());
    let mut chars = template.char_indices().peekable();
    while let Some((i, ch)) = chars.next() {
        if ch == '`' {
            let rest_start = i + ch.len_utf8();
            if let Some(rel) = template[rest_start..].find('`') {
                let inner = &template[rest_start..rest_start + rel];
                if let Ok(n) = inner.parse::<usize>()
                    && n >= 1
                    && n <= args.len()
                {
                    result.push_str(&args[n - 1]);
                    // Skip the characters up to and including the closing backtick.
                    let end = rest_start + rel + ch.len_utf8();
                    while let Some(&(j, _)) = chars.peek() {
                        if j < end {
                            chars.next();
                        } else {
                            break;
                        }
                    }
                    continue;
                }
            }
        }
        result.push(ch);
    }
    result
}

/// Emit a message to stderr, formatted like Wolfram Language:
/// `Symbol::tag: Message text.`
///
/// If a message buffer is active (via [`with_buffer`]), the message is
/// collected there instead of printed to stderr. Returns `true` if the message
/// was actually displayed (i.e. not suppressed by `Off`/`Quiet`); suppressed
/// messages are still recorded in `$MessageList`.
pub fn emit(tag: &str, args: &[String]) -> bool {
    let template = get_template(tag);
    emit_with_template(tag, template.as_deref(), args)
}

/// Like [`emit`] but with an explicit template (e.g. a user-assigned
/// `Sym::tag = "..."` template). `None` falls back to joining the arguments.
pub fn emit_with_template(tag: &str, template: Option<&str>, args: &[String]) -> bool {
    MESSAGE_LIST.with(|l| l.borrow_mut().push(tag.to_string()));
    if !is_message_on(tag) {
        return false;
    }
    let line = match template {
        Some(t) => format!("{}: {}", tag, format_message(t, args)),
        None => format!("{}: {}", tag, args.join(", ")),
    };
    let captured = MESSAGE_BUFFER.with(|buf| {
        if let Some(ref mut messages) = *buf.borrow_mut() {
            messages.push(line.clone());
            true
        } else {
            false
        }
    });
    if !captured {
        // No buffer active — print to stderr as before
        eprintln!("{}", line);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::{
        clear_message_list, emit, emitted_since, format_message, full_name, get_template,
        is_message_on, mark, message_list, pop_quiet, push_quiet, render_arg, reset_state, set_off,
        set_on, set_template, split_tag, with_buffer,
    };
    use crate::value::{Format, Value};

    #[test]
    fn test_format_message_positional() {
        let out = format_message("arg `1` and `2`", &["a".to_string(), "b".to_string()]);
        assert_eq!(out, "arg a and b");
    }

    #[test]
    fn test_format_message_does_not_resubstitute_args() {
        // Regression: a naive sequential `.replace` re-substitutes `` `2` ``
        // appearing inside an earlier argument.
        let out = format_message("`1` then `2`", &["x `2` y".to_string(), "z".to_string()]);
        assert_eq!(out, "x `2` y then z");
    }

    #[test]
    fn test_format_message_leaves_unknown_placeholder() {
        let out = format_message("`1` `3`", &["a".to_string()]);
        assert_eq!(out, "a `3`");
    }

    #[test]
    fn test_format_message_two_digit_index() {
        let args: Vec<String> = (1..=10).map(|i| i.to_string()).collect();
        assert_eq!(format_message("`1`..`10`", &args), "1..10");
    }

    #[test]
    fn test_format_message_adjacent_slots() {
        let out = format_message("`1``2`", &["a".to_string(), "b".to_string()]);
        assert_eq!(out, "ab");
    }

    #[test]
    fn test_format_message_empty_template() {
        assert_eq!(format_message("", &["a".to_string()]), "");
    }

    #[test]
    fn test_format_message_unicode_around_slot() {
        let out = format_message("\u{ab}`1`\u{bb}", &["\u{3b1}\u{3b2}".to_string()]);
        assert_eq!(out, "\u{ab}\u{3b1}\u{3b2}\u{bb}");
    }

    #[test]
    fn test_format_message_unterminated_slot() {
        assert_eq!(format_message("`1", &["a".to_string()]), "`1");
    }

    #[test]
    fn test_format_message_index_zero_is_out_of_range() {
        // `0` is not a valid slot (slots are 1-based); it is left verbatim.
        assert_eq!(format_message("`0` `1`", &["a".to_string()]), "`0` a");
    }

    #[test]
    fn test_set_and_get_template() {
        set_template("Foo::bar", "x = `1`");
        assert_eq!(get_template("Foo::bar").as_deref(), Some("x = `1`"));
        assert_eq!(get_template("Nope::none"), None);
    }

    #[test]
    fn test_with_buffer_collects_and_restores() {
        let ((), msgs) = with_buffer(|| {
            emit("Power::infy", &["1/0".to_string()]);
        });
        assert_eq!(msgs.len(), 1);
        assert!(msgs[0].starts_with("Power::infy: "), "got: {}", msgs[0]);
        assert!(msgs[0].contains("Infinite"));
        // Buffer restored afterwards: this must not panic and goes to stderr.
        emit("General::zero", &["0".to_string()]);
    }

    #[test]
    fn test_with_buffer_nested() {
        let (inner_result, outer_msgs) = with_buffer(|| {
            let (_, inner) = with_buffer(|| emit("General::zero", &["0".to_string()]));
            emit("Power::infy", &["1/0".to_string()]);
            inner
        });
        assert_eq!(outer_msgs.len(), 1);
        assert!(outer_msgs[0].starts_with("Power::infy: "));
        assert_eq!(inner_result.len(), 1);
        assert!(inner_result[0].starts_with("General::zero: "));
    }

    #[test]
    fn test_emit_unknown_tag_joins_args() {
        let (_, msgs) = with_buffer(|| emit("Nope::tag", &["a".to_string(), "b".to_string()]));
        assert_eq!(msgs, vec!["Nope::tag: a, b".to_string()]);
    }

    // ── Tag / name parsing ──────────────────────────────────────────────

    #[test]
    fn test_split_tag_roundtrip() {
        assert_eq!(
            split_tag("Power::infy"),
            ("Power".to_string(), "infy".to_string())
        );
        assert_eq!(full_name("Power", "infy"), "Power::infy");
    }

    #[test]
    fn test_split_tag_without_colons_defaults_to_general() {
        assert_eq!(split_tag("foo"), ("General".to_string(), "foo".to_string()));
    }

    #[test]
    fn test_split_tag_splits_on_first_separator_only() {
        assert_eq!(split_tag("a::b::c"), ("a".to_string(), "b::c".to_string()));
    }

    // ── Argument rendering ──────────────────────────────────────────────

    #[test]
    fn test_render_arg_plain() {
        assert_eq!(render_arg(&Value::Integer(3.into())), "3");
        // Strings keep their quotes, matching the historical `to_string` path.
        assert_eq!(render_arg(&Value::Str("x".to_string())), "\"x\"");
    }

    #[test]
    fn test_render_arg_strips_holdform() {
        let held = Value::Call {
            head: "HoldForm".to_string(),
            args: vec![Value::Symbol("x".to_string())],
        };
        assert_eq!(render_arg(&held), "x");
    }

    #[test]
    fn test_render_arg_unwraps_formatted() {
        let f = Value::Formatted {
            format: Format::InputForm,
            value: Box::new(Value::Call {
                head: "Plus".to_string(),
                args: vec![Value::Integer(1.into()), Value::Integer(1.into())],
            }),
        };
        assert_eq!(render_arg(&f), "1 + 1");
    }

    // ── Off / On ────────────────────────────────────────────────────────

    #[test]
    fn test_off_suppresses_emit_and_on_restores() {
        let tag = "TestOffOn::x";
        set_off("TestOffOn");
        assert!(!is_message_on(tag));
        let ((), msgs) = with_buffer(|| {
            assert!(!emit(tag, &[]));
        });
        assert!(msgs.is_empty(), "off message must not be buffered");
        set_on("TestOffOn");
        assert!(is_message_on(tag));
        let ((), msgs) = with_buffer(|| {
            assert!(emit(tag, &[]));
        });
        assert_eq!(msgs.len(), 1);
    }

    #[test]
    fn test_off_exact_tag_only() {
        set_off("TestExact::a");
        assert!(!is_message_on("TestExact::a"));
        assert!(is_message_on("TestExact::b"));
        set_on("TestExact::a");
    }

    #[test]
    fn test_off_group_prefix_and_wildcard() {
        set_off("TestGroup::");
        assert!(!is_message_on("TestGroup::any"));
        set_on("TestGroup::");

        set_off("TestWild::spell*");
        assert!(!is_message_on("TestWild::spell1"));
        assert!(is_message_on("TestWild::other"));
        set_on("TestWild::spell*");
    }

    #[test]
    fn test_off_all() {
        set_off("All");
        assert!(!is_message_on("Anything::goes"));
        set_on("All");
    }

    // ── Quiet ───────────────────────────────────────────────────────────

    #[test]
    fn test_quiet_all_suppresses_but_records() {
        clear_message_list();
        push_quiet(None);
        let ((), msgs) = with_buffer(|| {
            assert!(!emit("QuietAll::m", &[]));
        });
        assert!(msgs.is_empty());
        pop_quiet();
        assert!(message_list().contains(&"QuietAll::m".to_string()));
        assert!(emit("QuietAll::m", &[]));
    }

    #[test]
    fn test_quiet_named_suppresses_only_listed() {
        push_quiet(Some(vec!["QuietNamed::a".to_string()]));
        assert!(!is_message_on("QuietNamed::a"));
        assert!(is_message_on("QuietNamed::b"));
        pop_quiet();
        assert!(is_message_on("QuietNamed::a"));
    }

    #[test]
    fn test_quiet_nesting_is_lifo() {
        push_quiet(Some(vec!["Nest::a".to_string()]));
        push_quiet(Some(vec!["Nest::b".to_string()]));
        assert!(!is_message_on("Nest::b"));
        // Inner scope still active (a stays quiet from outer too).
        pop_quiet();
        assert!(!is_message_on("Nest::a"));
        assert!(is_message_on("Nest::b"));
        pop_quiet();
        assert!(is_message_on("Nest::a"));
    }

    // ── $MessageList ────────────────────────────────────────────────────

    #[test]
    fn test_emit_records_message_list_even_when_suppressed() {
        clear_message_list();
        set_off("Record::x");
        let _ = emit("Record::x", &[]);
        set_on("Record::x");
        assert_eq!(message_list(), vec!["Record::x".to_string()]);
        clear_message_list();
        assert!(message_list().is_empty());
    }

    #[test]
    fn test_mark_and_emitted_since() {
        clear_message_list();
        let m = mark();
        assert!(!emitted_since(m));
        let _ = emit("Since::a", &[]);
        assert!(emitted_since(m));
        clear_message_list();
    }

    // ── reset_state ─────────────────────────────────────────────────────

    #[test]
    fn test_reset_state_restores_builtin_templates() {
        reset_state();
        assert!(get_template("Power::infy").is_some());
        assert!(get_template("Assert::asrt").is_some());
        assert!(is_message_on("Power::infy"));
        assert!(message_list().is_empty());
    }
}
