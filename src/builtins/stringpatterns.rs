//! String-pattern heads and regex: `StartOfString`, `EndOfString`,
//! `WordCharacter`, `DigitCharacter`, `LetterCharacter`, `WhitespaceCharacter`,
//! `Except`, `RegularExpression`, plus the pattern-aware overrides for
//! `StringMatchQ`, `StringCases`, `StringReplace` and `StringSplit`, and
//! `StringReplacePart`.
//!
//! The pattern heads are not callable builtins; they survive as symbols (or as
//! the unevaluated `Except[...]` / `RegularExpression[...]` calls) and are
//! interpreted here as elements of a `StringExpression` sequence. Whenever a
//! request cannot be understood the builtin returns unevaluated so the symbolic
//! form survives.

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, Value};
#[cfg(test)]
use rug::Integer;

// ── Pattern AST ────────────────────────────────────────────────────────────

/// One link of a compiled string pattern.
#[derive(Clone, Debug)]
enum Node {
    /// A literal WL string part, where `*` and `?` are wildcards.
    Glob(Vec<char>),
    /// An exact literal run (used by compiled regexes).
    Lit(Vec<char>),
    /// `StartOfString`.
    Start,
    /// `EndOfString`.
    End,
    /// `_` / `Blank[]` / regex `.` — matches any single character.
    AnyChar,
    /// One of the named character classes.
    Class { kind: Predefined, negated: bool },
    /// A bracketed regex character set.
    Set {
        ranges: Vec<(char, char)>,
        negated: bool,
    },
    /// Zero or more repetitions.
    Star(Box<Node>),
    /// One or more repetitions.
    Plus(Box<Node>),
    /// Zero or one occurrence.
    Optional(Box<Node>),
    /// A concatenation.
    Seq(Vec<Node>),
    /// A set of alternatives.
    Alt(Vec<Node>),
    /// `Except[p]` — one character not matched by `p`.
    Except(Box<Node>),
}

/// Named character classes shared by the WL heads and the regex engine.
#[derive(Clone, Copy, Debug)]
enum Predefined {
    Word,
    Digit,
    Letter,
    Whitespace,
}

impl Predefined {
    fn matches(self, c: char) -> bool {
        match self {
            Predefined::Word => c.is_alphanumeric() || c == '_',
            Predefined::Digit => c.is_ascii_digit(),
            Predefined::Letter => c.is_alphabetic(),
            Predefined::Whitespace => c.is_whitespace(),
        }
    }
}

// ── Parsing WL string patterns into `Node`s ────────────────────────────────

/// Parse a runtime value that denotes a string pattern.
fn parse_value(v: &Value) -> Option<Node> {
    match v {
        Value::Str(s) => Some(Node::Glob(s.chars().collect())),
        Value::Symbol(name) => parse_head_symbol(name),
        Value::Builtin(name, _) => parse_head_symbol(name),
        Value::Pattern(expr) => parse_expr(expr),
        Value::Call { head, args } => parse_call(head, args),
        _ => None,
    }
}

/// Parse a pattern AST node (used for `Value::Pattern` and `Blank` nodes).
fn parse_expr(expr: &Expr) -> Option<Node> {
    match expr {
        Expr::Blank { .. } => Some(Node::AnyChar),
        Expr::NamedBlank { .. } => Some(Node::AnyChar),
        Expr::BlankSequence { .. } => Some(Node::Plus(Box::new(Node::AnyChar))),
        Expr::BlankNullSequence { .. } => Some(Node::Star(Box::new(Node::AnyChar))),
        Expr::Str(s) => Some(Node::Glob(s.chars().collect())),
        Expr::Symbol(name) => parse_head_symbol(name),
        Expr::Call { head, args } => {
            let head_name = match head.as_ref() {
                Expr::Symbol(name) => name.clone(),
                other => crate::pattern::unwrap_expr_to_value(other).to_string(),
            };
            let arg_vals: Vec<Value> = args
                .iter()
                .map(crate::pattern::unwrap_expr_to_value)
                .collect();
            parse_call(&head_name, &arg_vals)
        }
        other => parse_value(&crate::pattern::unwrap_expr_to_value(other)),
    }
}

