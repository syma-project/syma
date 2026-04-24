/// Wolfram Language parser for Rubi rule files.
///
/// Parses the subset of Wolfram Language used in Rubi `.m` rule files:
/// - Expression syntax: `Int[pattern_, x_Symbol] := result /; condition`
/// - Arithmetic operators: `+`, `-`, `*`, `/`, `^`
/// - Lists: `{a, b, c}`
/// - Patterns: `_`, `x_`, `x_Symbol`, `x_.`
/// - Conditions: `/; cond1 && cond2`
/// - Rule syntax: `->`, `:>`
/// - `With[{x = val}, expr]`
/// - Comments: `(* ... *)`
use crate::rubi::wl_ast::{BinOp, IntRule, RuleFile, UnaryOp, WLExpr};

/// A simple tokenizer for the Wolfram Language subset.
#[derive(Debug, Clone)]
struct Tokenizer<'a> {
    input: &'a str,
    pos: usize,
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Ident(String),
    Integer(i64),
    Real(f64),
    String(String),
    LParen,
    RParen,
    LBracket,
    RBracket,
    LBrace,
    RBrace,
    Comma,
    Semi,
    Colon,
    ColonColon,
    Plus,
    Minus,
    Star,
    Slash,
    Caret,
    Equal,
    ColonEqual,    // :=
    SemicolonCond, // /;
    Underscore,
    Dot,
    Pipe,        // /.
    PipePipe,    // //.
    At,          // @
    AtAt,        // @@
    SlashAt,     // /@
    Mapping,     // // or |>
    Rule,        // ->
    RuleDelayed, // :>
    Slot,
    SlotNum(i64),
    Not, // !
    And, // &&
    Or,  // ||
    Less,
    Greater,
    LessEqual,
    GreaterEqual,
    EqualEqual, // ==
    Unequal,    // !=
    SameQ,      // ===
    UnsameQ,    // =!=
    NewLine,
    EOF,
}

impl<'a> Tokenizer<'a> {
    fn new(input: &'a str) -> Self {
        Tokenizer { input, pos: 0 }
    }

