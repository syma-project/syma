/// CLI integration tests: eval, examples, localsymbol, format, pattern.

#[path = "common/mod.rs"]
mod common;
pub use common::*;

#[path = "cli/eval.rs"]
mod eval;
#[path = "cli/examples.rs"]
mod examples;
#[path = "cli/format.rs"]
mod format;
#[path = "cli/localsymbol.rs"]
mod localsymbol;
#[path = "cli/pattern.rs"]
mod pattern;
