use rug::Float;
use rug::Integer;
/// AST node definitions for Syma language.
///
/// Everything in Syma is an expression. The AST mirrors the expression
/// structure: `head[arg1, arg2, ...]`.
use std::fmt;

/// A symbol identifier.
pub type Symbol = String;

/// Core expression type — the universal AST node.
#[derive(Debug, Clone)]
pub enum Expr {
    // ── Atoms ──
    Integer(Integer),
    Real(Float),
    Complex {
        re: f64,
        im: f64,
    },
    Str(String),
    Bool(bool),
    Symbol(Symbol),
    Null,

    // ── Compound ──
    /// f[arg1, arg2, ...] — function application / compound expression
    Call {
        head: Box<Expr>,
        args: Vec<Expr>,
    },

    /// {a, b, c} — list literal, sugar for List[a, b, c]
    List(Vec<Expr>),

    /// <|"key" -> val, ...|> — association (hash map)
    Assoc(Vec<(String, Expr)>),

    /// a -> b — immediate rule
    Rule {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },

    /// a :> b — delayed rule
    RuleDelayed {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },

    // ── Function constructs ──
    /// #, #1, #2 — slot in pure function
    Slot(Option<usize>),

    /// Function[{x, y}, body] — named-parameter lambda
    Function {
        params: Vec<Symbol>,
        body: Box<Expr>,
    },

    /// expr & — pure function sugar (lowest-precedence postfix)
    Pure {
        body: Box<Expr>,
    },

    // ── Pattern nodes (appear in function definitions and rules) ──
    /// _ — anonymous blank
    Blank {
        type_constraint: Option<Symbol>,
    },

    /// x_ or x_Integer — named blank
    NamedBlank {
        name: Symbol,
        type_constraint: Option<Symbol>,
    },

    /// __ — sequence blank (1+)
    BlankSequence {
        name: Option<Symbol>,
        type_constraint: Option<Symbol>,
    },

    /// ___ — optional sequence blank (0+)
    BlankNullSequence {
        name: Option<Symbol>,
        type_constraint: Option<Symbol>,
    },

    /// pattern /; guard
    PatternGuard {
        pattern: Box<Expr>,
        condition: Box<Expr>,
    },

    /// _.— optional blank (defaults to Null if unmatched, or expr with :default)
    OptionalBlank {
        type_constraint: Option<Symbol>,
        /// Default value expression (e.g., 5 in _:5). None means fallback to Null.
        default_value: Option<Box<Expr>>,
    },

    /// x_. or x_Integer.— named optional blank
    OptionalNamedBlank {
        name: Symbol,
        type_constraint: Option<Symbol>,
        /// Default value expression (e.g., 5 in x_:5). None means fallback to Null.
        default_value: Option<Box<Expr>>,
    },

    // ── Special forms ──
    /// a /. rules — replace all
    ReplaceAll {
        expr: Box<Expr>,
        rules: Box<Expr>,
    },

    /// a //. rules — replace repeated
    ReplaceRepeated {
        expr: Box<Expr>,
        rules: Box<Expr>,
    },

    /// f /@ list — map
    Map {
        func: Box<Expr>,
        list: Box<Expr>,
    },

    /// f @@ expr — apply
    Apply {
        func: Box<Expr>,
        expr: Box<Expr>,
    },

    /// a // f — postfix application
    Pipe {
        expr: Box<Expr>,
        func: Box<Expr>,
    },

    /// f @ x — prefix application
    Prefix {
        func: Box<Expr>,
        arg: Box<Expr>,
    },