    fn remaining(&self) -> &'a str {
        &self.input[self.pos..]
    }

    fn peek_char(&self) -> Option<char> {
        self.remaining().chars().next()
    }

    fn advance(&mut self) {
        if let Some(c) = self.peek_char() {
            self.pos += c.len_utf8();
        }
    }

    fn skip_whitespace_and_comments(&mut self) {
        loop {
            // Skip whitespace
            while let Some(c) = self.peek_char() {
                if c.is_whitespace() && c != '\n' {
                    self.advance();
                } else {
                    break;
                }
            }

            // Skip comments (* ... *)
            if self.remaining().starts_with("(*") {
                self.pos += 2;
                let mut depth = 1;
                while depth > 0 && self.pos < self.input.len() {
                    if self.remaining().starts_with("(*") {
                        depth += 1;
                        self.pos += 2;
                    } else if self.remaining().starts_with("*)") {
                        depth -= 1;
                        self.pos += 2;
                    } else {
                        self.advance();
                    }
                }
            } else {
                break;
            }
        }
    }

    fn next_token(&mut self) -> Token {
        self.skip_whitespace_and_comments();

        if self.pos >= self.input.len() {
            return Token::EOF;
        }

        let rest = self.remaining();
        let c = rest.chars().next().unwrap();

        // Handle multi-character tokens
        // Newline
        if c == '\n' {
            self.advance();
            // Skip consecutive newlines
            self.skip_whitespace_and_comments();
            if self.peek_char() == Some('\n') {
                self.advance();
                return Token::NewLine;
            }
            return self.next_token(); // Skip single newlines
        }

        // Comment (* ... *)
        if rest.starts_with("(*") {
            self.advance();
            self.advance();
            let mut depth = 1;
            while depth > 0 && self.pos < self.input.len() {
                if self.remaining().starts_with("(*") {
                    depth += 1;
                    self.pos += 2;
                } else if self.remaining().starts_with("*)") {
                    depth -= 1;
                    self.pos += 2;
                } else {
                    self.advance();
                }
            }
            return self.next_token();
        }

        // Two and three character operators
        if rest.starts_with("//.") {
            self.pos += 3;
            return Token::PipePipe;
        }
        if rest.starts_with("/;") {
            self.pos += 2;
            return Token::SemicolonCond;
        }
        if rest.starts_with("/@") {
            self.pos += 2;
            return Token::SlashAt;
        }
        if rest.starts_with("//") {
            self.pos += 2;
            return Token::Mapping;
        }
        if rest.starts_with(":>") {
            self.pos += 2;
            return Token::RuleDelayed;
        }
        if rest.starts_with(":=") {
            self.pos += 2;
            return Token::ColonEqual;
        }
        if rest.starts_with("->") {
            self.pos += 2;
            return Token::Rule;
        }
        if rest.starts_with("@@") {
            self.pos += 2;
            return Token::AtAt;
        }
        if rest.starts_with("/.") {
            self.pos += 2;
            return Token::Pipe;
        }
        if rest.starts_with("&&") {
            self.pos += 2;
            return Token::And;
        }
        if rest.starts_with("||") {
            self.pos += 2;
            return Token::Or;
        }
        if rest.starts_with("==") {
            self.pos += 2;
            return Token::EqualEqual;
        }
        if rest.starts_with("!=") {
            self.pos += 2;
            return Token::Unequal;
        }
        if rest.starts_with("===") {
            self.pos += 3;
            return Token::SameQ;
        }
        if rest.starts_with("=!=") {
            self.pos += 3;
            return Token::UnsameQ;
        }
        if rest.starts_with("<=") {
            self.pos += 2;
            return Token::LessEqual;
        }
        if rest.starts_with(">=") {
            self.pos += 2;
            return Token::GreaterEqual;
        }
        if rest.starts_with("::") {
            self.pos += 2;
            return Token::ColonColon;
        }

        // Single character tokens
        match c {
            '(' => {
                self.advance();
                Token::LParen
            }
            ')' => {
                self.advance();
                Token::RParen
            }
            '[' => {
                self.advance();
                Token::LBracket
            }
            ']' => {
                self.advance();
                Token::RBracket
            }
            '{' => {
                self.advance();
                Token::LBrace
            }
            '}' => {
                self.advance();
                Token::RBrace
            }
            ',' => {
                self.advance();
                Token::Comma
            }
            ';' => {
                self.advance();
                Token::Semi
            }
            '+' => {
                self.advance();
                Token::Plus
            }
            '-' => {
                self.advance();
                Token::Minus
            }
            '*' => {
                self.advance();
                Token::Star
            }
            '/' => {
                self.advance();
                Token::Slash
            }
            '^' => {
                self.advance();
                Token::Caret
            }
            '=' => {
                self.advance();
                Token::Equal
            }
            ':' => {
                self.advance();
                Token::Colon
            }
            '!' if rest.len() > 1 && rest.as_bytes()[1] != b'=' => {
                self.advance();
                Token::Not
            }
            '<' => {
                self.advance();
                Token::Less
            }
            '>' => {
                self.advance();
                Token::Greater
            }
            '.' => {
                self.advance();
                Token::Dot
            }
            '_' => {
                self.advance();
                Token::Underscore
            }
            '@' => {
                self.advance();
                Token::At
            }
            '|' if rest.len() > 1 => {
                // |> operator for mapping
                if rest.as_bytes()[1] == b'>' {
                    self.pos += 2;
                    Token::Mapping
                } else {
                    self.advance();
                    Token::Mapping
                }
            }
            '#' => {
                self.advance();
                // Check for slot number
                let start = self.pos;
                while let Some(d) = self.peek_char() {
                    if d.is_ascii_digit() {
                        self.advance();
                    } else {
                        break;
                    }
                }
                if self.pos > start {
                    let num: i64 = self.input[start..self.pos].parse().unwrap_or(1);
                    Token::SlotNum(num)
                } else {
                    Token::Slot
                }
            }
            '"' => self.read_string(),
            _ if c.is_ascii_digit() => self.read_number(),
            _ if c.is_alphabetic() || c == '$' => self.read_identifier(),
            _ => {
                self.advance();
                Token::EOF
            }
        }
    }

    fn read_string(&mut self) -> Token {
        self.advance(); // skip opening "
        let start = self.pos;
        while self.pos < self.input.len() {
            if self.remaining().starts_with('"') {
                let s = self.input[start..self.pos].to_string();
                self.advance(); // skip closing "
                return Token::String(s);
            }
            if self.remaining().starts_with('\\') {
                self.advance(); // skip backslash
                if self.peek_char().is_some() {
                    self.advance();
                }
            } else {
                self.advance();
            }
        }
        Token::String(self.input[start..].to_string())
    }

    fn read_number(&mut self) -> Token {
        let start = self.pos;
        let mut is_real = false;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_digit() {
                self.advance();
            } else if c == '.' {
                is_real = true;
                self.advance();
            } else {
                break;
            }
        }
        let num_str = &self.input[start..self.pos];
        if is_real {
            Token::Real(num_str.parse().unwrap_or(0.0))
        } else {
            Token::Integer(num_str.parse().unwrap_or(0))
        }
    }

    fn read_identifier(&mut self) -> Token {
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c.is_alphanumeric() || c == '$' {
                self.advance();
            } else {
                break;
            }
        }
        let ident = self.input[start..self.pos].to_string();
        Token::Ident(ident)
    }
}

