/// Rule data structures for the Rubi integration engine.
///
/// These hold parsed integration rules and provide access
/// to them by category and loading order.
pub use crate::rubi::wl_ast::{IntRule, RuleFile};

/// The complete set of loaded integration rules.
#[derive(Debug, Clone)]
pub struct RuleDatabase {
    /// All rules in loading order (as specified in Rubi.m)
    pub rules: Vec<IntRule>,
    /// Individual rule files that were loaded
    pub files: Vec<RuleFile>,
}

impl RuleDatabase {
    pub fn new() -> Self {
        RuleDatabase {
            rules: Vec::new(),
            files: Vec::new(),
        }
    }

    /// Add a rule file's rules to the database, in order.
    pub fn add_file(&mut self, file: RuleFile) {
        let start_index = self.rules.len();
        for (i, rule) in file.rules.iter().enumerate() {
            let mut r = rule.clone();
            r.index = start_index + i;
            self.rules.push(r);
        }
        self.files.push(file);
    }

    /// Total number of rules loaded.
    pub fn len(&self) -> usize {
        self.rules.len()
    }

    /// Whether the database is empty.
    pub fn is_empty(&self) -> bool {
        self.rules.is_empty()
    }
}

impl Default for RuleDatabase {
    fn default() -> Self {
        Self::new()
    }
}