fn parse_head_symbol(name: &str) -> Option<Node> {
    match name {
        "StartOfString" => Some(Node::Start),
        "EndOfString" => Some(Node::End),
        "WordCharacter" => Some(Node::Class {
            kind: Predefined::Word,
            negated: false,
        }),
        "DigitCharacter" => Some(Node::Class {
            kind: Predefined::Digit,
            negated: false,
        }),
        "LetterCharacter" => Some(Node::Class {
            kind: Predefined::Letter,
            negated: false,
        }),
        "WhitespaceCharacter" => Some(Node::Class {
            kind: Predefined::Whitespace,
            negated: false,
        }),
        // Alternate spellings accepted by WL for the same classes.
        "Whitespace" => Some(Node::Class {
            kind: Predefined::Whitespace,
            negated: false,
        }),
        _ => None,
    }
}

fn parse_call(head: &str, args: &[Value]) -> Option<Node> {
    match head {
        "StringExpression" => {
            let mut nodes = Vec::with_capacity(args.len());
            for a in args {
                nodes.push(parse_value(a)?);
            }
            Some(Node::Seq(nodes))
        }
        "Except" if args.len() == 1 => Some(Node::Except(Box::new(parse_value(&args[0])?))),
        "RegularExpression" if args.len() == 1 => match &args[0] {
            Value::Str(s) => compile_regex(s),
            Value::Pattern(Expr::Str(s)) => compile_regex(s),
            _ => None,
        },
        "Repeated" if !args.is_empty() => Some(Node::Plus(Box::new(parse_value(&args[0])?))),
        "RepeatedNull" if !args.is_empty() => Some(Node::Star(Box::new(parse_value(&args[0])?))),
        "Alternatives" if !args.is_empty() => {
            let mut alts = Vec::with_capacity(args.len());
            for a in args {
                alts.push(parse_value(a)?);
            }
            Some(Node::Alt(alts))
        }
        "Blank" => Some(Node::AnyChar),
        _ => None,
    }
}

/// True when `v` represents a pattern rather than a bare literal string.
/// Bare strings are left to the pre-existing literal/glob implementations so
/// their semantics stay untouched.
fn is_pattern(v: &Value) -> bool {
    !matches!(v, Value::Str(_))
}

// ── Matching engine ────────────────────────────────────────────────────────

fn dedup(mut v: Vec<usize>) -> Vec<usize> {
    v.sort_unstable();
    v.dedup();
    v
}

/// All positions reachable by matching `node` starting at `pos`.
fn match_node(node: &Node, chars: &[char], pos: usize) -> Vec<usize> {
    match node {
        Node::Start => {
            if pos == 0 {
                vec![pos]
            } else {
                vec![]
            }
        }
        Node::End => {
            if pos == chars.len() {
                vec![pos]
            } else {
                vec![]
            }
        }
        Node::AnyChar => {
            if pos < chars.len() {
                vec![pos + 1]
            } else {
                vec![]
            }
        }
        Node::Lit(lit) => {
            if chars[pos..].starts_with(lit) {
                vec![pos + lit.len()]
            } else {
                vec![]
            }
        }
        Node::Glob(glob) => glob_ends(glob, chars, pos),
        Node::Class { kind, negated } => match chars.get(pos) {
            Some(&c) if kind.matches(c) != *negated => vec![pos + 1],
            _ => vec![],
        },
        Node::Set { ranges, negated } => match chars.get(pos) {
            Some(&c) if in_ranges(ranges, c) != *negated => vec![pos + 1],
            _ => vec![],
        },
        Node::Optional(inner) => {
            let mut out = vec![pos];
            out.extend(match_node(inner, chars, pos));
            dedup(out)
        }
        Node::Alt(alts) => {
            let mut out = Vec::new();
            for a in alts {
                out.extend(match_node(a, chars, pos));
            }
            dedup(out)
        }
        Node::Seq(nodes) => {
            let mut cur = vec![pos];
            for n in nodes {
                let mut next = Vec::new();
                for &p in &cur {
                    next.extend(match_node(n, chars, p));
                }
                cur = dedup(next);
                if cur.is_empty() {
                    break;
                }
            }
            cur
        }
        Node::Plus(inner) => {
            let initial = match_node(inner, chars, pos);
            closure(inner, chars, initial)
        }
        Node::Star(inner) => closure(inner, chars, vec![pos]),
        Node::Except(inner) => {
            if pos >= chars.len() {
                return vec![];
            }
            // Consume one character unless `inner` matches exactly that one
            // character starting here.
            let es = match_node(inner, chars, pos);
            if es.contains(&(pos + 1)) {
                vec![]
            } else {
                vec![pos + 1]
            }
        }
    }
}

/// Expand a set of start positions to the transitive closure under `inner`.
fn closure(inner: &Node, chars: &[char], initial: Vec<usize>) -> Vec<usize> {
    let mut result = dedup(initial);
    let mut i = 0;
    while i < result.len() {
        let p = result[i];
        for q in match_node(inner, chars, p) {
            if !result.contains(&q) {
                result.push(q);
            }
        }
        i += 1;
    }
    dedup(result)
}