/// The Rubi rule file parser.
pub struct WLParser<'a> {
    tokenizer: Tokenizer<'a>,
    current_token: Token,
    peek_token: Token,
}

impl<'a> WLParser<'a> {
    pub fn new(input: &'a str) -> Self {
        let mut parser = WLParser {
            tokenizer: Tokenizer::new(input),
            current_token: Token::EOF,
            peek_token: Token::EOF,
        };
        parser.advance();
        parser.advance();
        parser
    }

    fn advance(&mut self) {
        self.current_token = self.peek_token.clone();
        self.peek_token = self.tokenizer.next_token();
    }

    fn expect(&mut self, expected: &Token) -> Result<(), String> {
        if self.current_token == *expected {
            self.advance();
            Ok(())
        } else {
            Err(format!(
                "Expected {:?}, got {:?}",
                expected, self.current_token
            ))
        }
    }

    /// Parse all Int rules from the file.
    pub fn parse_rules(&mut self) -> Result<RuleFile, String> {
        let mut rules = Vec::new();
        let name = String::new();

        loop {
            match &self.current_token {
                Token::EOF => break,
                Token::Ident(_) => {
                    let rule = self.parse_rule()?;
                    if let Some(r) = rule {
                        rules.push(r);
                    }
                }
                _ => {
                    self.advance();
                }
            }
        }

        Ok(RuleFile { name, rules })
    }

    /// Parse a single rule: Int[pattern_, x_Symbol] := result /; condition
    fn parse_rule(&mut self) -> Result<Option<IntRule>, String> {
        // Expect "Int"
        if !matches!(&self.current_token, Token::Ident(s) if s == "Int") {
            self.advance();
            return Ok(None);
        }
        self.advance();

        // Expect "["
        if self.current_token != Token::LBracket {
            return Ok(None);
        }
        self.advance();

        // Parse the integrand pattern
        let pattern = self.parse_expression(0)?;

        // Expect ","
        if self.current_token != Token::Comma {
            return Ok(None);
        }
        self.advance();

        // Parse x_Symbol (second argument to Int)
        let _x_pattern = self.parse_expression(0)?;

        // Expect "]"
        if self.current_token != Token::RBracket {
            // Skip to :=
            while self.current_token != Token::ColonEqual && self.current_token != Token::EOF {
                self.advance();
            }
            if self.current_token == Token::EOF {
                return Ok(None);
            }
        } else {
            self.advance();
        }

        // Expect ":="
        if self.current_token != Token::ColonEqual {
            return Ok(None);
        }
        self.advance();

        // Parse the result expression
        let result = self.parse_expression(0)?;

        // Parse optional condition "/;"
        let condition = if self.current_token == Token::SemicolonCond {
            self.advance();
            Some(self.parse_expression(0)?)
        } else {
            None
        };

        Ok(Some(IntRule {
            index: 0,
            source: String::new(),
            pattern,
            result,
            condition,
        }))
    }

