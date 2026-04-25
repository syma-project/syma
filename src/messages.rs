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
pub fn format_message(template: &str, args: &[String]) -> String {
    let mut result = template.to_string();
    for (i, arg) in args.iter().enumerate() {
        let placeholder = format!("`{}`", i + 1);
        result = result.replace(&placeholder, arg);
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
        if let Some(template) = get_template(tag) {
            let text = format_message(&template, args);
            eprintln!("{}: {}", tag, text);
        }
    }
}