fn in_ranges(ranges: &[(char, char)], c: char) -> bool {
    ranges.iter().any(|&(lo, hi)| c >= lo && c <= hi)
}

/// End positions for a WL glob part (`*` any run, `?` any single char).
fn glob_ends(glob: &[char], chars: &[char], pos: usize) -> Vec<usize> {
    match glob.first() {
        None => vec![pos],
        Some('*') => {
            let rest = &glob[1..];
            let mut out = Vec::new();
            for k in pos..=chars.len() {
                out.extend(glob_ends(rest, chars, k));
            }
            dedup(out)
        }
        Some('?') => {
            if pos < chars.len() {
                glob_ends(&glob[1..], chars, pos + 1)
            } else {
                vec![]
            }
        }
        Some(&c) => {
            if chars.get(pos) == Some(&c) {
                glob_ends(&glob[1..], chars, pos + 1)
            } else {
                vec![]
            }
        }
    }
}

/// True when `node` matches the entirety of `chars`.
fn matches_whole(node: &Node, chars: &[char]) -> bool {
    match_node(node, chars, 0).contains(&chars.len())
}

/// Leftmost-longest, non-overlapping `(start, end)` matches in `chars`.
fn find_matches(chars: &[char], node: &Node) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos <= chars.len() {
        let mut best: Option<usize> = None;
        for end in match_node(node, chars, pos) {
            if end > pos && best.is_none_or(|b| end > b) {
                best = Some(end);
            }
        }
        match best {
            Some(end) => {
                out.push((pos, end));
                pos = end;
            }
            None => pos += 1,
        }
    }
    out
}

// ── Tiny regex engine (compiles into `Node`s) ──────────────────────────────

struct RegexParser<'a> {
    src: &'a [char],
    pos: usize,
}

impl<'a> RegexParser<'a> {
    fn new(src: &'a [char]) -> Self {
        RegexParser { src, pos: 0 }
    }

    fn peek(&self) -> Option<char> {
        self.src.get(self.pos).copied()
    }

    fn next(&mut self) -> Option<char> {
        let c = self.peek();
        if c.is_some() {
            self.pos += 1;
        }
        c
    }

    fn parse_alt(&mut self) -> Option<Node> {
        let mut alts = vec![self.parse_seq()?];
        while self.peek() == Some('|') {
            self.pos += 1;
            alts.push(self.parse_seq()?);
        }
        if alts.len() == 1 {
            Some(alts.pop()?)
        } else {
            Some(Node::Alt(alts))
        }
    }

    fn parse_seq(&mut self) -> Option<Node> {
        let mut nodes = Vec::new();
        while let Some(c) = self.peek() {
            if c == ')' || c == '|' {
                break;
            }
            let atom = self.parse_atom()?;
            nodes.push(self.parse_quant(atom)?);
        }
        Some(Node::Seq(nodes))
    }

    fn parse_quant(&mut self, atom: Node) -> Option<Node> {
        match self.peek() {
            Some('*') => {
                self.pos += 1;
                Some(Node::Star(Box::new(atom)))
            }
            Some('+') => {
                self.pos += 1;
                Some(Node::Plus(Box::new(atom)))
            }
            Some('?') => {
                self.pos += 1;
                Some(Node::Optional(Box::new(atom)))
            }
            Some('{') => self.parse_braces(atom),
            _ => Some(atom),
        }
    }

    fn parse_braces(&mut self, atom: Node) -> Option<Node> {
        self.pos += 1; // consume '{'
        let min = self.parse_int()?;
        let (max, unbounded) = match self.peek() {
            Some(',') => {
                self.pos += 1;
                if self.peek() == Some('}') {
                    (None, true)
                } else {
                    (Some(self.parse_int()?), false)
                }
            }
            _ => (Some(min), false),
        };
        if self.next() != Some('}') {
            return None;
        }
        let mut nodes = Vec::new();
        for _ in 0..min {
            nodes.push(atom.clone());
        }
        if unbounded {
            nodes.push(Node::Star(Box::new(atom.clone())));
        } else {
            let max = max?;
            if max < min {
                return None;
            }
            // `(n,m)` expands to `n` fixed copies plus `m - n` optionals.
            for _ in min..max {
                nodes.push(Node::Optional(Box::new(atom.clone())));
            }
        }
        match nodes.len() {
            0 => Some(Node::Seq(vec![])),
            1 => nodes.pop(),
            _ => Some(Node::Seq(nodes)),
        }
    }

