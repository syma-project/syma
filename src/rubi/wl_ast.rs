/// Wolfram Language AST subset for Rubi rule parsing.
///
/// Defines the subset of Wolfram Language needed to represent
/// Rubi integration rules: Int[pattern_, x_Symbol] := result /; condition
use std::fmt;

/// A Wolfram Language expression (subset used in Rubi rules).
#[derive(Debug, Clone, PartialEq)]
pub enum WLExpr {
    /// Symbol name, e.g. `x`, `Sin`, `Plus`, `Integrate`
    Symbol(String),
    /// Integer literal
    Integer(i64),
    /// Real number literal
    Real(f64),
    /// String literal
    Str(String),
    /// List: `{a, b, c}`
    List(Vec<WLExpr>),
    /// Function call: `head[arg1, arg2, ...]`
    Call {
        head: Box<WLExpr>,
        args: Vec<WLExpr>,
    },
    /// Binary operation: `a + b`, `a * b`, `a ^ b`, `a / b`
    BinaryOp {
        op: BinOp,
        lhs: Box<WLExpr>,
        rhs: Box<WLExpr>,
    },
    /// Unary operation: `-a`
    UnaryOp { op: UnaryOp, expr: Box<WLExpr> },
    /// Condition: `expr /; cond`
    Condition {
        expr: Box<WLExpr>,
        cond: Box<WLExpr>,
    },
    /// With block: `With[{x = val}, expr]`
    With {
        bindings: Vec<(String, WLExpr)>,
        body: Box<WLExpr>,
    },
    /// Wildcard pattern: `_`
    Blank,
    /// Wildcard pattern with type: `_Integer`, `_Symbol`, `_Real`
    BlankType(String),
    /// Named pattern: `x_`
    NamedBlank(String),
    /// Typed pattern: `x_Integer`, `x_Symbol`
    NamedBlankType(String, String),
    /// Optional pattern: `a_.`  (optional with default value)
    Optional(String),
    /// Optional pattern with default value: `a_:default`
    OptionalDefault(String, Box<WLExpr>),
    /// Pattern sequence: `x__` (one or more), `x___` (zero or more)
    PatternSequence(String, bool), // bool = True for ___
    /// Rule substitution: `x -> val` or `x :> val`
    Rule {
        lhs: Box<WLExpr>,
        rhs: Box<WLExpr>,
        delayed: bool,
    },
    /// Slot: `#`, `#1`, `#2`, etc.
    Slot(Option<i64>),
    /// Hold: HoldForm, Defer, etc.
    Hold(Box<WLExpr>),
}

/// Binary operator types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinOp {
    Plus,
    Minus,
    Times,
    Divide,
    Power,
    Equal,
    Unequal,
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    And,
    Or,
    SameQ,
    UnsameQ,
    Rule,
    RuleDelayed,
    ReplaceAll,
    ReplaceRepeated,
    Map,
    Apply,
}

/// Unary operator types.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
    Neg,
    Not,
}

impl fmt::Display for WLExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WLExpr::Symbol(s) => write!(f, "{}", s),
            WLExpr::Integer(n) => write!(f, "{}", n),
            WLExpr::Real(n) => write!(f, "{}", n),
            WLExpr::Str(s) => write!(f, "\"{}\"", s),
            WLExpr::List(items) => {
                write!(f, "{{")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", item)?;
                }
                write!(f, "}}")
            }
            WLExpr::Call { head, args } => {
                write!(f, "{}[", head)?;
                for (i, arg) in args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", arg)?;
                }
                write!(f, "]")
            }
            WLExpr::BinaryOp { op, lhs, rhs } => {
                let op_str = match op {
                    BinOp::Plus => " + ",
                    BinOp::Minus => " - ",
                    BinOp::Times => " * ",
                    BinOp::Divide => " / ",
                    BinOp::Power => " ^ ",
                    BinOp::Equal => " == ",
                    BinOp::Unequal => " != ",
                    BinOp::Less => " < ",
                    BinOp::Greater => " > ",
                    BinOp::LessEqual => " <= ",
                    BinOp::GreaterEqual => " >= ",
                    BinOp::And => " && ",
                    BinOp::Or => " || ",
                    BinOp::SameQ => " === ",
                    BinOp::UnsameQ => " =!= ",
                    BinOp::Rule => " -> ",
                    BinOp::RuleDelayed => " :> ",
                    BinOp::ReplaceAll => " /. ",
                    BinOp::ReplaceRepeated => " //. ",
                    BinOp::Map => " /@ ",
                    BinOp::Apply => " @@ ",
                };
                write!(f, "({}{}{})", lhs, op_str, rhs)
            }
            WLExpr::UnaryOp { op, expr } => match op {
                UnaryOp::Neg => write!(f, "(-{})", expr),
                UnaryOp::Not => write!(f, "(!{})", expr),
            },
            WLExpr::Condition { expr, cond } => write!(f, "({} /; {})", expr, cond),
            WLExpr::With { bindings, body } => {
                write!(f, "With[{{")?;
                for (i, (name, val)) in bindings.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{} = {}", name, val)?;
                }
                write!(f, "}}, {}]", body)
            }
            WLExpr::Blank => write!(f, "_"),
            WLExpr::BlankType(t) => write!(f, "_{}", t),
            WLExpr::NamedBlank(n) => write!(f, "{}_", n),
            WLExpr::NamedBlankType(n, t) => write!(f, "{}_{}", n, t),
            WLExpr::Optional(n) => write!(f, "{}.", n),
            WLExpr::OptionalDefault(n, d) => write!(f, "{}_:{}", n, d),
            WLExpr::PatternSequence(n, triple) => {
                if *triple {
                    write!(f, "{}___", n)
                } else {
                    write!(f, "{}__", n)
                }
            }
            WLExpr::Rule { lhs, rhs, delayed } => {
                if *delayed {
                    write!(f, "{} :> {}", lhs, rhs)
                } else {
                    write!(f, "{} -> {}", lhs, rhs)
                }
            }
            WLExpr::Slot(None) => write!(f, "#"),
            WLExpr::Slot(Some(n)) => write!(f, "#{}", n),
            WLExpr::Hold(e) => write!(f, "Hold[{}]", e),
        }
    }
}

/// A parsed Rubi integration rule.
#[derive(Debug, Clone)]
pub struct IntRule {
    /// Index in loading order
    pub index: usize,
    /// Source file name (for debugging)
    pub source: String,
    /// The integrand pattern (first argument to Int)
    pub pattern: WLExpr,
    /// The result expression (antiderivative or reduction)
    pub result: WLExpr,
    /// Optional condition that must be satisfied
    pub condition: Option<WLExpr>,
}

/// A parsed rule file containing multiple Int rules.
#[derive(Debug, Clone)]
pub struct RuleFile {
    pub name: String,
    pub rules: Vec<IntRule>,
}
