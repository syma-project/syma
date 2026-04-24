/// ANSI terminal formatting helpers.
///
/// Each function wraps its input in ANSI escape codes and returns the formatted
/// `String`. Output is plain (no escape codes) when the `NO_COLOR` environment
/// variable is set.
use std::sync::LazyLock;

static NO_COLOR: LazyLock<bool> = LazyLock::new(|| std::env::var_os("NO_COLOR").is_some());

macro_rules! ansi {
    ($code:expr, $s:expr) => {{
        if *NO_COLOR {
            $s.to_string()
        } else {
            format!("\x1b[{}m{}\x1b[0m", $code, $s)
        }
    }};
}

pub fn green(s: &str) -> String {
    ansi!("32", s)
}
pub fn red(s: &str) -> String {
    ansi!("31", s)
}
pub fn bold_red(s: &str) -> String {
    ansi!("1;31", s)
}
pub fn cyan(s: &str) -> String {
    ansi!("36", s)
}
pub fn dim(s: &str) -> String {
    ansi!("2", s)
}
pub fn bold(s: &str) -> String {
    ansi!("1", s)
}
