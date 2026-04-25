/// Syma message system — analogous to Wolfram Language's Message/MessageName.
///
/// Messages are stored as `"Symbol::tag"` → template string.
/// In templates, `` `1` ``, `` `2` `` etc. are positional argument placeholders.
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

static MESSAGES: OnceLock<Mutex<HashMap<&'static str, &'static str>>> = OnceLock::new();

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
pub fn emit(tag: &str, args: &[String]) {
    if let Some(template) = get_template(tag) {
        let text = format_message(&template, args);
        eprintln!("{}: {}", tag, text);
    }
}