    /// Parse an expression with operator precedence.
    fn parse_expression(&mut self, min_prec: u32) -> Result<WLExpr, String> {
        let mut lhs = self.parse_primary()?;

        loop {
            let (prec, _is_left) = self.token_precedence();
            if prec < min_prec {
                break;
            }

            match &self.current_token {
                Token::Plus => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::Plus,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::Minus => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::Minus,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::Star => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::Times,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::Slash => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::Divide,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::Caret => {
                    self.advance();
                    let rhs = self.parse_expression(prec)?; // right-associative
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::Power,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::EqualEqual => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::Equal,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::Unequal => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::Unequal,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::Less => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::Less,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::Greater => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::Greater,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::LessEqual => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::LessEqual,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::GreaterEqual => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::GreaterEqual,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::And => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::And,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::Or => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::Or,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::Rule => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::Rule {
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                        delayed: false,
                    };
                }
                Token::RuleDelayed => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::Rule {
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                        delayed: true,
                    };
                }
                Token::Pipe => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::ReplaceAll,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                Token::PipePipe => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::BinaryOp {
                        op: BinOp::ReplaceRepeated,
                        lhs: Box::new(lhs),
                        rhs: Box::new(rhs),
                    };
                }
                // Function application: f @ x, f // x
                Token::At => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::Call {
                        head: Box::new(lhs),
                        args: vec![rhs],
                    };
                }
                Token::Mapping => {
                    self.advance();
                    let rhs = self.parse_expression(prec + 1)?;
                    lhs = WLExpr::Call {
                        head: Box::new(rhs),
                        args: vec![lhs],
                    };
                }
                _ => break,
            }
        }

        Ok(lhs)
    }

    /// Token precedence values (higher = tighter binding)
    fn token_precedence(&self) -> (u32, bool) {
        match &self.current_token {
            Token::Or => (1, true),
            Token::And => (2, true),
            Token::EqualEqual
            | Token::Unequal
            | Token::Less
            | Token::Greater
            | Token::LessEqual
            | Token::GreaterEqual => (3, true),
            Token::Plus | Token::Minus => (4, true),
            Token::Star | Token::Slash => (5, true),
            Token::Caret => (6, false), // right-associative
            Token::At | Token::Mapping => (7, true),
            Token::Pipe | Token::PipePipe => (0, true),
            Token::Rule | Token::RuleDelayed => (0, true),
            _ => (0, true),
        }
    }

    /// Parse a primary expression (atom, call, list, pattern, etc.)
    fn parse_primary(&mut self) -> Result<WLExpr, String> {
        match &self.current_token.clone() {
            Token::Integer(n) => {
                self.advance();
                // Check for pattern markers after the integer
                // Like in `1/x_`: after `1`, `/` will be handled as operator
                Ok(WLExpr::Integer(*n))
            }
            Token::Real(n) => {
                self.advance();
                Ok(WLExpr::Real(*n))
            }
            Token::String(s) => {
                let s = s.clone();
                self.advance();
                Ok(WLExpr::Str(s))
            }
            Token::LParen => {
                self.advance();
                let expr = self.parse_expression(0)?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }
            Token::LBrace => {
                // List: {a, b, c}
                self.advance();
                let mut items = Vec::new();
                if self.current_token != Token::RBrace {
                    items.push(self.parse_expression(0)?);
                    while self.current_token == Token::Comma {
                        self.advance();
                        if self.current_token == Token::RBrace {
                            break;
                        }
                        items.push(self.parse_expression(0)?);
                    }
                }
                self.expect(&Token::RBrace)?;
                Ok(WLExpr::List(items))
            }
            Token::Underscore => {
                // Blank: _ or _Integer
                self.advance();
                if let Token::Ident(type_name) = &self.current_token.clone() {
                    self.advance();
                    Ok(WLExpr::BlankType(type_name.clone()))
                } else {
                    Ok(WLExpr::Blank)
                }
            }
            Token::Slot => {
                self.advance();
                Ok(WLExpr::Slot(None))
            }
            Token::SlotNum(n) => {
                let n = *n;
                self.advance();
                Ok(WLExpr::Slot(Some(n)))
            }
            Token::Not => {
                self.advance();
                let expr = self.parse_expression(4)?; // tight binding for unary !
                Ok(WLExpr::UnaryOp {
                    op: UnaryOp::Not,
                    expr: Box::new(expr),
                })
            }
            Token::Minus => {
                // Could be unary minus
                self.advance();
                let expr = self.parse_expression(5)?;
                Ok(WLExpr::UnaryOp {
                    op: UnaryOp::Neg,
                    expr: Box::new(expr),
                })
            }
            Token::Ident(name) => {
                let name = name.clone();
                self.advance();

                // Check for pattern suffix: x_, x_Integer, x_., x_.type
                if self.current_token == Token::Underscore {
                    self.advance();
                    if self.current_token == Token::Dot {
                        // Optional named blank: x_.
                        self.advance();
                        // Check for type after dot: x_.type (rare)
                        if let Token::Ident(type_name) = &self.current_token.clone() {
                            self.advance();
                            Ok(WLExpr::NamedBlankType(name, type_name.clone()))
                        } else {
                            // x_. → Optional[name]
                            Ok(WLExpr::Optional(name))
                        }
                    } else if let Token::Ident(type_name) = &self.current_token.clone() {
                        // Typed named blank: x_Integer
                        self.advance();
                        Ok(WLExpr::NamedBlankType(name, type_name.clone()))
                    } else {
                        // Plain named blank: x_
                        Ok(WLExpr::NamedBlank(name))
                    }
                } else if self.current_token == Token::Dot {
                    // Optional pattern: x_. or x
                    self.advance();
                    // Check for _ meaning `x. _` (x_.) — named blank with optional
                    if self.current_token == Token::Underscore {
                        self.advance();
                        if let Token::Ident(type_name) = &self.current_token.clone() {
                            self.advance();
                            Ok(WLExpr::NamedBlankType(name, type_name.clone()))
                        } else {
                            Ok(WLExpr::NamedBlank(name))
                        }
                    } else {
                        Ok(WLExpr::Optional(name))
                    }
                } else if self.current_token == Token::ColonColon {
                    // Message name: symbol::tag — treated as symbol
                    self.advance();
                    if let Token::Ident(tag) = &self.current_token.clone() {
                        self.advance();
                        // Use the qualified name
                        let qualified = format!("{}::{}", name, tag);
                        Ok(WLExpr::Symbol(qualified))
                    } else {
                        Ok(WLExpr::Symbol(name))
                    }
                } else if self.current_token == Token::ColonEqual {
                    // This is a definition, not an expression.
                    // Return the symbol and let the caller handle :=
                    Ok(WLExpr::Symbol(name))
                } else if self.current_token == Token::Equal {
                    // Set: expr = val — keep as symbol for now
                    self.advance();
                    let val = self.parse_expression(0)?;
                    Ok(WLExpr::Rule {
                        lhs: Box::new(WLExpr::Symbol(name)),
                        rhs: Box::new(val),
                        delayed: false,
                    })
                } else if self.current_token == Token::LBracket {
                    // Function call: name[arg1, arg2, ...]
                    self.advance();
                    let mut args = Vec::new();
                    if self.current_token != Token::RBracket {
                        args.push(self.parse_expression(0)?);
                        while self.current_token == Token::Comma {
                            self.advance();
                            if self.current_token == Token::RBracket {
                                break;
                            }
                            args.push(self.parse_expression(0)?);
                        }
                    }
                    self.expect(&Token::RBracket)?;

                    // Check for postfix patterns on the entire call
                    if self.current_token == Token::Underscore {
                        self.advance();
                        if let Token::Ident(type_name) = &self.current_token.clone() {
                            self.advance();
                            Ok(WLExpr::NamedBlankType(name, type_name.clone()))
                        } else {
                            Ok(WLExpr::NamedBlank(name))
                        }
                    } else if self.current_token == Token::Dot {
                        self.advance();
                        Ok(WLExpr::Optional(name))
                    } else {
                        Ok(WLExpr::Call {
                            head: Box::new(WLExpr::Symbol(name)),
                            args,
                        })
                    }
                } else {
                    Ok(WLExpr::Symbol(name))
                }
            }
            _ => {
                let msg = format!("Unexpected token: {:?}", self.current_token);
                self.advance();
                Err(msg)
            }
        }
    }
}

