//! String-pattern heads and regex: `StartOfString`, `EndOfString`,
//! `WordCharacter`, `DigitCharacter`, `LetterCharacter`, `WhitespaceCharacter`,
//! `Except`, `RegularExpression`, plus the pattern-aware overrides for
//! `StringMatchQ`, `StringCases`, `StringReplace`, `StringSplit`,
//! `StringPosition`, `StringCount`, `StringFreeQ`, `StringContainsQ`,
//! `StringStartsQ`, `StringEndsQ`, and `StringReplacePart`.
//!
//! The pattern heads are not callable builtins; they survive as symbols (or as
//! the unevaluated `Except[...]` / `RegularExpression[...]` calls) and are
//! interpreted here as elements of a `StringExpression` sequence. Whenever a
//! request cannot be understood the builtin returns unevaluated so the symbolic
//! form survives.
//!
//! The supported pattern language covers WL string patterns:
//! `*` / `@` / `?` wildcards, `[a-z]` character classes, the named character
//! classes, `StringExpression`, `Alternatives`, `Repeated` / `RepeatedNull`,
//! `Except`, `Shortest` / `Longest`, named `x : pat` sub-patterns (with capture
//! substitution in rule right-hand sides) and `RegularExpression` (compiled to
//! the same node graph). Several builtins accept the `IgnoreCase -> True` and
//! `Overlaps -> True` options.

use crate::ast::Expr;
use crate::env::Env;
use crate::value::{EvalError, Value};
use rug::Integer;

/// A captured sub-span, keyed by the `x` in `x : pat`.
type Caps = Vec<(String, (usize, usize))>;

// ── Pattern AST ────────────────────────────────────────────────────────────

/// One link of a compiled string pattern.
#[derive(Clone, Debug)]
enum Node {
    /// An exact literal run.
    Lit(Vec<char>),
    /// `StartOfString`.
    Start,
    /// `EndOfString`.
    End,
    /// `_` / `Blank[]` / regex `.` — matches any single character.
    AnyChar,
    /// One of the named character classes.
    Class { kind: Predefined, negated: bool },
    /// A bracketed regex / glob character set.
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
    /// `Shortest[p]` — prefer the shortest match (non-greedy).
    Shortest(Box<Node>),
    /// `Longest[p]` — prefer the longest match (the default).
    Longest(Box<Node>),
    /// `x : p` — match `p` and bind the matched substring to `x`.
    Bind { name: String, inner: Box<Node> },
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

fn class(kind: Predefined, negated: bool) -> Node {
    Node::Class { kind, negated }
}

// ── Parsing WL string patterns into `Node`s ────────────────────────────────

/// Parse a runtime value that denotes a string pattern.
fn parse_value(v: &Value) -> Option<Node> {
    match v {
        Value::Str(s) => Some(parse_string_pattern(&s.chars().collect::<Vec<_>>())),
        Value::Symbol(name) | Value::Builtin(name, _) => parse_head_symbol(name),
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
        Expr::Str(s) => Some(parse_string_pattern(&s.chars().collect::<Vec<_>>())),
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
        "WordCharacter" => Some(class(Predefined::Word, false)),
        "DigitCharacter" => Some(class(Predefined::Digit, false)),
        "LetterCharacter" => Some(class(Predefined::Letter, false)),
        "WhitespaceCharacter" => Some(class(Predefined::Whitespace, false)),
        // Alternate spellings accepted by WL for the same classes.
        "Whitespace" => Some(class(Predefined::Whitespace, false)),
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
        "Shortest" if args.len() == 1 => Some(Node::Shortest(Box::new(parse_value(&args[0])?))),
        "Longest" if args.len() == 1 => Some(Node::Longest(Box::new(parse_value(&args[0])?))),
        // `Pattern[x, p]` — only meaningful for a bare symbol name; anything
        // else is transparently the inner pattern.
        "Pattern" if args.len() == 2 => match &args[0] {
            Value::Symbol(name) => Some(Node::Bind {
                name: name.clone(),
                inner: Box::new(parse_value(&args[1])?),
            }),
            _ => parse_value(&args[1]),
        },
        _ => None,
    }
}

/// Compile a WL string pattern (`*`, `@`, `?` wildcards, `[...]` classes and
/// `\` escapes) into a `Node`.
fn parse_string_pattern(chars: &[char]) -> Node {
    let mut nodes: Vec<Node> = Vec::new();
    let mut lit: Vec<char> = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '*' => {
                flush_lit(&mut nodes, &mut lit);
                nodes.push(Node::Star(Box::new(Node::AnyChar)));
                i += 1;
            }
            '@' => {
                flush_lit(&mut nodes, &mut lit);
                nodes.push(Node::Plus(Box::new(Node::AnyChar)));
                i += 1;
            }
            '?' => {
                flush_lit(&mut nodes, &mut lit);
                nodes.push(Node::AnyChar);
                i += 1;
            }
            '\\' => {
                flush_lit(&mut nodes, &mut lit);
                match chars.get(i + 1).copied() {
                    Some('d') => nodes.push(class(Predefined::Digit, false)),
                    Some('D') => nodes.push(class(Predefined::Digit, true)),
                    Some('w') => nodes.push(class(Predefined::Word, false)),
                    Some('W') => nodes.push(class(Predefined::Word, true)),
                    Some('s') => nodes.push(class(Predefined::Whitespace, false)),
                    Some('S') => nodes.push(class(Predefined::Whitespace, true)),
                    Some(other) => nodes.push(Node::Lit(vec![other])),
                    None => nodes.push(Node::Lit(vec!['\\'])),
                }
                let step = if chars.get(i + 1).is_some() { 2 } else { 1 };
                i += step;
            }
            '[' => {
                if let Some((node, next)) = parse_string_class(chars, i) {
                    flush_lit(&mut nodes, &mut lit);
                    nodes.push(node);
                    i = next;
                } else {
                    lit.push('[');
                    i += 1;
                }
            }
            other => {
                lit.push(other);
                i += 1;
            }
        }
    }
    flush_lit(&mut nodes, &mut lit);
    match nodes.len() {
        0 => Node::Lit(vec![]),
        1 => nodes.pop().unwrap(),
        _ => Node::Seq(nodes),
    }
}

