/// Lexer for Syma language.
///
/// Tokenizes source code into a stream of tokens. Handles:
/// - Wolfram-style bracket syntax
/// - Operator disambiguation (/. vs //. vs // vs /@)
/// - String literals with escape sequences
/// - Numeric literals (integer, real, complex)
/// - Symbols and keywords

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // ── Literals ──
    Integer(String),
    Real(String),
    Str(String),
    True,
    False,
    Null,

    // ── Identifiers ──
    Ident(String),

    // ── Delimiters ──
    LParen,     // (
    RParen,     // )
    LBracket,   // [
    RBracket,   // ]
    LBrace,     // {
    RBrace,     // }
    LAssoc,     // <|
    RAssoc,     // |>
    LDoubleBracket,  // [[
    RDoubleBracket,  // ]]

    // ── Operators ──
    Plus,       // +
    Minus,      // -
    Star,       // *
    Slash,      // /
    Caret,      // ^
    Dot,        // .
    Comma,      // ,
    Semicolon,  // ;
    Colon,      // :

    // ── Multi-char operators ──
    Assign,         // =
    DelayedAssign,  // :=
    Rule,           // ->
    DelayedRule,    // :>
    ReplaceAll,     // /.
    ReplaceRepeated,// /.
    MapOp,          // /@
    ApplyOp,        // @@
    At,             // @
    Pipe,           // //
    Equal,          // ==
    Unequal,        // !=
    Less,           // <
    Greater,        // >
    LessEqual,      // <=
    GreaterEqual,   // >=
    And,            // &&
    Or,             // ||
    Not,            // !
    FatArrow,       // =>
    #[allow(dead_code)]
    StringJoinOp,   // <>

    // ── Special ──
    Quote,      // ' (quote)
    Tilde,      // ~ (splice)
    Slot,       // #
    SlotN(usize), // #1, #2, ...

    // ── Keywords ──
    If,
    Which,
    Switch,
    Match,
    For,
    While,
    Do,
    Try,
    Catch,
    Finally,
    Throw,
    Function,
    Class,
    Extends,
    With,
    Method,
    Field,
    Constructor,
    Module,
    Import,
    Export,
    As,
    RuleKw,
    Hold,
    HoldComplete,
    ReleaseHold,
    #[allow(dead_code)]
    Transform,
    Mixin,

    // ── Special tokens ──
    ColonSlashSemicolon, // /;  (guard)
    AtTransform,         // @transform

    // ── End of input ──
    Eof,
}

