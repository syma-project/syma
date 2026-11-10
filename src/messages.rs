/// Syma message system — analogous to Wolfram Language's Message/MessageName.
///
/// Messages are stored as `"Symbol::tag"` → template string.
/// In templates, `` `1` ``, `` `2` `` etc. are positional argument placeholders.
///
/// Messages can be captured into a buffer (instead of printing to stderr) by
/// calling [`with_buffer`]. The kernel uses this to include messages in JSON
/// responses.
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, OnceLock};

thread_local! {
    static MESSAGE_BUFFER: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

static MESSAGES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

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

        Mutex::new(m)
    })
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
/// collected there instead of printed to stderr.
pub fn emit(tag: &str, args: &[String]) {
    let captured = MESSAGE_BUFFER.with(|buf| {
        if let Some(ref mut messages) = *buf.borrow_mut() {
            let text = if let Some(template) = get_template(tag) {
                format_message(&template, args)
            } else {
                args.join(", ")
            };
            messages.push(format!("{}: {}", tag, text));
            true
        } else {
            false
        }
    });
    if !captured {
        // No buffer active — print to stderr as before
        match get_template(tag) {
            Some(template) => eprintln!("{}: {}", tag, format_message(&template, args)),
            None => eprintln!("{}: {}", tag, args.join(", ")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{emit, format_message, get_template, set_template, with_buffer};

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
            let ((), inner) = with_buffer(|| emit("General::zero", &["0".to_string()]));
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
        let ((), msgs) = with_buffer(|| emit("Nope::tag", &["a".to_string(), "b".to_string()]));
        assert_eq!(msgs, vec!["Nope::tag: a, b".to_string()]);
    }
}