fn flush_lit(nodes: &mut Vec<Node>, lit: &mut Vec<char>) {
    if !lit.is_empty() {
        nodes.push(Node::Lit(std::mem::take(lit)));
    }
}

/// Parse a `[...]` character class starting at `start`. Returns the node and
/// the index just past the closing `]`, or `None` when unterminated.
fn parse_string_class(chars: &[char], start: usize) -> Option<(Node, usize)> {
    let mut i = start + 1;
    let negated = if chars.get(i) == Some(&'^') {
        i += 1;
        true
    } else {
        false
    };
    let mut ranges: Vec<(char, char)> = Vec::new();
    // A `]` in the first position is a literal.
    if chars.get(i) == Some(&']') {
        ranges.push((']', ']'));
        i += 1;
    }
    let mut closed = false;
    while i < chars.len() {
        let c = chars[i];
        if c == ']' {
            closed = true;
            i += 1;
            break;
        }
        let lo = if c == '\\' {
            match chars.get(i + 1).copied() {
                Some('d') => {
                    ranges.push(('0', '9'));
                    i += 2;
                    continue;
                }
                Some('w') => {
                    ranges.push(('0', '9'));
                    ranges.push(('A', 'Z'));
                    ranges.push(('a', 'z'));
                    ranges.push(('_', '_'));
                    i += 2;
                    continue;
                }
                Some('s') => {
                    for ws in [' ', '\t', '\n', '\r'] {
                        ranges.push((ws, ws));
                    }
                    i += 2;
                    continue;
                }
                Some(e) => {
                    i += 2;
                    e
                }
                None => {
                    ranges.push(('\\', '\\'));
                    i += 1;
                    continue;
                }
            }
        } else {
            i += 1;
            c
        };
        if chars.get(i) == Some(&'-')
            && chars.get(i + 1).is_some()
            && chars.get(i + 1) != Some(&']')
        {
            i += 1; // consume '-'
            let hi = match chars.get(i).copied() {
                Some('\\') => {
                    let e = chars.get(i + 1).copied().unwrap_or('\\');
                    i += 2;
                    e
                }
                Some(h) => {
                    i += 1;
                    h
                }
                None => {
                    ranges.push((lo, lo));
                    continue;
                }
            };
            ranges.push((lo, hi));
        } else {
            ranges.push((lo, lo));
        }
    }
    if !closed {
        return None;
    }
    Some((Node::Set { ranges, negated }, i))
}

// ── IgnoreCase folding ─────────────────────────────────────────────────────