impl fmt::Display for Token {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Token::Integer(s) => write!(f, "{}", s),
            Token::Real(s) => write!(f, "{}", s),
            Token::Str(s) => write!(f, "\"{}\"", s),
            Token::True => write!(f, "True"),
            Token::False => write!(f, "False"),
            Token::Null => write!(f, "Null"),
            Token::Ident(s) => write!(f, "{}", s),
            Token::LParen => write!(f, "("),
            Token::RParen => write!(f, ")"),
            Token::LBracket => write!(f, "["),
            Token::RBracket => write!(f, "]"),
            Token::LBrace => write!(f, "{{"),
            Token::RBrace => write!(f, "}}"),
            Token::LAssoc => write!(f, "<|"),
            Token::RAssoc => write!(f, "|>"),
            Token::LDoubleBracket => write!(f, "[["),
            Token::RDoubleBracket => write!(f, "]]"),
            Token::Plus => write!(f, "+"),
            Token::Minus => write!(f, "-"),
            Token::Star => write!(f, "*"),
            Token::Slash => write!(f, "/"),
            Token::Caret => write!(f, "^"),
            Token::Dot => write!(f, "."),
            Token::Comma => write!(f, ","),
            Token::Semicolon => write!(f, ";"),
            Token::Colon => write!(f, ":"),
            Token::Assign => write!(f, "="),
            Token::DelayedAssign => write!(f, ":="),
            Token::Rule => write!(f, "->"),
            Token::DelayedRule => write!(f, ":>"),
            Token::ReplaceAll => write!(f, "/."),
            Token::ReplaceRepeated => write!(f, "//."),
            Token::MapOp => write!(f, "/@"),
            Token::ApplyOp => write!(f, "@@"),
            Token::At => write!(f, "@"),
            Token::Pipe => write!(f, "//"),
            Token::Equal => write!(f, "=="),
            Token::Unequal => write!(f, "!="),
            Token::Less => write!(f, "<"),
            Token::Greater => write!(f, ">"),
            Token::LessEqual => write!(f, "<="),
            Token::GreaterEqual => write!(f, ">="),
            Token::And => write!(f, "&&"),
            Token::Or => write!(f, "||"),
            Token::Not => write!(f, "!"),
            Token::FatArrow => write!(f, "=>"),
            Token::StringJoinOp => write!(f, "<>"),
            Token::Quote => write!(f, "'"),
            Token::Tilde => write!(f, "~"),
            Token::Slot => write!(f, "#"),
            Token::SlotN(n) => write!(f, "#{}", n),
            Token::If => write!(f, "If"),
            Token::Which => write!(f, "Which"),
            Token::Switch => write!(f, "Switch"),
            Token::Match => write!(f, "match"),
            Token::For => write!(f, "For"),
            Token::While => write!(f, "While"),
            Token::Do => write!(f, "Do"),
            Token::Try => write!(f, "try"),
            Token::Catch => write!(f, "catch"),
            Token::Finally => write!(f, "finally"),
            Token::Throw => write!(f, "throw"),
            Token::Function => write!(f, "Function"),
            Token::Class => write!(f, "class"),
            Token::Extends => write!(f, "extends"),
            Token::With => write!(f, "with"),
            Token::Method => write!(f, "method"),
            Token::Field => write!(f, "field"),
            Token::Constructor => write!(f, "constructor"),
            Token::Module => write!(f, "module"),
            Token::Import => write!(f, "import"),
            Token::Export => write!(f, "export"),
            Token::As => write!(f, "as"),
            Token::RuleKw => write!(f, "rule"),
            Token::Hold => write!(f, "Hold"),
            Token::HoldComplete => write!(f, "HoldComplete"),
            Token::ReleaseHold => write!(f, "ReleaseHold"),
            Token::Transform => write!(f, "@transform"),
            Token::Mixin => write!(f, "mixin"),
            Token::ColonSlashSemicolon => write!(f, "/;"),
            Token::AtTransform => write!(f, "@transform"),
            Token::Eof => write!(f, "EOF"),
        }
    }
}

pub struct Lexer {
    input: Vec<char>,
    pos: usize,
    tokens: Vec<Token>,
    /// Tracks nesting depth of [[ ... ]] Part-access brackets.
    /// Only emit RDoubleBracket when depth > 0; otherwise ]] is two RBracket tokens.
    double_bracket_depth: usize,
}