    // ── Control flow ──
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Option<Box<Expr>>,
    },

    Which {
        pairs: Vec<(Expr, Expr)>,
    },

    Switch {
        expr: Box<Expr>,
        cases: Vec<(Expr, Expr)>,
    },

    Match {
        expr: Box<Expr>,
        branches: Vec<MatchBranch>,
    },

    For {
        init: Box<Expr>,
        condition: Box<Expr>,
        step: Box<Expr>,
        body: Box<Expr>,
    },

    While {
        condition: Box<Expr>,
        body: Box<Expr>,
    },

    Do {
        body: Box<Expr>,
        iterator: IteratorSpec,
    },

    // ── Definitions ──
    /// f[x_] := body — function definition
    FuncDef {
        name: Symbol,
        params: Vec<Expr>,
        body: Box<Expr>,
        delayed: bool,
    },

    /// x = value — assignment
    Assign {
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },

    /// {a, b} = value — destructuring assignment
    DestructAssign {
        patterns: Vec<Expr>,
        rhs: Box<Expr>,
    },

    /// x++ — post-increment (returns old value)
    PostIncrement {
        expr: Box<Expr>,
    },

    /// x-- — post-decrement (returns old value)
    PostDecrement {
        expr: Box<Expr>,
    },

    /// x =. — unset (clear definition)
    Unset {
        expr: Box<Expr>,
    },

    /// rule name = { ... }
    RuleDef {
        name: Symbol,
        rules: Vec<(Expr, Expr)>,
    },

    // ── Class/Module (parsed but simplified in Phase 1) ──
    ClassDef {
        name: Symbol,
        parent: Option<Symbol>,
        mixins: Vec<Symbol>,
        members: Vec<MemberDef>,
    },

    ModuleDef {
        name: Symbol,
        exports: Vec<Symbol>,
        body: Vec<Expr>,
    },

    Import {
        module: Vec<Symbol>,
        selective: Option<Vec<Symbol>>,
        alias: Option<Symbol>,
    },

    Export(Vec<Symbol>),

    // ── Sequence ──
    /// a; b; c — sequence, evaluates all, returns last
    Sequence(Vec<Expr>),

    // ── Hold ──
    Hold(Box<Expr>),
    HoldComplete(Box<Expr>),
    ReleaseHold(Box<Expr>),

    // ── Help ──
    /// ?expr — information/help query
    Information(Box<Expr>),
}

/// A branch in a match expression: pattern => result
#[derive(Debug, Clone)]
pub struct MatchBranch {
    pub pattern: Expr,
    pub result: Expr,
}

/// Iterator specification in Do loops: {i, min, max} or {i, list}
#[derive(Debug, Clone)]
pub enum IteratorSpec {
    Range {
        var: Symbol,
        min: Box<Expr>,
        max: Box<Expr>,
    },
    List {
        var: Symbol,
        list: Box<Expr>,
    },
}

/// Class member definition
#[derive(Debug, Clone)]
pub enum MemberDef {
    Field {
        name: Symbol,
        type_hint: Option<String>,
        default: Option<Expr>,
    },
    Method {
        name: Symbol,
        params: Vec<Expr>,
        return_type: Option<String>,
        body: MethodBody,
    },
    Constructor {
        params: Vec<Expr>,
        body: Vec<Expr>,
    },
    Transform {
        name: Symbol,
        rules: Vec<(Expr, Expr)>,
    },
}

/// Method body — either an expression or a block
#[derive(Debug, Clone)]
pub enum MethodBody {
    Expr(Expr),
    Block(Vec<Expr>),
}

// ── PartialEq implementations ──