fn fold_char(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Lower-case a character sequence, preserving the 1:1 character count so the
/// folded and original indices stay aligned.
fn fold_chars(s: &str) -> Vec<char> {
    s.chars().map(fold_char).collect()
}

/// Lower-case the literal parts of a compiled pattern for `IgnoreCase`.
fn fold_node(n: &Node) -> Node {
    match n {
        Node::Lit(v) => Node::Lit(v.iter().map(|c| fold_char(*c)).collect()),
        Node::Set { ranges, negated } => {
            let mut r: Vec<(char, char)> = ranges
                .iter()
                .map(|&(a, b)| {
                    let (a, b) = (fold_char(a), fold_char(b));
                    if a <= b { (a, b) } else { (b, a) }
                })
                .collect();
            r.sort_unstable();
            r.dedup();
            Node::Set {
                ranges: r,
                negated: *negated,
            }
        }
        Node::Star(x) => Node::Star(Box::new(fold_node(x))),
        Node::Plus(x) => Node::Plus(Box::new(fold_node(x))),
        Node::Optional(x) => Node::Optional(Box::new(fold_node(x))),
        Node::Seq(v) => Node::Seq(v.iter().map(fold_node).collect()),
        Node::Alt(v) => Node::Alt(v.iter().map(fold_node).collect()),
        Node::Except(x) => Node::Except(Box::new(fold_node(x))),
        Node::Shortest(x) => Node::Shortest(Box::new(fold_node(x))),
        Node::Longest(x) => Node::Longest(Box::new(fold_node(x))),
        Node::Bind { name, inner } => Node::Bind {
            name: name.clone(),
            inner: Box::new(fold_node(inner)),
        },
        Node::Start | Node::End | Node::AnyChar | Node::Class { .. } => n.clone(),
    }
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
        Node::Shortest(inner) | Node::Longest(inner) | Node::Bind { inner, .. } => {
            match_node(inner, chars, pos)
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

/// True when `node` matches the entirety of `chars`.
fn matches_whole(node: &Node, chars: &[char]) -> bool {
    match_node(node, chars, 0).contains(&chars.len())
}

// ── Capture-aware matching ─────────────────────────────────────────────────

fn dedup_caps(mut v: Vec<(usize, Caps)>) -> Vec<(usize, Caps)> {
    let mut seen: Vec<usize> = Vec::new();
    let mut out = Vec::new();
    for (e, b) in v.drain(..) {
        if !seen.contains(&e) {
            seen.push(e);
            out.push((e, b));
        }
    }
    out
}

/// Capture-aware version of [`match_node`]: every reachable end position paired
/// with the bindings introduced on the way there.
fn cap_node(
    node: &Node,
    chars: &[char],
    pos: usize,
    binds: &[(String, (usize, usize))],
) -> Vec<(usize, Caps)> {
    match node {
        Node::Bind { name, inner } => cap_node(inner, chars, pos, binds)
            .into_iter()
            .map(|(end, mut b)| {
                b.retain(|(n, _)| n != name);
                b.push((name.clone(), (pos, end)));
                (end, b)
            })
            .collect(),
        Node::Seq(nodes) => {
            let mut cur: Vec<(usize, Caps)> = vec![(pos, binds.to_vec())];
            for n in nodes {
                let mut next = Vec::new();
                for (p, b) in &cur {
                    next.extend(cap_node(n, chars, *p, b));
                }
                cur = dedup_caps(next);
                if cur.is_empty() {
                    break;
                }
            }
            cur
        }
        Node::Alt(alts) => {
            let mut out = Vec::new();
            for a in alts {
                out.extend(cap_node(a, chars, pos, binds));
            }
            dedup_caps(out)
        }
        Node::Shortest(inner) | Node::Longest(inner) => cap_node(inner, chars, pos, binds),
        Node::Star(inner) => {
            let mut result: Vec<(usize, Caps)> = vec![(pos, binds.to_vec())];
            expand_caps(inner, chars, &mut result);
            result
        }
        Node::Plus(inner) => {
            let mut result = dedup_caps(cap_node(inner, chars, pos, binds));
            expand_caps(inner, chars, &mut result);
            result
        }
        Node::Optional(inner) => {
            let mut out: Vec<(usize, Caps)> = vec![(pos, binds.to_vec())];
            out.extend(cap_node(inner, chars, pos, binds));
            dedup_caps(out)
        }
        other => match_node(other, chars, pos)
            .into_iter()
            .map(|e| (e, binds.to_vec()))
            .collect(),
    }
}

/// Extend `result` with the transitive closure under `inner`, carrying bindings.
fn expand_caps(inner: &Node, chars: &[char], result: &mut Vec<(usize, Caps)>) {
    let mut i = 0;
    while i < result.len() {
        let (p, b) = result[i].clone();
        for (q, nb) in cap_node(inner, chars, p, &b) {
            if q > p && !result.iter().any(|(e, _)| *e == q) {
                result.push((q, nb));
            }
        }
        i += 1;
    }
}

/// Pick the preferred `(end, bindings)` among the candidates at `pos`.
fn pick(cands: Vec<(usize, Caps)>, pos: usize, shortest: bool) -> Option<(usize, Caps)> {
    let mut best_gt: Option<(usize, Caps)> = None;
    let mut eq: Option<(usize, Caps)> = None;
    for (end, b) in cands {
        if end == pos {
            if eq.is_none() {
                eq = Some((end, b));
            }
            continue;
        }
        if end < pos {
            continue;
        }
        match &best_gt {
            None => best_gt = Some((end, b)),
            Some((be, _)) => {
                let better = if shortest { end < *be } else { end > *be };
                if better {
                    best_gt = Some((end, b));
                }
            }
        }
    }
    best_gt.or(eq)
}

fn node_shortest(node: &Node) -> bool {
    match node {
        Node::Shortest(_) => true,
        Node::Bind { inner, .. } => node_shortest(inner),
        Node::Seq(v) if v.len() == 1 => node_shortest(&v[0]),
        _ => false,
    }
}

/// Leftmost matches as `(start, end, bindings)`. When `overlaps` is set a match
/// is attempted from every start position.
fn find_matches_caps(chars: &[char], node: &Node, overlaps: bool) -> Vec<(usize, usize, Caps)> {
    let shortest = node_shortest(node);
    let mut out = Vec::new();
    let mut pos = 0;
    while pos <= chars.len() {
        let cands = cap_node(node, chars, pos, &[]);
        match pick(cands, pos, shortest) {
            Some((end, b)) => {
                out.push((pos, end, b));
                if overlaps || end == pos {
                    pos += 1;
                } else {
                    pos = end;
                }
            }
            None => pos += 1,
        }
    }
    out
}

/// True when the pattern can match the empty string and nothing longer, used to
/// give `StringSplit[s, ""]` its per-character behaviour.
fn node_zero_width(node: &Node) -> bool {
    match node {
        Node::Lit(v) => v.is_empty(),
        Node::Seq(v) => v.iter().all(node_zero_width),
        Node::Alt(v) => !v.is_empty() && v.iter().all(node_zero_width),
        Node::Shortest(x) | Node::Longest(x) | Node::Bind { inner: x, .. } => node_zero_width(x),
        Node::Optional(_) | Node::Star(_) => true,
        _ => false,
    }
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
            'd' => class(Predefined::Digit, false),
            'D' => class(Predefined::Digit, true),
            'w' => class(Predefined::Word, false),
            'W' => class(Predefined::Word, true),
            's' => class(Predefined::Whitespace, false),
            'S' => class(Predefined::Whitespace, true),
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

/// Unwrap a `Value::Pattern`/`Hold` wrapper to the plain value.
fn unwrap(v: &Value) -> Value {
    match v {
        Value::Pattern(e) => crate::pattern::unwrap_expr_to_value(e),
        Value::Hold(inner) | Value::HoldComplete(inner) => unwrap(inner),
        _ => v.clone(),
    }
}

/// Options accepted by the pattern builtins.
#[derive(Default, Clone, Copy)]
struct Opts {
    ignore_case: bool,
    overlaps: bool,
    overlaps_set: bool,
}

/// Scan trailing arguments (rules or lists of rules) for known options.
fn parse_opts(args: &[Value], start: usize) -> Opts {
    let mut o = Opts::default();
    for a in args.iter().skip(start) {
        let items: &[Value] = match a {
            Value::List(v) => v,
            other => std::slice::from_ref(other),
        };
        for r in items {
            let Value::Rule { lhs, rhs, .. } = r else {
                continue;
            };
            let Some(name) = opt_name(lhs) else {
                continue;
            };
            let b = truthy(rhs).unwrap_or(false);
            match name.as_str() {
                "IgnoreCase" => o.ignore_case = b,
                "Overlaps" => {
                    o.overlaps = b;
                    o.overlaps_set = true;
                }
                _ => {}
            }
        }
    }
    o
}

fn opt_name(v: &Value) -> Option<String> {
    match unwrap(v) {
        Value::Symbol(s) => Some(s),
        _ => None,
    }
}

fn truthy(v: &Value) -> Option<bool> {
    match unwrap(v) {
        Value::Bool(b) => Some(b),
        Value::Symbol(s) | Value::Str(s) if s == "True" => Some(true),
        Value::Symbol(s) | Value::Str(s) if s == "False" => Some(false),
        _ => None,
    }
}

/// Fold a string for matching plus keep the original characters for slicing.
fn match_chars(s: &str, ignore_case: bool) -> (Vec<char>, Vec<char>) {
    let orig: Vec<char> = s.chars().collect();
    let folded = if ignore_case {
        fold_chars(s)
    } else {
        orig.clone()
    };
    (orig, folded)
}

/// Replace free occurrences of capture names in `expr` with their substrings and
/// evaluate the result.
fn eval_rhs(rhs: &Value, binds: &Caps, orig: &[char], whole: (usize, usize)) -> Value {
    let expr = match rhs {
        Value::Pattern(e) => e.clone(),
        other => crate::eval::table::value_to_expr(other),
    };
    let whole_str: String = orig[whole.0..whole.1].iter().collect();
    // Fast paths keep the common cases free of an evaluator round-trip.
    if let Expr::Symbol(name) = &expr
        && let Some((_, (a, b))) = binds.iter().find(|(n, _)| n == name)
    {
        return Value::Str(orig[*a..*b].iter().collect());
    }
    if let Expr::Str(s) = &expr {
        return Value::Str(s.replace("$0", &whole_str));
    }
    let subst = subst_expr(&expr, binds, orig);
    let env = Env::new();
    crate::builtins::register_builtins(&env);
    match crate::eval::eval(&subst, &env) {
        Ok(val) => match val {
            Value::Str(s) if s.contains("$0") => Value::Str(s.replace("$0", &whole_str)),
            other => other,
        },
        Err(_) => Value::Pattern(subst),
    }
}

/// Replace free occurrences of capture names with their (quoted) substrings.
fn subst_expr(e: &Expr, binds: &Caps, orig: &[char]) -> Expr {
    match e {
        Expr::Symbol(name) => match binds.iter().find(|(n, _)| n == name) {
            Some((_, (a, b))) => Expr::Str(orig[*a..*b].iter().collect()),
            None => e.clone(),
        },
        Expr::Call { head, args } => Expr::Call {
            head: head.clone(),
            args: args.iter().map(|a| subst_expr(a, binds, orig)).collect(),
        },
        Expr::List(v) => Expr::List(v.iter().map(|x| subst_expr(x, binds, orig)).collect()),
        other => other.clone(),
    }
}

// ── Pattern-aware builtins ─────────────────────────────────────────────────

/// `StringMatchQ[string, pattern, opts…]` honouring the string-pattern heads.
pub fn builtin_string_match_q(args: &[Value]) -> Result<Value, EvalError> {
    if (args.len() == 2 || args.len() == 3)
        && let Some(s) = as_str(&args[0])
        && let Some(node) = parse_value(&args[1])
    {
        let opts = parse_opts(args, 2);
        let (_, folded) = match_chars(s, opts.ignore_case);
        let node = if opts.ignore_case {
            fold_node(&node)
        } else {
            node
        };
        return Ok(Value::Bool(matches_whole(&node, &folded)));
    }
    super::string::builtin_string_match_q(args)
}

/// `StringCases[string, pattern, opts…]` honouring the string-pattern heads,
/// named captures, `Shortest`/`Longest`, `Overlaps` and rule right-hand sides.
pub fn builtin_string_cases(args: &[Value]) -> Result<Value, EvalError> {
    if !(args.len() == 2 || args.len() == 3) {
        return super::list::builtin_string_cases(args);
    }
    let Some(s) = as_str(&args[0]) else {
        return super::list::builtin_string_cases(args);
    };
    let opts = parse_opts(args, 2);
    let (orig, folded) = match_chars(s, opts.ignore_case);
    let entries: Vec<&Value> = match &args[1] {
        Value::List(v) => v.iter().collect(),
        other => vec![other],
    };
    let mut out = Vec::new();
    let mut parsed_any = false;
    for entry in entries {
        let (lhs, rhs) = match entry {
            Value::Rule { lhs, rhs, .. } => (lhs.as_ref(), Some(rhs.as_ref())),
            other => (other, None),
        };
        let Some(node0) = parse_value(lhs) else {
            continue;
        };
        parsed_any = true;
        let node = if opts.ignore_case {
            fold_node(&node0)
        } else {
            node0
        };
        for (a, b, binds) in find_matches_caps(&folded, &node, opts.overlaps) {
            match rhs {
                Some(r) => out.push(eval_rhs(r, &binds, &orig, (a, b))),
                None => out.push(Value::Str(orig[a..b].iter().collect())),
            }
        }
    }
    if !parsed_any {
        return super::list::builtin_string_cases(args);
    }
    Ok(Value::List(out))
}

/// `StringReplace[string, rule, opts…]` honouring the string-pattern heads.
pub fn builtin_string_replace(args: &[Value]) -> Result<Value, EvalError> {
    if !(args.len() == 2 || args.len() == 3) {
        return super::string::builtin_string_replace(args);
    }
    let Some(s) = as_str(&args[0]) else {
        return super::string::builtin_string_replace(args);
    };
    let max: Option<usize> = match args.get(2) {
        Some(Value::Integer(n)) => n.to_i64().map(|n| if n < 0 { 0 } else { n as usize }),
        _ => None,
    };
    let opts = parse_opts(args, 2);
    let rules: Vec<&Value> = match &args[1] {
        Value::List(v) => v.iter().collect(),
        other => vec![other],
    };
    let mut parsed_any = false;
    let mut result = s.to_string();
    let mut remaining = max;
    for rule in rules {
        let Value::Rule { lhs, rhs, .. } = rule else {
            continue;
        };
        let Some(node0) = parse_value(lhs) else {
            continue;
        };
        parsed_any = true;
        result = replace_all(&result, &node0, rhs, &opts, &mut remaining);
        if remaining == Some(0) {
            break;
        }
    }
    if !parsed_any {
        return super::string::builtin_string_replace(args);
    }
    Ok(Value::Str(result))
}

/// Replace every leftmost match of `node` in `s` with the rule RHS.
fn replace_all(
    s: &str,
    node0: &Node,
    rhs: &Value,
    opts: &Opts,
    remaining: &mut Option<usize>,
) -> String {
    let (orig, folded) = match_chars(s, opts.ignore_case);
    let node = if opts.ignore_case {
        fold_node(node0)
    } else {
        node0.clone()
    };
    let matches = find_matches_caps(&folded, &node, false);
    if matches.is_empty() {
        return s.to_string();
    }
    let mut result = String::new();
    let mut last = 0;
    let mut done = 0usize;
    for (a, b, binds) in matches {
        if let Some(rem) = remaining
            && done >= *rem
        {
            break;
        }
        result.extend(orig[last..a].iter());
        let replacement = eval_rhs(rhs, &binds, &orig, (a, b));
        match replacement {
            Value::Str(r) => result.push_str(&r),
            other => result.push_str(&other.to_string()),
        }
        last = b;
        done += 1;
    }
    result.extend(orig[last..].iter());
    if let Some(rem) = remaining {
        *rem = rem.saturating_sub(done);
    }
    result
}

/// `StringSplit[string, pattern, opts…]` honouring the string-pattern heads.
pub fn builtin_string_split(args: &[Value]) -> Result<Value, EvalError> {
    if !(args.len() == 2 || args.len() == 3) {
        return super::string::builtin_string_split(args);
    }
    let Some(s) = as_str(&args[0]) else {
        return super::string::builtin_string_split(args);
    };
    let Some(node0) = parse_value(&args[1]) else {
        return super::string::builtin_string_split(args);
    };
    let opts = parse_opts(args, 2);
    let (orig, folded) = match_chars(s, opts.ignore_case);
    let node = if opts.ignore_case {
        fold_node(&node0)
    } else {
        node0
    };
    if node_zero_width(&node) {
        return Ok(Value::List(
            orig.iter().map(|c| Value::Str(c.to_string())).collect(),
        ));
    }
    let mut parts = Vec::new();
    let mut last = 0;
    for (a, b, _) in find_matches_caps(&folded, &node, false) {
        parts.push(Value::Str(orig[last..a].iter().collect()));
        last = b;
    }
    parts.push(Value::Str(orig[last..].iter().collect()));
    Ok(Value::List(parts))
}

/// `StringPosition[string, pattern, opts…]` — `{start, end}` spans (1-based).
pub fn builtin_string_position(args: &[Value]) -> Result<Value, EvalError> {
    let Some((_orig, folded, node, opts)) = prepare_scan("StringPosition", args) else {
        return super::string::builtin_string_position(args);
    };
    // `StringPosition` reports overlapping matches by default.
    let overlaps = if opts.overlaps_set {
        opts.overlaps
    } else {
        true
    };
    let out: Vec<Value> = find_matches_caps(&folded, &node, overlaps)
        .into_iter()
        .map(|(a, b, _)| {
            Value::List(vec![
                Value::Integer(Integer::from(a as i64 + 1)),
                Value::Integer(Integer::from(b as i64)),
            ])
        })
        .collect();
    Ok(Value::List(out))
}

/// `StringCount[string, pattern, opts…]` — number of matches.
pub fn builtin_string_count(args: &[Value]) -> Result<Value, EvalError> {
    let Some((_orig, folded, node, opts)) = prepare_scan("StringCount", args) else {
        return super::string::builtin_string_count(args);
    };
    let n = find_matches_caps(&folded, &node, opts.overlaps).len();
    Ok(Value::Integer(Integer::from(n as i64)))
}

/// `StringFreeQ[string, pattern]` — True when no match occurs.
pub fn builtin_string_free_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some((_orig, folded, node, _)) = prepare_scan("StringFreeQ", args) else {
        return super::string::builtin_string_free_q(args);
    };
    Ok(Value::Bool(
        find_matches_caps(&folded, &node, true).is_empty(),
    ))
}

/// `StringContainsQ[string, pattern]` — True when any match occurs.
pub fn builtin_string_contains_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some((_orig, folded, node, _)) = prepare_scan("StringContainsQ", args) else {
        return super::string::builtin_string_contains_q(args);
    };
    Ok(Value::Bool(
        !find_matches_caps(&folded, &node, true).is_empty(),
    ))
}

/// `StringStartsQ[string, pattern]` — True when a match begins at the start.
pub fn builtin_string_starts_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some((_orig, folded, node, _)) = prepare_scan("StringStartsQ", args) else {
        return super::string::builtin_string_starts_q(args);
    };
    Ok(Value::Bool(!cap_node(&node, &folded, 0, &[]).is_empty()))
}

/// `StringEndsQ[string, pattern]` — True when a match ends at the end.
pub fn builtin_string_ends_q(args: &[Value]) -> Result<Value, EvalError> {
    let Some((_orig, folded, node, _)) = prepare_scan("StringEndsQ", args) else {
        return super::string::builtin_string_ends_q(args);
    };
    let len = folded.len();
    let found = (0..=len).any(|start| {
        cap_node(&node, &folded, start, &[])
            .iter()
            .any(|(end, _)| *end == len)
    });
    Ok(Value::Bool(found))
}

/// Shared preparation for the boolean/counting scan builtins.
fn prepare_scan(_head: &str, args: &[Value]) -> Option<(Vec<char>, Vec<char>, Node, Opts)> {
    if !(args.len() == 2 || args.len() == 3) {
        return None;
    }
    let s = as_str(&args[0])?;
    let node0 = parse_value(&args[1])?;
    let opts = parse_opts(args, 2);
    let (orig, folded) = match_chars(s, opts.ignore_case);
    let node = if opts.ignore_case {
        fold_node(&node0)
    } else {
        node0
    };
    Some((orig, folded, node, opts))
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
    super::register_builtin(env, "StringPosition", builtin_string_position);
    super::register_builtin(env, "StringCount", builtin_string_count);
    super::register_builtin(env, "StringFreeQ", builtin_string_free_q);
    super::register_builtin(env, "StringContainsQ", builtin_string_contains_q);
    super::register_builtin(env, "StringStartsQ", builtin_string_starts_q);
    super::register_builtin(env, "StringEndsQ", builtin_string_ends_q);
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
    /// A rule whose RHS is an arbitrary expression.
    fn rule_expr(lhs: Value, rhs: Value) -> Value {
        Value::Rule {
            lhs: Box::new(lhs),
            rhs: Box::new(rhs),
            delayed: false,
        }
    }
    fn pat(name: &str, inner: Value) -> Value {
        call("Pattern", vec![sym(name), inner])
    }
    /// Parse a pattern value into a `Node` for engine-level assertions.
    fn pn(v: &Value) -> Node {
        parse_value(v).unwrap()
    }

    // ── Engine primitives ──

    #[test]
    fn test_class_node_matches() {
        let node = parse_head_symbol("WordCharacter").unwrap();
        assert!(matches_whole(&node, &['a']));
        assert!(matches_whole(&node, &['1']));
        assert!(!matches_whole(&node, &['-']));
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

    // ── Bare-string pattern language ──

    #[test]
    fn test_glob_parser_wildcards() {
        // `*` = zero-or-more, `@` = one-or-more, `?` = exactly one.
        assert!(matches_whole(
            &pn(&s("h*")),
            &"hello".chars().collect::<Vec<_>>()
        ));
        assert!(matches_whole(
            &pn(&s("h@")),
            &"hello".chars().collect::<Vec<_>>()
        ));
        assert!(!matches_whole(
            &pn(&s("h@x")),
            &"hello".chars().collect::<Vec<_>>()
        ));
        assert!(matches_whole(
            &pn(&s("h?llo")),
            &"hello".chars().collect::<Vec<_>>()
        ));
        assert!(matches_whole(
            &pn(&s("h*o")),
            &"hello".chars().collect::<Vec<_>>()
        ));
        assert!(!matches_whole(
            &pn(&s("h@o")),
            &"ho".chars().collect::<Vec<_>>()
        ));
    }

    #[test]
    fn test_glob_parser_classes() {
        assert!(matches_whole(&pn(&s("[a-z]")), &['q']));
        assert!(!matches_whole(&pn(&s("[a-z]")), &['Q']));
        assert!(matches_whole(&pn(&s("[^0-9]")), &['x']));
        assert!(!matches_whole(&pn(&s("[^0-9]")), &['7']));
        // A `]` first is a literal; ranges and escapes inside classes work.
        assert!(matches_whole(&pn(&s("[]a]")), &[']']));
        assert!(matches_whole(&pn(&s(r"[\d]")), &['4']));
        // Unterminated class falls back to a literal `[`.
        assert!(matches_whole(
            &pn(&s("[abc")),
            &"[abc".chars().collect::<Vec<_>>()
        ));
    }

    #[test]
    fn test_glob_escape() {
        // `\*` matches a literal star.
        assert!(matches_whole(&pn(&s(r"\*")), &['*']));
        assert!(matches_whole(&pn(&s(r"a\d")), &['a', '3']));
    }

    #[test]
    fn test_shortest_and_longest_flags() {
        let chars: Vec<char> = "aaa".chars().collect();
        let shortest = pn(&call("Shortest", vec![call("Repeated", vec![s("a")])]));
        let ms: Vec<(usize, usize)> = find_matches_caps(&chars, &shortest, false)
            .into_iter()
            .map(|(a, b, _)| (a, b))
            .collect();
        assert_eq!(ms, vec![(0, 1), (1, 2), (2, 3)]);
        let longest = pn(&call("Longest", vec![call("Repeated", vec![s("a")])]));
        let ml: Vec<(usize, usize)> = find_matches_caps(&chars, &longest, false)
            .into_iter()
            .map(|(a, b, _)| (a, b))
            .collect();
        assert_eq!(ml, vec![(0, 3)]);
    }

    #[test]
    fn test_bind_capture() {
        let node = pn(&pat("x", call("Repeated", vec![sym("DigitCharacter")])));
        let chars: Vec<char> = "ab123c".chars().collect();
        let m = find_matches_caps(&chars, &node, false);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].0, 2);
        assert_eq!(m[0].1, 5);
        assert_eq!(m[0].2, vec![("x".to_string(), (2usize, 5usize))]);
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
    fn test_string_match_q_wildcards() {
        assert_eq!(
            builtin_string_match_q(&[s("hello"), s("h*")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_match_q(&[s("hello"), s("h@o")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_match_q(&[s("hello"), s("h?llo")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_match_q(&[s("hello"), s("world")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_string_match_q_char_class() {
        assert_eq!(
            builtin_string_match_q(&[s("b"), s("[a-z]")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_match_q(&[s("B"), s("[a-z]")]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_string_match_q(&[s("abc9"), s("[a-z]*[0-9]")]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_string_match_q_ignore_case() {
        let opt = rule(sym("IgnoreCase"), "True");
        assert_eq!(
            builtin_string_match_q(&[s("HELLO"), s("hello"), opt.clone()]).unwrap(),
            Value::Bool(true)
        );
        // A class folds too.
        assert_eq!(
            builtin_string_match_q(&[s("B"), s("[a-z]"), opt]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_match_q(&[s("HELLO"), s("hello")]).unwrap(),
            Value::Bool(false)
        );
        // Option as an explicit list.
        assert_eq!(
            builtin_string_match_q(&[
                s("HELLO"),
                s("h*"),
                Value::List(vec![rule(sym("IgnoreCase"), "True")])
            ])
            .unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_string_match_q_inapplicable_unevaluated() {
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
    fn test_string_cases_literal() {
        let out = builtin_string_cases(&[s("abcabc"), s("ab")]).unwrap();
        assert_eq!(out, Value::List(vec![s("ab"), s("ab")]));
    }

    #[test]
    fn test_string_cases_named_capture_rule() {
        let lhs = pat("x", call("Repeated", vec![sym("DigitCharacter")]));
        let out = builtin_string_cases(&[s("a1b22c"), rule(lhs.clone(), "x")]).unwrap();
        // RHS `"x"` is the string "x"; use a symbol RHS for capture substitution.
        assert_eq!(out, Value::List(vec![s("x"), s("x")]));
        let out = builtin_string_cases(&[s("a1b22c"), rule_expr(lhs, sym("x"))]).unwrap();
        assert_eq!(out, Value::List(vec![s("1"), s("22")]));
    }

    #[test]
    fn test_string_cases_named_capture_stringjoin() {
        let lhs = pat("x", call("Repeated", vec![sym("DigitCharacter")]));
        let rhs = call("StringJoin", vec![s("<"), sym("x"), s(">")]);
        let out = builtin_string_cases(&[s("a1b22c"), rule_expr(lhs, rhs)]).unwrap();
        assert_eq!(out, Value::List(vec![s("<1>"), s("<22>")]));
    }

    #[test]
    fn test_string_cases_shortest_overlaps() {
        let shortest = call("Shortest", vec![call("Repeated", vec![s("a")])]);
        let out = builtin_string_cases(&[s("aaa"), shortest]).unwrap();
        assert_eq!(out, Value::List(vec![s("a"), s("a"), s("a")]));
    }

    #[test]
    fn test_string_cases_overlaps_option() {
        let out =
            builtin_string_cases(&[s("aaa"), s("aa"), rule(sym("Overlaps"), "True")]).unwrap();
        assert_eq!(out, Value::List(vec![s("aa"), s("aa")]));
    }

    #[test]
    fn test_string_cases_multiple_patterns() {
        let pats = Value::List(vec![sym("DigitCharacter"), s("[a-z]")]);
        let out = builtin_string_cases(&[s("a1b2"), pats]).unwrap();
        // Patterns are applied in order: digits first, then letters.
        assert_eq!(out, Value::List(vec![s("1"), s("2"), s("a"), s("b")]));
    }

    #[test]
    fn test_string_cases_multiple_rules() {
        let rules = Value::List(vec![
            rule_expr(sym("DigitCharacter"), s("#")),
            rule_expr(sym("LetterCharacter"), s("@")),
        ]);
        let out = builtin_string_cases(&[s("a1b2"), rules]).unwrap();
        assert_eq!(out, Value::List(vec![s("#"), s("#"), s("@"), s("@")]));
    }

    #[test]
    fn test_string_cases_ignore_case() {
        let out =
            builtin_string_cases(&[s("AbCaD"), s("a"), rule(sym("IgnoreCase"), "True")]).unwrap();
        // The original casing is preserved in the output.
        assert_eq!(out, Value::List(vec![s("A"), s("a")]));
    }

    // ── StringReplace ──

    #[test]
    fn test_string_replace_literal_all_occurrences() {
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

    #[test]
    fn test_string_replace_regex() {
        let lhs = call("RegularExpression", vec![s("[0-9]+")]);
        let out = builtin_string_replace(&[s("a12b345"), rule(lhs, "#")]).unwrap();
        assert_eq!(out, s("a#b#"));
    }

    #[test]
    fn test_string_replace_alternatives() {
        let lhs = call("Alternatives", vec![s("cat"), s("dog")]);
        let out = builtin_string_replace(&[s("a cat and a dog"), rule(lhs, "pet")]).unwrap();
        assert_eq!(out, s("a pet and a pet"));
    }

    #[test]
    fn test_string_replace_captures() {
        let lhs = pat("x", call("Repeated", vec![sym("DigitCharacter")]));
        let rhs = call("StringJoin", vec![s("<"), sym("x"), s(">")]);
        let out = builtin_string_replace(&[s("a1b22"), rule_expr(lhs, rhs)]).unwrap();
        assert_eq!(out, s("a<1>b<22>"));
    }

    #[test]
    fn test_string_replace_max_count() {
        let out = builtin_string_replace(&[s("aaaa"), rule(s("a"), "-")]).unwrap();
        assert_eq!(out, s("----"));
        let out = builtin_string_replace(&[s("aaaa"), rule(s("a"), "-"), int(2)]).unwrap();
        assert_eq!(out, s("--aa"));
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
    fn test_string_split_literal() {
        let out = builtin_string_split(&[s("a,b,c"), s(",")]).unwrap();
        assert_eq!(out, Value::List(vec![s("a"), s("b"), s("c")]));
    }

    #[test]
    fn test_string_split_regex() {
        let pat = call("RegularExpression", vec![s("\\s+")]);
        let out = builtin_string_split(&[s("a b   c"), pat]).unwrap();
        assert_eq!(out, Value::List(vec![s("a"), s("b"), s("c")]));
    }

    #[test]
    fn test_string_split_empty() {
        let out = builtin_string_split(&[s("abc"), s("")]).unwrap();
        assert_eq!(out, Value::List(vec![s("a"), s("b"), s("c")]));
    }

    // ── StringPosition / StringCount / predicates ──

    #[test]
    fn test_string_position_pattern() {
        let out = builtin_string_position(&[s("a1b22c"), sym("DigitCharacter")]).unwrap();
        assert_eq!(
            out,
            Value::List(vec![
                Value::List(vec![int(2), int(2)]),
                Value::List(vec![int(4), int(4)]),
                Value::List(vec![int(5), int(5)]),
            ])
        );
    }

    #[test]
    fn test_string_position_literal_overlapping() {
        let out = builtin_string_position(&[s("aaa"), s("aa")]).unwrap();
        assert_eq!(
            out,
            Value::List(vec![
                Value::List(vec![int(1), int(2)]),
                Value::List(vec![int(2), int(3)]),
            ])
        );
    }

    #[test]
    fn test_string_count_pattern() {
        let out = builtin_string_count(&[s("a1b22c"), sym("DigitCharacter")]).unwrap();
        assert_eq!(out, int(3));
        let out = builtin_string_count(&[s("aaaa"), s("aa")]).unwrap();
        assert_eq!(out, int(2));
    }

    #[test]
    fn test_string_count_overlaps_option() {
        let out =
            builtin_string_count(&[s("aaaa"), s("aa"), rule(sym("Overlaps"), "True")]).unwrap();
        assert_eq!(out, int(3));
    }

    #[test]
    fn test_string_predicates_with_patterns() {
        assert_eq!(
            builtin_string_contains_q(&[s("a1b2"), sym("DigitCharacter")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_contains_q(&[s("abc"), sym("DigitCharacter")]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_string_free_q(&[s("abc"), sym("DigitCharacter")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_starts_q(&[s("a1b"), sym("LetterCharacter")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_starts_q(&[s("1ab"), sym("LetterCharacter")]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_string_ends_q(&[s("ab1"), sym("DigitCharacter")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_ends_q(&[s("1ab"), sym("DigitCharacter")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_string_predicates_ignore_case() {
        let opt = rule(sym("IgnoreCase"), "True");
        assert_eq!(
            builtin_string_contains_q(&[s("ABC"), s("b"), opt.clone()]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_starts_q(&[s("ABC"), s("ab"), opt.clone()]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_string_ends_q(&[s("XYZ"), s("yz"), opt]).unwrap(),
            Value::Bool(true)
        );
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
            "StringPosition",
            "StringCount",
            "StringFreeQ",
            "StringContainsQ",
            "StringStartsQ",
            "StringEndsQ",
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
        // Parser-level `..` and `x : pat` support.
        let v = crate::eval_input(r#"StringCases["a1b22c", DigitCharacter..]"#, &env).unwrap();
        assert_eq!(v, Value::List(vec![s("1"), s("22")]));
        let v = crate::eval_input(r#"StringCases["a1b22c", x : DigitCharacter .. :> x]"#, &env)
            .unwrap();
        assert_eq!(v, Value::List(vec![s("1"), s("22")]));
        let v =
            crate::eval_input(r#"StringMatchQ["B", "[a-z]", IgnoreCase -> True]"#, &env).unwrap();
        assert_eq!(v, Value::Bool(true));
        let v = crate::eval_input(r#"StringSplit["a1b2c", RegularExpression["[0-9]+"]]"#, &env)
            .unwrap();
        assert_eq!(v, Value::List(vec![s("a"), s("b"), s("c")]));
    }
}
