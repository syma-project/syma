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
use std::sync::{Mutex, OnceLock};

thread_local! {
    static MESSAGE_BUFFER: RefCell<Option<Vec<String>>> = const { RefCell::new(None) };
}

static MESSAGES: OnceLock<Mutex<HashMap<&'static str, &'static str>>> = OnceLock::new();

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

fn registry() -> &'static Mutex<HashMap<&'static str, &'static str>> {
    MESSAGES.get_or_init(|| {
        let mut m: HashMap<&'static str, &'static str> = HashMap::new();

        // Power messages
        m.insert("Power::infy", "Infinite expression `1` encountered.");

        // Infinity messages
        m.insert(
            "Infinity::indet",
            "Indeterminate expression `1` encountered.",
        );

        // General numeric messages
        m.insert("General::zero", "The argument `1` should be nonzero.");

        // Image messages
        m.insert(
            "Image::imgarray",
            "The specified argument `1` should be an array of rank 2 or 3 with machine-sized numbers.",
        );

        Mutex::new(m)
    })
}

/// Look up the template for a message tag (e.g. `"Power::infy"`).
pub fn get_template(tag: &str) -> Option<String> {
    registry().lock().unwrap().get(tag).map(|s| s.to_string())
}

/// Register a user-defined message template.
pub fn set_template(tag: &'static str, template: &'static str) {
    registry().lock().unwrap().insert(tag, template);
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
    use super::format_message;

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
}