impl PartialEq for Expr {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Expr::Integer(a), Expr::Integer(b)) => a == b,
            (Expr::Real(a), Expr::Real(b)) => a == b,
            (Expr::Complex { re: a1, im: a2 }, Expr::Complex { re: b1, im: b2 }) => {
                a1 == b1 && a2 == b2
            }
            (Expr::Str(a), Expr::Str(b)) => a == b,
            (Expr::Bool(a), Expr::Bool(b)) => a == b,
            (Expr::Symbol(a), Expr::Symbol(b)) => a == b,
            (Expr::Null, Expr::Null) => true,
            (Expr::Call { head: h1, args: a1 }, Expr::Call { head: h2, args: a2 }) => {
                h1 == h2 && a1 == a2
            }
            (Expr::List(a), Expr::List(b)) => a == b,
            (Expr::Assoc(a), Expr::Assoc(b)) => a == b,
            (Expr::Rule { lhs: l1, rhs: r1 }, Expr::Rule { lhs: l2, rhs: r2 }) => {
                l1 == l2 && r1 == r2
            }
            (Expr::RuleDelayed { lhs: l1, rhs: r1 }, Expr::RuleDelayed { lhs: l2, rhs: r2 }) => {
                l1 == l2 && r1 == r2
            }
            (Expr::Slot(a), Expr::Slot(b)) => a == b,
            (Expr::Pure { body: a }, Expr::Pure { body: b }) => a == b,
            (
                Expr::Function {
                    params: p1,
                    body: b1,
                },
                Expr::Function {
                    params: p2,
                    body: b2,
                },
            ) => p1 == p2 && b1 == b2,
            (Expr::Blank { type_constraint: a }, Expr::Blank { type_constraint: b }) => a == b,
            (
                Expr::NamedBlank {
                    name: n1,
                    type_constraint: t1,
                },
                Expr::NamedBlank {
                    name: n2,
                    type_constraint: t2,
                },
            ) => n1 == n2 && t1 == t2,
            (
                Expr::BlankSequence {
                    name: n1,
                    type_constraint: t1,
                },
                Expr::BlankSequence {
                    name: n2,
                    type_constraint: t2,
                },
            ) => n1 == n2 && t1 == t2,
            (
                Expr::BlankNullSequence {
                    name: n1,
                    type_constraint: t1,
                },
                Expr::BlankNullSequence {
                    name: n2,
                    type_constraint: t2,
                },
            ) => n1 == n2 && t1 == t2,
            (
                Expr::PatternGuard {
                    pattern: p1,
                    condition: c1,
                },
                Expr::PatternGuard {
                    pattern: p2,
                    condition: c2,
                },
            ) => p1 == p2 && c1 == c2,
            (
                Expr::OptionalBlank {
                    type_constraint: a,
                    default_value: d1,
                },
                Expr::OptionalBlank {
                    type_constraint: b,
                    default_value: d2,
                },
            ) => a == b && d1 == d2,
            (
                Expr::OptionalNamedBlank {
                    name: n1,
                    type_constraint: t1,
                    default_value: d1,
                },
                Expr::OptionalNamedBlank {
                    name: n2,
                    type_constraint: t2,
                    default_value: d2,
                },
            ) => n1 == n2 && t1 == t2 && d1 == d2,
            (
                Expr::ReplaceAll {
                    expr: e1,
                    rules: r1,
                },
                Expr::ReplaceAll {
                    expr: e2,
                    rules: r2,
                },
            ) => e1 == e2 && r1 == r2,
            (
                Expr::ReplaceRepeated {
                    expr: e1,
                    rules: r1,
                },
                Expr::ReplaceRepeated {
                    expr: e2,
                    rules: r2,
                },
            ) => e1 == e2 && r1 == r2,
            (Expr::Map { func: f1, list: l1 }, Expr::Map { func: f2, list: l2 }) => {
                f1 == f2 && l1 == l2
            }
            (Expr::Apply { func: f1, expr: e1 }, Expr::Apply { func: f2, expr: e2 }) => {
                f1 == f2 && e1 == e2
            }
            (Expr::Pipe { expr: e1, func: f1 }, Expr::Pipe { expr: e2, func: f2 }) => {
                e1 == e2 && f1 == f2
            }
            (Expr::Prefix { func: f1, arg: a1 }, Expr::Prefix { func: f2, arg: a2 }) => {
                f1 == f2 && a1 == a2
            }
            (
                Expr::If {
                    condition: c1,
                    then_branch: t1,
                    else_branch: e1,
                },
                Expr::If {
                    condition: c2,
                    then_branch: t2,
                    else_branch: e2,
                },
            ) => c1 == c2 && t1 == t2 && e1 == e2,
            (Expr::Which { pairs: p1 }, Expr::Which { pairs: p2 }) => p1 == p2,
            (
                Expr::Switch {
                    expr: e1,
                    cases: c1,
                },
                Expr::Switch {
                    expr: e2,
                    cases: c2,
                },
            ) => e1 == e2 && c1 == c2,
            (
                Expr::Match {
                    expr: e1,
                    branches: b1,
                },
                Expr::Match {
                    expr: e2,
                    branches: b2,
                },
            ) => e1 == e2 && b1 == b2,
            (
                Expr::For {
                    init: i1,
                    condition: c1,
                    step: s1,
                    body: b1,
                },
                Expr::For {
                    init: i2,
                    condition: c2,
                    step: s2,
                    body: b2,
                },
            ) => i1 == i2 && c1 == c2 && s1 == s2 && b1 == b2,
            (
                Expr::While {
                    condition: c1,
                    body: b1,
                },
                Expr::While {
                    condition: c2,
                    body: b2,
                },
            ) => c1 == c2 && b1 == b2,
            (
                Expr::Do {
                    body: b1,
                    iterator: i1,
                },
                Expr::Do {
                    body: b2,
                    iterator: i2,
                },
            ) => b1 == b2 && i1 == i2,
            (
                Expr::FuncDef {
                    name: n1,
                    params: p1,
                    body: b1,
                    delayed: d1,
                },
                Expr::FuncDef {
                    name: n2,
                    params: p2,
                    body: b2,
                    delayed: d2,
                },
            ) => n1 == n2 && p1 == p2 && b1 == b2 && d1 == d2,
            (Expr::Assign { lhs: l1, rhs: r1 }, Expr::Assign { lhs: l2, rhs: r2 }) => {
                l1 == l2 && r1 == r2
            }
            (
                Expr::DestructAssign {
                    patterns: p1,
                    rhs: r1,
                },
                Expr::DestructAssign {
                    patterns: p2,
                    rhs: r2,
                },
            ) => p1 == p2 && r1 == r2,
            (
                Expr::RuleDef {
                    name: n1,
                    rules: r1,
                },
                Expr::RuleDef {
                    name: n2,
                    rules: r2,
                },
            ) => n1 == n2 && r1 == r2,
            (
                Expr::ClassDef {
                    name: n1,
                    parent: p1,
                    mixins: m1,
                    members: me1,
                },
                Expr::ClassDef {
                    name: n2,
                    parent: p2,
                    mixins: m2,
                    members: me2,
                },
            ) => n1 == n2 && p1 == p2 && m1 == m2 && me1 == me2,
            (
                Expr::ModuleDef {
                    name: n1,
                    exports: e1,
                    body: b1,
                },
                Expr::ModuleDef {
                    name: n2,
                    exports: e2,
                    body: b2,
                },
            ) => n1 == n2 && e1 == e2 && b1 == b2,
            (
                Expr::Import {
                    module: m1,
                    selective: s1,
                    alias: a1,
                },
                Expr::Import {
                    module: m2,
                    selective: s2,
                    alias: a2,
                },
            ) => m1 == m2 && s1 == s2 && a1 == a2,
            (Expr::Export(a), Expr::Export(b)) => a == b,
            (Expr::Sequence(a), Expr::Sequence(b)) => a == b,
            (Expr::Hold(a), Expr::Hold(b)) => a == b,
            (Expr::HoldComplete(a), Expr::HoldComplete(b)) => a == b,
            (Expr::ReleaseHold(a), Expr::ReleaseHold(b)) => a == b,
            (Expr::Information(a), Expr::Information(b)) => a == b,
            _ => false,
        }
    }
}