#[derive(Debug)]
pub struct LexError {
    pub message: String,
    pub pos: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Lex error at position {}: {}", self.pos, self.message)
    }
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Lexer {
            input: input.chars().collect(),
            pos: 0,
            tokens: Vec::new(),
            double_bracket_depth: 0,
        }
    }

    fn peek(&self) -> Option<char> {
        self.input.get(self.pos).copied()
    }

    fn peek_ahead(&self, offset: usize) -> Option<char> {
        self.input.get(self.pos + offset).copied()
    }

    fn advance(&mut self) -> Option<char> {
        let ch = self.input.get(self.pos).copied();
        if ch.is_some() {
            self.pos += 1;
        }
        ch
    }

    fn skip_whitespace(&mut self) {
        while let Some(ch) = self.peek() {
            if ch.is_whitespace() {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn skip_comment(&mut self) -> Result<(), LexError> {
        // Already consumed opening (*
        let mut depth = 1;
        while depth > 0 {
            match self.advance() {
                Some('(') if self.peek() == Some('*') => {
                    self.advance();
                    depth += 1;
                }
                Some('*') if self.peek() == Some(')') => {
                    self.advance();
                    depth -= 1;
                }
                None => {
                    return Err(LexError {
                        message: "Unterminated comment".to_string(),
                        pos: self.pos,
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn read_string(&mut self) -> Result<String, LexError> {
        let mut s = String::new();
        loop {
            match self.advance() {
                Some('"') => return Ok(s),
                Some('\\') => {
                    match self.advance() {
                        Some('n') => s.push('\n'),
                        Some('t') => s.push('\t'),
                        Some('r') => s.push('\r'),
                        Some('\\') => s.push('\\'),
                        Some('"') => s.push('"'),
                        Some(c) => {
                            s.push('\\');
                            s.push(c);
                        }
                        None => {
                            return Err(LexError {
                                message: "Unterminated string escape".to_string(),
                                pos: self.pos,
                            });
                        }
                    }
                }
                Some(c) => s.push(c),
                None => {
                    return Err(LexError {
                        message: "Unterminated string literal".to_string(),
                        pos: self.pos,
                    });
                }
            }
        }
    }

    fn read_number(&mut self, first: char) -> Token {
        let mut num_str = String::new();
        num_str.push(first);

        // Read integer part
        while let Some(ch) = self.peek() {
            if ch.is_ascii_digit() {
                num_str.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        // Check for real (decimal point)
        if self.peek() == Some('.') && self.peek_ahead(1).map_or(false, |c| c.is_ascii_digit()) {
            num_str.push('.');
            self.advance(); // consume '.'
            while let Some(ch) = self.peek() {
                if ch.is_ascii_digit() {
                    num_str.push(ch);
                    self.advance();
                } else {
                    break;
                }
            }

            // Check for scientific notation
            if self.peek() == Some('e') || self.peek() == Some('E') {
                num_str.push(self.advance().unwrap());
                if self.peek() == Some('+') || self.peek() == Some('-') {
                    num_str.push(self.advance().unwrap());
                }
                while let Some(ch) = self.peek() {
                    if ch.is_ascii_digit() {
                        num_str.push(ch);
                        self.advance();
                    } else {
                        break;
                    }
                }
            }

            // Check for complex (I suffix)
            if self.peek() == Some('I') {
                self.advance();
                // Simplified: treat as real for now
            }

            Token::Real(num_str)
        } else {
            // Check for complex (I suffix on integer)
            if self.peek() == Some('I') {
                self.advance();
                // Simplified: treat as real for now
                return Token::Real(num_str);
            }

            Token::Integer(num_str)
        }
    }

    fn read_ident(&mut self, first: char) -> String {
        let mut ident = String::new();
        ident.push(first);

        while let Some(ch) = self.peek() {
            if ch.is_alphanumeric() || ch == '_' || ch == '$' {
                ident.push(ch);
                self.advance();
            } else {
                break;
            }
        }

        ident
    }

    fn keyword_or_ident(ident: &str) -> Token {
        match ident {
            "True" => Token::True,
            "False" => Token::False,
            "Null" => Token::Null,
            "If" => Token::If,
            "Which" => Token::Which,
            "Switch" => Token::Switch,
            "match" => Token::Match,
            "For" => Token::For,
            "While" => Token::While,
            "Do" => Token::Do,
            "try" => Token::Try,
            "catch" => Token::Catch,
            "finally" => Token::Finally,
            "throw" => Token::Throw,
            "Function" => Token::Function,
            "class" => Token::Class,
            "extends" => Token::Extends,
            "with" => Token::With,
            "method" => Token::Method,
            "field" => Token::Field,
            "constructor" => Token::Constructor,
            "module" => Token::Module,
            "import" => Token::Import,
            "export" => Token::Export,
            "as" => Token::As,
            "rule" => Token::RuleKw,
            "Hold" => Token::Hold,
            "HoldComplete" => Token::HoldComplete,
            "ReleaseHold" => Token::ReleaseHold,
            "mixin" => Token::Mixin,
            _ => Token::Ident(ident.to_string()),
        }
    }

    pub fn tokenize(&mut self) -> Result<Vec<Token>, LexError> {
        loop {
            self.skip_whitespace();

            let ch = match self.peek() {
                Some(c) => c,
                None => {
                    self.tokens.push(Token::Eof);
                    return Ok(self.tokens.clone());
                }
            };

            match ch {
                // Comments: (* ... *)
                '(' if self.peek_ahead(1) == Some('*') => {
                    self.advance(); // consume '('
                    self.advance(); // consume '*'
                    self.skip_comment()?;
                }

                // Numbers
                '0'..='9' => {
                    self.advance();
                    let token = self.read_number(ch);
                    self.tokens.push(token);
                }

                // Identifiers and keywords
                'a'..='z' | 'A'..='Z' | '$' => {
                    self.advance();
                    let ident = self.read_ident(ch);
                    let token = Self::keyword_or_ident(&ident);
                    self.tokens.push(token);
                }

                // Strings
                '"' => {
                    self.advance();
                    let s = self.read_string()?;
                    self.tokens.push(Token::Str(s));
                }

                // Single-char tokens
                '(' => { self.advance(); self.tokens.push(Token::LParen); }
                ')' => { self.advance(); self.tokens.push(Token::RParen); }
                '[' => {
                    self.advance();
                    if self.peek() == Some('[') {
                        self.advance();
                        self.double_bracket_depth += 1;
                        self.tokens.push(Token::LDoubleBracket);
                    } else {
                        self.tokens.push(Token::LBracket);
                    }
                }
                ']' => {
                    self.advance();
                    if self.peek() == Some(']') && self.double_bracket_depth > 0 {
                        self.advance();
                        self.double_bracket_depth -= 1;
                        self.tokens.push(Token::RDoubleBracket);
                    } else {
                        self.tokens.push(Token::RBracket);
                    }
                }
                '{' => { self.advance(); self.tokens.push(Token::LBrace); }
                '}' => { self.advance(); self.tokens.push(Token::RBrace); }
                ',' => { self.advance(); self.tokens.push(Token::Comma); }
                ';' => { self.advance(); self.tokens.push(Token::Semicolon); }
                '^' => { self.advance(); self.tokens.push(Token::Caret); }
                '\'' => { self.advance(); self.tokens.push(Token::Quote); }
                '~' => { self.advance(); self.tokens.push(Token::Tilde); }

                // Dot: member access or decimal
                '.' => {
                    self.advance();
                    self.tokens.push(Token::Dot);
                }

                // Slot: # or #N
                '#' => {
                    self.advance();
                    if let Some(c) = self.peek() {
                        if c.is_ascii_digit() {
                            let mut num_str = String::new();
                            while let Some(d) = self.peek() {
                                if d.is_ascii_digit() {
                                    num_str.push(d);
                                    self.advance();
                                } else {
                                    break;
                                }
                            }
                            let n: usize = num_str.parse().unwrap_or(1);
                            self.tokens.push(Token::SlotN(n));
                        } else {
                            self.tokens.push(Token::Slot);
                        }
                    } else {
                        self.tokens.push(Token::Slot);
                    }
                }

                // Operators starting with /
                '/' => {
                    self.advance();
                    match self.peek() {
                        Some('.') => {
                            self.advance();
                            self.tokens.push(Token::ReplaceAll);
                        }
                        Some('/') => {
                            self.advance();
                            if self.peek() == Some('.') {
                                self.advance();
                                self.tokens.push(Token::ReplaceRepeated);
                            } else {
                                self.tokens.push(Token::Pipe);
                            }
                        }
                        Some('@') => {
                            self.advance();
                            self.tokens.push(Token::MapOp);
                        }
                        Some(';') => {
                            self.advance();
                            self.tokens.push(Token::ColonSlashSemicolon);
                        }
                        _ => {
                            self.tokens.push(Token::Slash);
                        }
                    }
                }

                // Operators starting with :
                ':' => {
                    self.advance();
                    match self.peek() {
                        Some('=') => {
                            self.advance();
                            self.tokens.push(Token::DelayedAssign);
                        }
                        Some('>') => {
                            self.advance();
                            self.tokens.push(Token::DelayedRule);
                        }
                        _ => {
                            self.tokens.push(Token::Colon);
                        }
                    }
                }

                // Operators starting with -
                '-' => {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        self.tokens.push(Token::Rule);
                    } else {
                        self.tokens.push(Token::Minus);
                    }
                }

                // Operators starting with =
                '=' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        self.tokens.push(Token::Equal);
                    } else if self.peek() == Some('>') {
                        self.advance();
                        self.tokens.push(Token::FatArrow);
                    } else {
                        self.tokens.push(Token::Assign);
                    }
                }

                // Operators starting with !
                '!' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        self.tokens.push(Token::Unequal);
                    } else {
                        self.tokens.push(Token::Not);
                    }
                }

                // Operators starting with <
                '<' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        self.tokens.push(Token::LessEqual);
                    } else if self.peek() == Some('|') {
                        self.advance();
                        self.tokens.push(Token::LAssoc);
                    } else if self.peek() == Some('>') {
                        self.advance();
                        self.tokens.push(Token::StringJoinOp);
                    } else {
                        self.tokens.push(Token::Less);
                    }
                }

                // Operators starting with >
                '>' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        self.tokens.push(Token::GreaterEqual);
                    } else {
                        self.tokens.push(Token::Greater);
                    }
                }

                // Operators starting with &
                '&' => {
                    self.advance();
                    if self.peek() == Some('&') {
                        self.advance();
                        self.tokens.push(Token::And);
                    } else {
                        // & is used in pure functions — treat as a special token
                        // For now, just push as a symbol
                        self.tokens.push(Token::Ident("&".to_string()));
                    }
                }

                // Operators starting with |
                '|' => {
                    self.advance();
                    if self.peek() == Some('|') {
                        self.advance();
                        self.tokens.push(Token::Or);
                    } else if self.peek() == Some('>') {
                        self.advance();
                        self.tokens.push(Token::RAssoc);
                    } else {
                        // | in pattern alternatives
                        self.tokens.push(Token::Ident("|".to_string()));
                    }
                }

                // @ operators
                '@' => {
                    self.advance();
                    // Check for @@
                    if self.peek() == Some('@') {
                        self.advance();
                        self.tokens.push(Token::ApplyOp);
                    } else {
                        // Check for @transform
                        let save_pos = self.pos;
                        self.skip_whitespace();
                        let mut ident = String::new();
                        while let Some(c) = self.peek() {
                            if c.is_alphanumeric() || c == '_' {
                                ident.push(c);
                                self.advance();
                            } else {
                                break;
                            }
                        }
                        if ident == "transform" {
                            self.tokens.push(Token::AtTransform);
                        } else {
                            // Not @transform, restore and push @
                            self.pos = save_pos;
                            self.tokens.push(Token::At);
                        }
                    }
                }

                // + operator
                '+' => { self.advance(); self.tokens.push(Token::Plus); }

                // * operator
                '*' => { self.advance(); self.tokens.push(Token::Star); }

                _ => {
                    return Err(LexError {
                        message: format!("Unexpected character: '{}'", ch),
                        pos: self.pos,
                    });
                }
            }
        }
    }
}

/// Convenience function to tokenize a string.
pub fn tokenize(input: &str) -> Result<Vec<Token>, LexError> {
    Lexer::new(input).tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_expression() {
        let tokens = tokenize("f[x, 1]").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("f".to_string()),
            Token::LBracket,
            Token::Ident("x".to_string()),
            Token::Comma,
            Token::Integer("1".to_string()),
            Token::RBracket,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_operators() {
        let tokens = tokenize("a + b * c").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("a".to_string()),
            Token::Plus,
            Token::Ident("b".to_string()),
            Token::Star,
            Token::Ident("c".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_rule_operators() {
        let tokens = tokenize("a /. rules").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("a".to_string()),
            Token::ReplaceAll,
            Token::Ident("rules".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_string() {
        let tokens = tokenize("\"hello world\"").unwrap();
        assert_eq!(tokens, vec![
            Token::Str("hello world".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_comment() {
        let tokens = tokenize("x (* comment *) y").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("x".to_string()),
            Token::Ident("y".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_slot() {
        let tokens = tokenize("# + #1 + #2").unwrap();
        assert_eq!(tokens, vec![
            Token::Slot,
            Token::Plus,
            Token::SlotN(1),
            Token::Plus,
            Token::SlotN(2),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_keywords() {
        let tokens = tokenize("If True False Null").unwrap();
        assert_eq!(tokens, vec![
            Token::If,
            Token::True,
            Token::False,
            Token::Null,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_more_keywords() {
        let tokens = tokenize("For While Do Which Switch Function").unwrap();
        assert_eq!(tokens, vec![
            Token::For,
            Token::While,
            Token::Do,
            Token::Which,
            Token::Switch,
            Token::Function,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_delayed_assign() {
        let tokens = tokenize("f[x_] := x^2").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("f".to_string()),
            Token::LBracket,
            Token::Ident("x_".to_string()),
            Token::RBracket,
            Token::DelayedAssign,
            Token::Ident("x".to_string()),
            Token::Caret,
            Token::Integer("2".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_delayed_rule() {
        let tokens = tokenize("x :> y").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("x".to_string()),
            Token::DelayedRule,
            Token::Ident("y".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_replace_repeated() {
        let tokens = tokenize("x //. rules").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("x".to_string()),
            Token::ReplaceRepeated,
            Token::Ident("rules".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_map_apply() {
        let tokens = tokenize("f /@ list").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("f".to_string()),
            Token::MapOp,
            Token::Ident("list".to_string()),
            Token::Eof,
        ]);

        let tokens = tokenize("f @@ args").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("f".to_string()),
            Token::ApplyOp,
            Token::Ident("args".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_comparison_ops() {
        let tokens = tokenize("a == b != c <= d >= e < f > g").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("a".to_string()),
            Token::Equal,
            Token::Ident("b".to_string()),
            Token::Unequal,
            Token::Ident("c".to_string()),
            Token::LessEqual,
            Token::Ident("d".to_string()),
            Token::GreaterEqual,
            Token::Ident("e".to_string()),
            Token::Less,
            Token::Ident("f".to_string()),
            Token::Greater,
            Token::Ident("g".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_logical_ops() {
        let tokens = tokenize("a && b || !c").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("a".to_string()),
            Token::And,
            Token::Ident("b".to_string()),
            Token::Or,
            Token::Not,
            Token::Ident("c".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_assoc_delimiters() {
        let tokens = tokenize("<| \"a\" -> 1 |>").unwrap();
        assert_eq!(tokens, vec![
            Token::LAssoc,
            Token::Str("a".to_string()),
            Token::Rule,
            Token::Integer("1".to_string()),
            Token::RAssoc,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_string_join_op() {
        let tokens = tokenize("\"a\" <> \"b\"").unwrap();
        assert_eq!(tokens, vec![
            Token::Str("a".to_string()),
            Token::StringJoinOp,
            Token::Str("b".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_double_brackets() {
        let tokens = tokenize("list[[1]]").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("list".to_string()),
            Token::LDoubleBracket,
            Token::Integer("1".to_string()),
            Token::RDoubleBracket,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_real_number() {
        let tokens = tokenize("3.14").unwrap();
        assert_eq!(tokens, vec![
            Token::Real("3.14".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_nested_comments() {
        let tokens = tokenize("a (* outer (* inner *) *) b").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("a".to_string()),
            Token::Ident("b".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_string_escape() {
        let tokens = tokenize(r#""hello\nworld""#).unwrap();
        assert_eq!(tokens, vec![
            Token::Str("hello\nworld".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_fat_arrow() {
        let tokens = tokenize("x => y").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("x".to_string()),
            Token::FatArrow,
            Token::Ident("y".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_guard() {
        let tokens = tokenize("x_ /; x > 0").unwrap();
        assert_eq!(tokens, vec![
            Token::Ident("x_".to_string()),
            Token::ColonSlashSemicolon,
            Token::Ident("x".to_string()),
            Token::Greater,
            Token::Integer("0".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_class_keywords() {
        let tokens = tokenize("class Foo extends Bar with Baz").unwrap();
        assert_eq!(tokens, vec![
            Token::Class,
            Token::Ident("Foo".to_string()),
            Token::Extends,
            Token::Ident("Bar".to_string()),
            Token::With,
            Token::Ident("Baz".to_string()),
            Token::Eof,
        ]);
    }

    #[test]
    fn test_module_import_export() {
        let tokens = tokenize("module import export as").unwrap();
        assert_eq!(tokens, vec![
            Token::Module,
            Token::Import,
            Token::Export,
            Token::As,
            Token::Eof,
        ]);
    }

    #[test]
    fn test_unexpected_char() {
        let result = tokenize("1 ` 2");
        assert!(result.is_err());
    }

    #[test]
    fn test_unterminated_string() {
        let result = tokenize("\"hello");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_input() {
        let tokens = tokenize("").unwrap();
        assert_eq!(tokens, vec![Token::Eof]);
    }

    #[test]
    fn test_whitespace_only() {
        let tokens = tokenize("   \t\n  ").unwrap();
        assert_eq!(tokens, vec![Token::Eof]);
    }
}