    fn parse_int(&mut self) -> Option<usize> {
        let mut n = 0usize;
        let mut any = false;
        while let Some(c) = self.peek() {
            if let Some(d) = c.to_digit(10) {
                n = n * 10 + d as usize;
                any = true;
                self.pos += 1;
            } else {
                break;
            }
        }
        if any { Some(n) } else { None }
    }

    fn parse_atom(&mut self) -> Option<Node> {
        let c = self.next()?;
        match c {
            '(' => {
                if self.peek() == Some('?') {
                    // Skip a non-capturing/flag group prefix `(?:` or `(?...`.
                    self.pos += 1;
                    while let Some(p) = self.peek() {
                        if p == ':' {
                            self.pos += 1;
                            break;
                        }
                        self.pos += 1;
                    }
                }
                let inner = self.parse_alt()?;
                if self.next() != Some(')') {
                    return None;
                }
                Some(inner)
            }
            '[' => self.parse_class(),
            '.' => Some(Node::AnyChar),
            '^' => Some(Node::Start),
            '$' => Some(Node::End),
            '\\' => self.parse_escape(),
            other => Some(Node::Lit(vec![other])),
        }
    }

    fn parse_escape(&mut self) -> Option<Node> {
        let c = self.next()?;
        Some(match c {
            'd' => Node::Class {
                kind: Predefined::Digit,
                negated: false,
            },
            'D' => Node::Class {
                kind: Predefined::Digit,
                negated: true,
            },
            'w' => Node::Class {
                kind: Predefined::Word,
                negated: false,
            },
            'W' => Node::Class {
                kind: Predefined::Word,
                negated: true,
            },
            's' => Node::Class {
                kind: Predefined::Whitespace,
                negated: false,
            },
            'S' => Node::Class {
                kind: Predefined::Whitespace,
                negated: true,
            },
            'n' => Node::Lit(vec!['\n']),
            't' => Node::Lit(vec!['\t']),
            'r' => Node::Lit(vec!['\r']),
            other => Node::Lit(vec![other]),
        })
    }

    fn parse_class(&mut self) -> Option<Node> {
        let negated = if self.peek() == Some('^') {
            self.pos += 1;
            true
        } else {
            false
        };
        let mut ranges: Vec<(char, char)> = Vec::new();
        if self.peek() == Some(']') {
            ranges.push((']', ']'));
            self.pos += 1;
        }
        loop {
            let c = self.next()?;
            if c == ']' {
                break;
            }
            let lo = if c == '\\' {
                let e = self.next()?;
                match e {
                    'd' => {
                        ranges.push(('0', '9'));
                        continue;
                    }
                    'w' => {
                        ranges.push(('0', '9'));
                        ranges.push(('A', 'Z'));
                        ranges.push(('a', 'z'));
                        ranges.push(('_', '_'));
                        continue;
                    }
                    's' => {
                        for ws in [' ', '\t', '\n', '\r'] {
                            ranges.push((ws, ws));
                        }
                        continue;
                    }
                    'n' => '\n',
                    't' => '\t',
                    'r' => '\r',
                    other => other,
                }
            } else {
                c
            };
            if self.peek() == Some('-') && self.src.get(self.pos + 1) != Some(&']') {
                self.pos += 1; // consume '-'
                let hi = self.next()?;
                ranges.push((lo, hi));
            } else {
                ranges.push((lo, lo));
            }
        }
        Some(Node::Set { ranges, negated })
    }
}

fn compile_regex(pattern: &str) -> Option<Node> {
    let chars: Vec<char> = pattern.chars().collect();
    let mut parser = RegexParser::new(&chars);
    let node = parser.parse_alt()?;
    if parser.pos != chars.len() {
        return None;
    }
    Some(node)
}

// ── Argument helpers ───────────────────────────────────────────────────────

fn as_str(v: &Value) -> Option<&str> {
    match v {
        Value::Str(s) => Some(s.as_str()),
        Value::Pattern(Expr::Str(s)) => Some(s.as_str()),
        _ => None,
    }
}

fn as_string(v: &Value) -> Option<String> {
    as_str(v).map(str::to_string)
}

/// Return the call form `head[args…]` unevaluated.
fn unevaluated(head: &str, args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    })
}

// ── Pattern-aware builtins ─────────────────────────────────────────────────