impl PartialEq for MatchBranch {
    fn eq(&self, other: &Self) -> bool {
        self.pattern == other.pattern && self.result == other.result
    }
}

impl PartialEq for IteratorSpec {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                IteratorSpec::Range {
                    var: v1,
                    min: m1,
                    max: x1,
                },
                IteratorSpec::Range {
                    var: v2,
                    min: m2,
                    max: x2,
                },
            ) => v1 == v2 && m1 == m2 && x1 == x2,
            (
                IteratorSpec::List { var: v1, list: l1 },
                IteratorSpec::List { var: v2, list: l2 },
            ) => v1 == v2 && l1 == l2,
            _ => false,
        }
    }
}

impl PartialEq for MemberDef {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (
                MemberDef::Field {
                    name: n1,
                    type_hint: t1,
                    default: d1,
                },
                MemberDef::Field {
                    name: n2,
                    type_hint: t2,
                    default: d2,
                },
            ) => n1 == n2 && t1 == t2 && d1 == d2,
            (
                MemberDef::Method {
                    name: n1,
                    params: p1,
                    return_type: r1,
                    body: b1,
                },
                MemberDef::Method {
                    name: n2,
                    params: p2,
                    return_type: r2,
                    body: b2,
                },
            ) => n1 == n2 && p1 == p2 && r1 == r2 && b1 == b2,
            (
                MemberDef::Constructor {
                    params: p1,
                    body: b1,
                },
                MemberDef::Constructor {
                    params: p2,
                    body: b2,
                },
            ) => p1 == p2 && b1 == b2,
            (
                MemberDef::Transform {
                    name: n1,
                    rules: r1,
                },
                MemberDef::Transform {
                    name: n2,
                    rules: r2,
                },
            ) => n1 == n2 && r1 == r2,
            _ => false,
        }
    }
}