/// Convenience function to parse a single Int rule from a string.
pub fn parse_rule_string(input: &str) -> Result<Option<IntRule>, String> {
    let mut parser = WLParser::new(input);
    parser.parse_rule()
}

/// Parse all Int rules from the given input string.
pub fn parse_rules(input: &str) -> Result<RuleFile, String> {
    let mut parser = WLParser::new(input);
    parser.parse_rules()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_blank() {
        let result = parse_rule_string("Int[_, x_Symbol] := x");
        assert!(result.is_ok());
        let rule = result.unwrap().unwrap();
        assert_eq!(rule.pattern, WLExpr::Blank);
    }

    #[test]
    fn test_parse_named_blank() {
        let result = parse_rule_string("Int[x_, x_Symbol] := x");
        assert!(result.is_ok());
        let rule = result.unwrap().unwrap();
        assert_eq!(rule.pattern, WLExpr::NamedBlank("x".to_string()));
    }

    #[test]
    fn test_parse_simple_integral() {
        // Int[x_^m_., x_Symbol] := x^(m + 1)/(m + 1) /; FreeQ[m, x] && NeQ[m, -1]
        let input = "Int[x_^m_., x_Symbol] := x^(m + 1)/(m + 1) /; FreeQ[m, x] && NeQ[m, -1]";
        let result = parse_rule_string(input);
        assert!(result.is_ok(), "Parse error: {:?}", result.err());
        let rule = result.unwrap().unwrap();
        // The pattern should be a Power call
        assert!(matches!(
            rule.pattern,
            WLExpr::BinaryOp {
                op: BinOp::Power,
                ..
            }
        ));
        // Should have a condition
        assert!(rule.condition.is_some());
    }

    #[test]
    fn test_parse_1_over_x() {
        let result = parse_rule_string("Int[1/x_, x_Symbol] := Log[x]");
        assert!(result.is_ok());
        let rule = result.unwrap().unwrap();
        assert!(rule.condition.is_none());
    }

    #[test]
    fn test_parse_pattern_with_type() {
        let result = parse_rule_string("Int[x_Symbol, x_Symbol] := x");
        assert!(result.is_ok());
    }

    #[test]
    fn test_parse_list_in_condition() {
        // FreeQ[{a, b}, x]
        let input = "Int[(a_. + b_.*x_)^m_, x_Symbol] := (a + b*x)^(m + 1)/(b*(m + 1)) /; FreeQ[{a, b, m}, x] && NeQ[m, -1]";
        let result = parse_rule_string(input);
        assert!(result.is_ok(), "Parse error: {:?}", result.err());
    }

    #[test]
    fn test_parse_with_subst() {
        // Subst[Int[(a + b*x)^m, x], x, u]
        let input = "Int[(a_. + b_.*u_)^m_, x_Symbol] := 1/Coefficient[u, x, 1]*Subst[Int[(a + b*x)^m, x], x, u] /; FreeQ[{a, b, m}, x] && LinearQ[u, x] && NeQ[u, x]";
        let result = parse_rule_string(input);
        assert!(result.is_ok(), "Parse error: {:?}", result.err());
    }

    #[test]
    fn test_parse_multiple_rules() {
        let input = "
Int[1/x_, x_Symbol] := Log[x]
Int[x_^m_., x_Symbol] := x^(m + 1)/(m + 1) /; FreeQ[m, x] && NeQ[m, -1]
Int[1/(a_ + b_.*x_), x_Symbol] := Log[RemoveContent[a + b*x, x]]/b /; FreeQ[{a, b}, x]
";
        let result = parse_rules(input);
        assert!(result.is_ok());
        assert_eq!(result.unwrap().rules.len(), 3);
    }
}