/// `StringMatchQ[string, pattern]` honouring the string-pattern heads.
pub fn builtin_string_match_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 2
        && let Some(node) = parse_value(&args[1])
        && let Some(s) = as_str(&args[0])
    {
        let chars: Vec<char> = s.chars().collect();
        return Ok(Value::Bool(matches_whole(&node, &chars)));
    }
    super::string::builtin_string_match_q(args)
}

/// `StringCases[string, pattern]` honouring the string-pattern heads.
pub fn builtin_string_cases(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 2
        && is_pattern(&args[1])
        && let Some(node) = parse_value(&args[1])
        && let Some(s) = as_str(&args[0])
    {
        let chars: Vec<char> = s.chars().collect();
        let matches: Vec<Value> = find_matches(&chars, &node)
            .into_iter()
            .map(|(a, b)| Value::Str(chars[a..b].iter().collect()))
            .collect();
        return Ok(Value::List(matches));
    }
    super::list::builtin_string_cases(args)
}

/// `StringReplace[string, rule]` honouring the string-pattern heads.
pub fn builtin_string_replace(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 2
        && let Some(s) = as_str(&args[0])
    {
        match &args[1] {
            Value::Rule { lhs, rhs, .. } => {
                if let (Some(node), Some(new)) = (parse_value(lhs), as_string(rhs)) {
                    return Ok(Value::Str(replace_all(s, &node, &new)));
                }
            }
            Value::List(rules) => {
                let mut result = s.to_string();
                for rule in rules {
                    if let Value::Rule { lhs, rhs, .. } = rule
                        && let (Some(node), Some(new)) = (parse_value(lhs), as_string(rhs))
                    {
                        result = replace_all(&result, &node, &new);
                    }
                }
                return Ok(Value::Str(result));
            }
            _ => {}
        }
    }
    super::string::builtin_string_replace(args)
}

/// `StringSplit[string, pattern]` honouring the string-pattern heads.
pub fn builtin_string_split(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() == 2
        && is_pattern(&args[1])
        && let Some(node) = parse_value(&args[1])
        && let Some(s) = as_str(&args[0])
    {
        let chars: Vec<char> = s.chars().collect();
        let mut parts = Vec::new();
        let mut last = 0;
        for (a, b) in find_matches(&chars, &node) {
            parts.push(Value::Str(chars[last..a].iter().collect()));
            last = b;
        }
        parts.push(Value::Str(chars[last..].iter().collect()));
        return Ok(Value::List(parts));
    }
    super::string::builtin_string_split(args)
}

/// Replace every leftmost-longest match of `node` in `s` with `new`.
fn replace_all(s: &str, node: &Node, new: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let matches = find_matches(&chars, node);
    if matches.is_empty() {
        return s.to_string();
    }
    let mut result = String::new();
    let mut last = 0;
    for (a, b) in matches {
        result.extend(chars[last..a].iter());
        result.push_str(new);
        last = b;
    }
    result.extend(chars[last..].iter());
    result
}

// ── StringReplacePart ──────────────────────────────────────────────────────

/// `StringReplacePart[string, new, {m, n}]` — replace characters `m`–`n`.
///
/// `new` may be a string or a list of strings; the third argument may be a
/// single span `{m, n}`, a single character index `n`, a list of spans, or a
/// list of character indices. Negative positions count from the end.
pub fn builtin_string_replace_part(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 3 {
        return unevaluated("StringReplacePart", args);
    }
    let Some(s) = as_str(&args[0]) else {
        return unevaluated("StringReplacePart", args);
    };
    let chars: Vec<char> = s.chars().collect();
    let len = chars.len() as i64;

    let replacements: Vec<String> = match &args[1] {
        Value::List(items) => {
            let mut v = Vec::with_capacity(items.len());
            for it in items {
                match as_string(it) {
                    Some(x) => v.push(x),
                    None => return unevaluated("StringReplacePart", args),
                }
            }
            v
        }
        other => match as_string(other) {
            Some(x) => vec![x],
            None => return unevaluated("StringReplacePart", args),
        },
    };

    let spans: Vec<(i64, i64)> = match &args[2] {
        Value::List(items) if is_span_list(items) => {
            let mut v = Vec::with_capacity(items.len());
            for it in items {
                match resolve_span_value(it, len) {
                    Some(span) => v.push(span),
                    None => return unevaluated("StringReplacePart", args),
                }
            }
            v
        }
        other => match resolve_span_value(other, len) {
            Some(span) => vec![span],
            None => return unevaluated("StringReplacePart", args),
        },
    };

    if spans.len() != replacements.len() {
        return Err(EvalError::Error(format!(
            "StringReplacePart: {} span(s) but {} replacement(s)",
            spans.len(),
            replacements.len()
        )));
    }

    // Apply from the end so earlier indices stay valid.
    let mut order: Vec<usize> = (0..spans.len()).collect();
    order.sort_by_key(|&i| std::cmp::Reverse(spans[i].0));
    let mut result = chars;
    for i in order {
        let (a, b) = spans[i];
        let start = a.max(0) as usize;
        let end = (b + 1) as usize;
        if start > result.len() || end > result.len() {
            return Err(EvalError::Error(format!(
                "StringReplacePart: span {{{}, {}}} out of bounds (length {})",
                a, b, len
            )));
        }
        let mut next: Vec<char> = result[..start].to_vec();
        next.extend(replacements[i].chars());
        next.extend(result[end..].iter());
        result = next;
    }
    Ok(Value::Str(result.iter().collect()))
}