impl PartialEq for MethodBody {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (MethodBody::Expr(a), MethodBody::Expr(b)) => a == b,
            (MethodBody::Block(a), MethodBody::Block(b)) => a == b,
            _ => false,
        }
    }
}

// ── Display implementations ──

impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Integer(n) => write!(f, "{}", n),
            Expr::Real(r) => {
                let s = format!("{}", r);
                if s.contains('.') {
                    let trimmed = s.trim_end_matches('0');
                    if trimmed.ends_with('.') {
                        write!(f, "{}0", trimmed)
                    } else {
                        write!(f, "{}", trimmed)
                    }
                } else {
                    write!(f, "{}", s)
                }
            }
            Expr::Complex { re, im } => {
                if *re == 0.0 {
                    write!(f, "{}I", im)
                } else if *im >= 0.0 {
                    write!(f, "{}+{}I", re, im)
                } else {
                    write!(f, "{}{}I", re, im)
                }
            }
            Expr::Str(s) => write!(f, "\"{}\"", s),
            Expr::Bool(b) => write!(f, "{}", b),
            Expr::Symbol(s) => write!(f, "{}", s),
            Expr::Null => write!(f, "Null"),
            Expr::Call { head, args } => {
                write!(f, "{}[", head)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, "]")
            }
            Expr::List(items) => {
                write!(f, "{{")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "}}")
            }
            Expr::Assoc(entries) => {
                write!(f, "<|")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "\"{}\" -> {}", k, v)?;
                }
                write!(f, "|>")
            }
            Expr::Rule { lhs, rhs } => write!(f, "{} -> {}", lhs, rhs),
            Expr::RuleDelayed { lhs, rhs } => write!(f, "{} :> {}", lhs, rhs),
            Expr::Slot(None) => write!(f, "#"),
            Expr::Slot(Some(n)) => write!(f, "#{}", n),
            Expr::Function { params, body } => {
                write!(f, "Function[")?;
                if params.len() == 1 {
                    write!(f, "{}", params[0])?;
                } else {
                    write!(f, "{{")?;
                    for (i, p) in params.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}", p)?;
                    }
                    write!(f, "}}")?;
                }
                write!(f, ", {}]", body)
            }
            Expr::Pure { body } => write!(f, "{} &", body),
            Expr::Blank { type_constraint } => match type_constraint {
                Some(tc) => write!(f, "_{}", tc),
                None => write!(f, "_"),
            },
            Expr::NamedBlank {
                name,
                type_constraint,
            } => match type_constraint {
                Some(tc) => write!(f, "{}_{}", name, tc),
                None => write!(f, "{}_", name),
            },
            Expr::BlankSequence {
                name,
                type_constraint,
            } => {
                if let Some(n) = name {
                    write!(f, "{}__", n)?;
                } else {
                    write!(f, "__")?;
                }
                if let Some(tc) = type_constraint {
                    write!(f, "{}", tc)?;
                }
                Ok(())
            }
            Expr::BlankNullSequence {
                name,
                type_constraint,
            } => {
                if let Some(n) = name {
                    write!(f, "{}___", n)?;
                } else {
                    write!(f, "___")?;
                }
                if let Some(tc) = type_constraint {
                    write!(f, "{}", tc)?;
                }
                Ok(())
            }
            Expr::PatternGuard { pattern, condition } => {
                write!(f, "{} /; {}", pattern, condition)
            }
            Expr::OptionalBlank {
                type_constraint,
                default_value,
            } => {
                match type_constraint {
                    Some(tc) => write!(f, "_{}.", tc)?,
                    None => write!(f, "_.")?,
                }
                if let Some(dv) = default_value {
                    write!(f, ":{}", dv)?;
                }
                Ok(())
            }
            Expr::OptionalNamedBlank {
                name,
                type_constraint,
                default_value,
            } => {
                match type_constraint {
                    Some(tc) => write!(f, "{}_{}.", name, tc)?,
                    None => write!(f, "{}_.", name)?,
                }
                if let Some(dv) = default_value {
                    write!(f, ":{}", dv)?;
                }
                Ok(())
            }
            Expr::ReplaceAll { expr, rules } => write!(f, "{} /. {}", expr, rules),
            Expr::ReplaceRepeated { expr, rules } => write!(f, "{} //. {}", expr, rules),
            Expr::Map { func, list } => write!(f, "{} /@ {}", func, list),
            Expr::Apply { func, expr } => write!(f, "{} @@ {}", func, expr),
            Expr::Pipe { expr, func } => write!(f, "{} // {}", expr, func),
            Expr::Prefix { func, arg } => write!(f, "{} @ {}", func, arg),
            Expr::If {
                condition,
                then_branch,
                else_branch,
            } => {
                write!(f, "If[{}, {}", condition, then_branch)?;
                if let Some(else_b) = else_branch {
                    write!(f, ", {}", else_b)?;
                }
                write!(f, "]")
            }
            Expr::Which { pairs } => {
                write!(f, "Which[")?;
                for (i, (cond, val)) in pairs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}, {}", cond, val)?;
                }
                write!(f, "]")
            }
            Expr::Switch { expr, cases } => {
                write!(f, "Switch[{}", expr)?;
                for (pat, val) in cases {
                    write!(f, ", {}, {}", pat, val)?;
                }
                write!(f, "]")
            }
            Expr::Match { expr, branches } => {
                writeln!(f, "match {} {{", expr)?;
                for b in branches {
                    writeln!(f, "    {} => {}", b.pattern, b.result)?;
                }
                write!(f, "}}")
            }
            Expr::FuncDef {
                name,
                params,
                body,
                delayed,
            } => {
                let op = if *delayed { ":=" } else { "=" };
                write!(f, "{}[", name)?;
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, "] {} {}", op, body)
            }
            Expr::Assign { lhs, rhs } => write!(f, "{} = {}", lhs, rhs),
            Expr::DestructAssign { patterns, rhs } => {
                write!(f, "{{")?;
                for (i, p) in patterns.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", p)?;
                }
                write!(f, "}} = {}", rhs)
            }
            Expr::PostIncrement { expr } => write!(f, "{}++", expr),
            Expr::PostDecrement { expr } => write!(f, "{}--", expr),
            Expr::Unset { expr } => write!(f, "{} =.", expr),
            Expr::RuleDef { name, rules } => {
                writeln!(f, "rule {} = {{", name)?;
                for (lhs, rhs) in rules {
                    writeln!(f, "    {} -> {}", lhs, rhs)?;
                }
                write!(f, "}}")
            }
            Expr::Sequence(exprs) => {
                for (i, e) in exprs.iter().enumerate() {
                    if i > 0 {
                        write!(f, "; ")?;
                    }
                    write!(f, "{}", e)?;
                }
                Ok(())
            }
            Expr::Hold(e) => write!(f, "Hold[{}]", e),
            Expr::HoldComplete(e) => write!(f, "HoldComplete[{}]", e),
            Expr::ReleaseHold(e) => write!(f, "ReleaseHold[{}]", e),
            Expr::Information(e) => write!(f, "Information[{}]", e),
            Expr::ClassDef {
                name,
                parent,
                mixins,
                members,
            } => {
                write!(f, "class {}", name)?;
                if let Some(p) = parent {
                    write!(f, " extends {}", p)?;
                }
                if !mixins.is_empty() {
                    write!(f, " with {}", mixins.join(", "))?;
                }
                writeln!(f, " {{")?;
                for m in members {
                    writeln!(f, "    {:?}", m)?;
                }
                write!(f, "}}")
            }
            Expr::ModuleDef {
                name,
                exports,
                body,
            } => {
                writeln!(f, "module {} {{", name)?;
                writeln!(f, "    export {}", exports.join(", "))?;
                for stmt in body {
                    writeln!(f, "    {};", stmt)?;
                }
                write!(f, "}}")
            }
            Expr::Import {
                module,
                selective,
                alias,
            } => {
                write!(f, "import {}", module.join("."))?;
                if let Some(sel) = selective {
                    write!(f, ".{{{}}}", sel.join(", "))?;
                }
                if let Some(a) = alias {
                    write!(f, " as {}", a)?;
                }
                Ok(())
            }
            Expr::Export(names) => write!(f, "export {}", names.join(", ")),
            Expr::For {
                init,
                condition,
                step,
                body,
            } => {
                write!(f, "For[{}, {}, {}, {}]", init, condition, step, body)
            }
            Expr::While { condition, body } => {
                write!(f, "While[{}, {}]", condition, body)
            }
            Expr::Do { body, iterator } => {
                write!(f, "Do[{}, {:?}]", body, iterator)
            }
        }
    }
}