/// True when the value is a list whose elements are themselves span specs.
fn is_span_list(items: &[Value]) -> bool {
    !items.is_empty() && items.iter().all(|v| matches!(v, Value::List(_)))
}

/// Resolve `{m, n}` or a bare index into a 0-based inclusive `(start, end)`.
fn resolve_span_value(v: &Value, len: i64) -> Option<(i64, i64)> {
    match v {
        Value::List(pair) if pair.len() == 2 => {
            let m = pair[0].to_integer()?;
            let n = pair[1].to_integer()?;
            Some((normalize(m, len)?, normalize(n, len)?))
        }
        Value::Integer(_) => {
            let n = v.to_integer()?;
            let idx = normalize(n, len)?;
            Some((idx, idx))
        }
        _ => None,
    }
}

/// Map a WL 1-based (optionally negative) position to a 0-based index.
fn normalize(n: i64, len: i64) -> Option<i64> {
    if n > 0 {
        if n > len { None } else { Some(n - 1) }
    } else if n < 0 {
        let idx = len + n;
        if idx < 0 { None } else { Some(idx) }
    } else {
        None
    }
}

// ── Registration ───────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
///
/// The pattern heads themselves (`StartOfString`, `WordCharacter`, …) are left
/// unregistered so they survive evaluation as plain symbols, which is what the
/// parser and the matcher expect to see.
pub fn register(env: &Env) {
    // Pattern-aware overrides, registered after the literal implementations.
    super::register_builtin(env, "StringMatchQ", builtin_string_match_q);
    super::register_builtin(env, "StringCases", builtin_string_cases);
    super::register_builtin(env, "StringReplace", builtin_string_replace);
    super::register_builtin(env, "StringSplit", builtin_string_split);
    super::register_builtin(env, "StringReplacePart", builtin_string_replace_part);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(x: &str) -> Value {
        Value::Str(x.to_string())
    }
    fn int(n: i64) -> Value {
        Value::Integer(Integer::from(n))
    }
    fn call(head: &str, args: Vec<Value>) -> Value {
        Value::Call {
            head: head.to_string(),
            args,
        }
    }
    fn sym(name: &str) -> Value {
        Value::Symbol(name.to_string())
    }
    fn expr(head: &str, args: Vec<Value>) -> Value {
        Value::Call {
            head: head.to_string(),
            args,
        }
    }
    fn seq(parts: Vec<Value>) -> Value {
        expr("StringExpression", parts)
    }
    fn rule(lhs: Value, rhs: &str) -> Value {
        Value::Rule {
            lhs: Box::new(lhs),
            rhs: Box::new(s(rhs)),
            delayed: false,
        }
    }
    /// Parse a pattern value into a `Node` for engine-level assertions.
    fn pn(v: &Value) -> Node {
        parse_value(v).unwrap()
    }

    // ── Engine primitives ──

    #[test]
    fn test_class_node_matches() {
        let node = parse_head_symbol("WordCharacter").unwrap();
        let chars: Vec<char> = "a".chars().collect();
        assert!(matches_whole(&node, &chars));
        let chars: Vec<char> = "1".chars().collect();
        assert!(matches_whole(&node, &chars));
        let chars: Vec<char> = "-".chars().collect();
        assert!(!matches_whole(&node, &chars));
    }

    #[test]
    fn test_digit_letter_whitespace_nodes() {
        let d = parse_head_symbol("DigitCharacter").unwrap();
        assert!(matches_whole(&d, &['7']));
        assert!(!matches_whole(&d, &['a']));
        let l = parse_head_symbol("LetterCharacter").unwrap();
        assert!(matches_whole(&l, &['Z']));
        assert!(!matches_whole(&l, &['9']));
        let w = parse_head_symbol("WhitespaceCharacter").unwrap();
        assert!(matches_whole(&w, &[' ']));
        assert!(!matches_whole(&w, &['x']));
    }

    #[test]
    fn test_start_end_anchors() {
        let node = pn(&seq(vec![
            sym("StartOfString"),
            s("ab"),
            sym("EndOfString"),
        ]));
        assert!(matches_whole(&node, &['a', 'b']));
        assert!(!matches_whole(&node, &['x', 'a', 'b']));
        let node = pn(&seq(vec![s("ab"), sym("EndOfString")]));
        assert!(matches_whole(&node, &['a', 'b']));
        assert!(!matches_whole(&node, &['a', 'b', 'c']));
    }

    #[test]
    fn test_except() {
        let node = pn(&call("Except", vec![sym("DigitCharacter")]));
        assert!(matches_whole(&node, &['a']));
        assert!(!matches_whole(&node, &['5']));
    }

    #[test]
    fn test_regex_basic() {
        let node = pn(&call("RegularExpression", vec![s("[a-z]+[0-9]{2}")]));
        assert!(matches_whole(&node, &"abc42".chars().collect::<Vec<_>>()));
        assert!(!matches_whole(&node, &"abc4".chars().collect::<Vec<_>>()));
        let anchored = pn(&call("RegularExpression", vec![s("^a.c$")]));
        assert!(matches_whole(&anchored, &['a', 'x', 'c']));
        assert!(!matches_whole(&anchored, &['x', 'a', 'c']));
        let open = pn(&call("RegularExpression", vec![s("a{2,}")]));
        assert!(matches_whole(&open, &['a', 'a', 'a']));
        assert!(!matches_whole(&open, &['a']));
    }

    #[test]
    fn test_regex_alternation_and_escapes() {
        let node = pn(&call("RegularExpression", vec![s(r"\d{2}|\w")]));
        assert!(matches_whole(&node, &['1', '2']));
        assert!(matches_whole(&node, &['_']));
        assert!(!matches_whole(&node, &['-']));
    }

    // ── StringMatchQ ──

    #[test]
    fn test_string_match_q_word_character() {
        assert_eq!(
            builtin_string_match_q(&[s("a"), sym("WordCharacter")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_match_q(&[s("ab"), sym("WordCharacter")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_string_match_q_expression() {
        let pat = seq(vec![
            sym("StartOfString"),
            sym("LetterCharacter"),
            sym("DigitCharacter"),
            sym("EndOfString"),
        ]);
        assert_eq!(
            builtin_string_match_q(&[s("a1"), pat.clone()]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_match_q(&[s("1a"), pat]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_string_match_q_repeated() {
        let pat = seq(vec![
            sym("StartOfString"),
            call("Repeated", vec![sym("DigitCharacter")]),
            sym("EndOfString"),
        ]);
        assert_eq!(
            builtin_string_match_q(&[s("123"), pat.clone()]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_match_q(&[s(""), pat]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_string_match_q_globs_fall_back() {
        assert_eq!(
            builtin_string_match_q(&[s("hello"), s("h*")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_match_q(&[s("hello"), s("world")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_string_match_q_inapplicable_unevaluated() {
        // A non-string, non-pattern second argument falls through to the
        // literal implementation, which reports a type error.
        assert!(builtin_string_match_q(&[s("a"), int(3)]).is_err());
    }

    // ── StringCases ──

    #[test]
    fn test_string_cases_digit_character() {
        let out = builtin_string_cases(&[s("a1b22c"), sym("DigitCharacter")]).unwrap();
        assert_eq!(out, Value::List(vec![s("1"), s("2"), s("2")]));
    }

    #[test]
    fn test_string_cases_expression() {
        let pat = seq(vec![sym("LetterCharacter"), sym("DigitCharacter")]);
        let out = builtin_string_cases(&[s("a1 b2 c3"), pat]).unwrap();
        assert_eq!(out, Value::List(vec![s("a1"), s("b2"), s("c3")]));
    }

    #[test]
    fn test_string_cases_literal_delegates() {
        let out = builtin_string_cases(&[s("abcabc"), s("ab")]).unwrap();
        assert_eq!(out, Value::List(vec![s("ab"), s("ab")]));
    }

    // ── StringReplace ──

    #[test]
    fn test_string_replace_literal_all_occurrences() {
        // Rule literals arrive Pattern-wrapped; replacement covers every match.
        let lhs = Value::Pattern(Expr::Str("a".to_string()));
        let out = builtin_string_replace(&[s("banana"), rule(lhs, "-")]).unwrap();
        assert_eq!(out, s("b-n-n-"));
    }

    #[test]
    fn test_string_replace_pattern() {
        let lhs = Value::Pattern(Expr::Call {
            head: Box::new(Expr::Symbol("StringExpression".to_string())),
            args: vec![
                Expr::Symbol("DigitCharacter".to_string()),
                Expr::Symbol("DigitCharacter".to_string()),
            ],
        });
        let out = builtin_string_replace(&[s("a12b34"), rule(lhs, "#")]).unwrap();
        assert_eq!(out, s("a#b#"));
    }

    #[test]
    fn test_string_replace_anchored() {
        let lhs = Value::Pattern(Expr::Call {
            head: Box::new(Expr::Symbol("StringExpression".to_string())),
            args: vec![
                Expr::Symbol("StartOfString".to_string()),
                Expr::Symbol("LetterCharacter".to_string()),
            ],
        });
        let out = builtin_string_replace(&[s("abc"), rule(lhs, "Z")]).unwrap();
        assert_eq!(out, s("Zbc"));
    }

    // ── StringSplit ──

    #[test]
    fn test_string_split_pattern() {
        let out = builtin_string_split(&[s("a1b22c"), sym("DigitCharacter")]).unwrap();
        assert_eq!(out, Value::List(vec![s("a"), s("b"), s(""), s("c")]));
    }

    #[test]
    fn test_string_split_repeated_pattern() {
        let pat = call("Repeated", vec![sym("DigitCharacter")]);
        let out = builtin_string_split(&[s("a123b45c"), pat]).unwrap();
        assert_eq!(out, Value::List(vec![s("a"), s("b"), s("c")]));
    }

    #[test]
    fn test_string_split_literal_delegates() {
        let out = builtin_string_split(&[s("a,b,c"), s(",")]).unwrap();
        assert_eq!(out, Value::List(vec![s("a"), s("b"), s("c")]));
    }

    // ── StringReplacePart ──

    #[test]
    fn test_string_replace_part_span() {
        let out =
            builtin_string_replace_part(&[s("abcde"), s("XY"), Value::List(vec![int(2), int(3)])])
                .unwrap();
        assert_eq!(out, s("aXYde"));
    }

    #[test]
    fn test_string_replace_part_negative() {
        let out =
            builtin_string_replace_part(&[s("abcde"), s("Z"), Value::List(vec![int(-1), int(-1)])])
                .unwrap();
        assert_eq!(out, s("abcdZ"));
    }

    #[test]
    fn test_string_replace_part_index() {
        let out = builtin_string_replace_part(&[s("abc"), s("X"), int(2)]).unwrap();
        assert_eq!(out, s("aXc"));
    }

    #[test]
    fn test_string_replace_part_multiple() {
        let spans = Value::List(vec![
            Value::List(vec![int(1), int(1)]),
            Value::List(vec![int(3), int(3)]),
        ]);
        let out =
            builtin_string_replace_part(&[s("abc"), Value::List(vec![s("X"), s("Y")]), spans])
                .unwrap();
        assert_eq!(out, s("XbY"));
    }

    #[test]
    fn test_string_replace_part_bad_arity_unevaluated() {
        let out = builtin_string_replace_part(&[s("abc")]).unwrap();
        assert_eq!(
            out,
            Value::Call {
                head: "StringReplacePart".to_string(),
                args: vec![s("abc")],
            }
        );
    }

    // ── Registration ──

    #[test]
    fn test_registered_in_env() {
        let env = Env::new();
        register(&env);
        for name in [
            "StringMatchQ",
            "StringCases",
            "StringReplace",
            "StringSplit",
            "StringReplacePart",
        ] {
            assert!(env.get(name).is_some(), "{name} not registered");
        }
        // Pattern heads stay plain symbols so `parse_value` sees them.
        for name in [
            "StartOfString",
            "EndOfString",
            "WordCharacter",
            "DigitCharacter",
            "LetterCharacter",
            "WhitespaceCharacter",
        ] {
            assert!(env.get(name).is_none(), "{name} should stay a symbol");
        }
    }

    #[test]
    fn test_eval_end_to_end() {
        let env = Env::new();
        crate::builtins::register_builtins(&env);
        let v = crate::eval_input(
            r#"StringMatchQ["abc", StringExpression[StartOfString, WordCharacter, Repeated[WordCharacter], EndOfString]]"#,
            &env,
        )
        .unwrap();
        assert_eq!(v, Value::Bool(true));
        let v = crate::eval_input(r#"StringReplace["aXbXc", "X" -> "-"]"#, &env).unwrap();
        assert_eq!(v, s("a-b-c"));
    }
}
