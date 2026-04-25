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
pub struct Span {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

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
    LParen,         // (
    RParen,         // )
    LBracket,       // [
    RBracket,       // ]
    LBrace,         // {
    RBrace,         // }
    LAssoc,         // <|
    RAssoc,         // |>
    LDoubleBracket, // [[
    RDoubleBracket, // ]]

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
    ColonColon, // ::  (MessageName separator)

    // ── Multi-char operators ──
    Assign,          // =
    DelayedAssign,   // :=
    Rule,            // ->
    DelayedRule,     // :>
    ReplaceAll,      // /.
    ReplaceRepeated, // /.
    MapOp,           // /@
    ApplyOp,         // @@
    At,              // @
    Pipe,            // //
    Equal,           // ==
    Unequal,         // !=
    Less,            // <
    Greater,         // >
    LessEqual,       // <=
    GreaterEqual,    // >=
    And,             // &&
    Or,              // ||
    Not,             // !
    FatArrow,        // =>
    StringJoinOp,    // <>
    PipeAlt,         // | (pattern alternatives)
    FuncRef,         // & (function reference / pure function)

    // ── Special ──
    Quote,        // ' (quote)
    Tilde,        // ~ (splice)
    QuestionMark, // ?
    Slot,         // #
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
            Token::ColonColon => write!(f, "::"),
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
            Token::PipeAlt => write!(f, "|"),
            Token::FuncRef => write!(f, "&"),
            Token::Quote => write!(f, "'"),
            Token::Tilde => write!(f, "~"),
            Token::QuestionMark => write!(f, "?"),
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
    tokens: Vec<SpannedToken>,
    /// Tracks nesting depth of [[ ... ]] Part-access brackets.
    /// Only emit RDoubleBracket when depth > 0; otherwise ]] is two RBracket tokens.
    double_bracket_depth: usize,
    line: usize,
    col: usize,
}

#[derive(Debug)]
pub struct LexError {
    pub message: String,
    pub pos: usize,
    pub line: usize,
    pub col: usize,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}: {}", self.line, self.col, self.message)
    }
}

impl Lexer {
    pub fn new(input: &str) -> Self {
        Lexer {
            input: input.chars().collect(),
            pos: 0,
            tokens: Vec::new(),
            double_bracket_depth: 0,
            line: 1,
            col: 1,
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
        if let Some(c) = ch {
            self.pos += 1;
            if c == '\n' {
                self.line += 1;
                self.col = 1;
            } else {
                self.col += 1;
            }
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
                        line: self.line,
                        col: self.col,
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
                Some('\\') => match self.advance() {
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
                            line: self.line,
                            col: self.col,
                        });
                    }
                },
                Some(c) => s.push(c),
                None => {
                    return Err(LexError {
                        message: "Unterminated string literal".to_string(),
                        pos: self.pos,
                        line: self.line,
                        col: self.col,
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
        if self.peek() == Some('.') && self.peek_ahead(1).is_some_and(|c| c.is_ascii_digit()) {
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
            if ch.is_alphanumeric() || ch == '_' || ch == '$' || ch == '`' {
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

    pub fn tokenize(&mut self) -> Result<Vec<SpannedToken>, LexError> {
        loop {
            self.skip_whitespace();

            let ch = match self.peek() {
                Some(c) => c,
                None => {
                    self.tokens.push(SpannedToken {
                        token: Token::Eof,
                        span: Span {
                            line: self.line,
                            col: self.col,
                        },
                    });
                    return Ok(self.tokens.clone());
                }
            };

            let start_line = self.line;
            let start_col = self.col;

            macro_rules! push {
                ($tok:expr) => {
                    self.tokens.push(SpannedToken {
                        token: $tok,
                        span: Span {
                            line: start_line,
                            col: start_col,
                        },
                    })
                };
            }

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
                    push!(token);
                }

                // Identifiers and keywords
                'a'..='z' | 'A'..='Z' | '$' | '_' => {
                    self.advance();
                    let ident = self.read_ident(ch);
                    let token = Self::keyword_or_ident(&ident);
                    push!(token);
                }

                // Strings
                '"' => {
                    self.advance();
                    let s = self.read_string()?;
                    push!(Token::Str(s));
                }

                // Single-char tokens
                '(' => {
                    self.advance();
                    push!(Token::LParen);
                }
                ')' => {
                    self.advance();
                    push!(Token::RParen);
                }
                '[' => {
                    self.advance();
                    if self.peek() == Some('[') {
                        self.advance();
                        self.double_bracket_depth += 1;
                        push!(Token::LDoubleBracket);
                    } else {
                        push!(Token::LBracket);
                    }
                }
                ']' => {
                    self.advance();
                    if self.peek() == Some(']') && self.double_bracket_depth > 0 {
                        self.advance();
                        self.double_bracket_depth -= 1;
                        push!(Token::RDoubleBracket);
                    } else {
                        push!(Token::RBracket);
                    }
                }
                '{' => {
                    self.advance();
                    push!(Token::LBrace);
                }
                '}' => {
                    self.advance();
                    push!(Token::RBrace);
                }
                ',' => {
                    self.advance();
                    push!(Token::Comma);
                }
                ';' => {
                    self.advance();
                    push!(Token::Semicolon);
                }
                '^' => {
                    self.advance();
                    push!(Token::Caret);
                }
                '\'' => {
                    self.advance();
                    push!(Token::Quote);
                }
                '~' => {
                    self.advance();
                    push!(Token::Tilde);
                }
                '?' => {
                    self.advance();
                    push!(Token::QuestionMark);
                }

                // Dot: member access or decimal
                '.' => {
                    self.advance();
                    push!(Token::Dot);
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
                            push!(Token::SlotN(n));
                        } else {
                            push!(Token::Slot);
                        }
                    } else {
                        push!(Token::Slot);
                    }
                }

                // Operators starting with /
                '/' => {
                    self.advance();
                    match self.peek() {
                        Some('.') => {
                            self.advance();
                            push!(Token::ReplaceAll);
                        }
                        Some('/') => {
                            self.advance();
                            if self.peek() == Some('.') {
                                self.advance();
                                push!(Token::ReplaceRepeated);
                            } else {
                                push!(Token::Pipe);
                            }
                        }
                        Some('@') => {
                            self.advance();
                            push!(Token::MapOp);
                        }
                        Some(';') => {
                            self.advance();
                            push!(Token::ColonSlashSemicolon);
                        }
                        _ => {
                            push!(Token::Slash);
                        }
                    }
                }

                // Operators starting with :
                ':' => {
                    self.advance();
                    match self.peek() {
                        Some('=') => {
                            self.advance();
                            push!(Token::DelayedAssign);
                        }
                        Some('>') => {
                            self.advance();
                            push!(Token::DelayedRule);
                        }
                        Some(':') => {
                            self.advance();
                            push!(Token::ColonColon);
                        }
                        _ => {
                            push!(Token::Colon);
                        }
                    }
                }

                // Operators starting with -
                '-' => {
                    self.advance();
                    if self.peek() == Some('>') {
                        self.advance();
                        push!(Token::Rule);
                    } else {
                        push!(Token::Minus);
                    }
                }

                // Operators starting with =
                '=' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        push!(Token::Equal);
                    } else if self.peek() == Some('>') {
                        self.advance();
                        push!(Token::FatArrow);
                    } else {
                        push!(Token::Assign);
                    }
                }

                // Operators starting with !
                '!' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        push!(Token::Unequal);
                    } else {
                        push!(Token::Not);
                    }
                }

                // Operators starting with <
                '<' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        push!(Token::LessEqual);
                    } else if self.peek() == Some('|') {
                        self.advance();
                        push!(Token::LAssoc);
                    } else if self.peek() == Some('>') {
                        self.advance();
                        push!(Token::StringJoinOp);
                    } else {
                        push!(Token::Less);
                    }
                }

                // Operators starting with >
                '>' => {
                    self.advance();
                    if self.peek() == Some('=') {
                        self.advance();
                        push!(Token::GreaterEqual);
                    } else {
                        push!(Token::Greater);
                    }
                }

                // Operators starting with &
                '&' => {
                    self.advance();
                    if self.peek() == Some('&') {
                        self.advance();
                        push!(Token::And);
                    } else {
                        // & is used in pure functions — treat as a special token
                        push!(Token::FuncRef);
                    }
                }

                // Operators starting with |
                '|' => {
                    self.advance();
                    if self.peek() == Some('|') {
                        self.advance();
                        push!(Token::Or);
                    } else if self.peek() == Some('>') {
                        self.advance();
                        push!(Token::RAssoc);
                    } else {
                        // | in pattern alternatives
                        push!(Token::PipeAlt);
                    }
                }

                // @ operators
                '@' => {
                    self.advance();
                    // Check for @@
                    if self.peek() == Some('@') {
                        self.advance();
                        push!(Token::ApplyOp);
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
                            push!(Token::AtTransform);
                        } else {
                            // Not @transform, restore and push @
                            self.pos = save_pos;
                            push!(Token::At);
                        }
                    }
                }

                // + operator
                '+' => {
                    self.advance();
                    push!(Token::Plus);
                }

                // * operator
                '*' => {
                    self.advance();
                    push!(Token::Star);
                }

                _ => {
                    return Err(LexError {
                        message: format!("Unexpected character: '{}'", ch),
                        pos: self.pos,
                        line: self.line,
                        col: self.col,
                    });
                }
            }
        }
    }
}

/// Convenience function to tokenize a string.
pub fn tokenize(input: &str) -> Result<Vec<SpannedToken>, LexError> {
    Lexer::new(input).tokenize()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(input: &str) -> Vec<Token> {
        tokenize(input)
            .unwrap()
            .into_iter()
            .map(|t| t.token)
            .collect()
    }

    #[test]
    fn test_simple_expression() {
        let tokens = tokens("f[x, 1]");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("f".to_string()),
                Token::LBracket,
                Token::Ident("x".to_string()),
                Token::Comma,
                Token::Integer("1".to_string()),
                Token::RBracket,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_operators() {
        let tokens = tokens("a + b * c");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("a".to_string()),
                Token::Plus,
                Token::Ident("b".to_string()),
                Token::Star,
                Token::Ident("c".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_rule_operators() {
        let tokens = tokens("a /. rules");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("a".to_string()),
                Token::ReplaceAll,
                Token::Ident("rules".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_string() {
        let tokens = tokens("\"hello world\"");
        assert_eq!(
            tokens,
            vec![Token::Str("hello world".to_string()), Token::Eof,]
        );
    }

    #[test]
    fn test_comment() {
        let tokens = tokens("x (* comment *) y");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("x".to_string()),
                Token::Ident("y".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_slot() {
        let tokens = tokens("# + #1 + #2");
        assert_eq!(
            tokens,
            vec![
                Token::Slot,
                Token::Plus,
                Token::SlotN(1),
                Token::Plus,
                Token::SlotN(2),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_keywords() {
        let tokens = tokens("If True False Null");
        assert_eq!(
            tokens,
            vec![
                Token::If,
                Token::True,
                Token::False,
                Token::Null,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_more_keywords() {
        let tokens = tokens("For While Do Which Switch Function");
        assert_eq!(
            tokens,
            vec![
                Token::For,
                Token::While,
                Token::Do,
                Token::Which,
                Token::Switch,
                Token::Function,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_delayed_assign() {
        let tokens = tokens("f[x_] := x^2");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("f".to_string()),
                Token::LBracket,
                Token::Ident("x_".to_string()),
                Token::RBracket,
                Token::DelayedAssign,
                Token::Ident("x".to_string()),
                Token::Caret,
                Token::Integer("2".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_delayed_rule() {
        let tokens = tokens("x :> y");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("x".to_string()),
                Token::DelayedRule,
                Token::Ident("y".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_replace_repeated() {
        let tokens = tokens("x //. rules");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("x".to_string()),
                Token::ReplaceRepeated,
                Token::Ident("rules".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_map_apply() {
        let toks = tokens("f /@ list");
        assert_eq!(
            toks,
            vec![
                Token::Ident("f".to_string()),
                Token::MapOp,
                Token::Ident("list".to_string()),
                Token::Eof,
            ]
        );

        let toks = tokens("f @@ args");
        assert_eq!(
            toks,
            vec![
                Token::Ident("f".to_string()),
                Token::ApplyOp,
                Token::Ident("args".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_comparison_ops() {
        let tokens = tokens("a == b != c <= d >= e < f > g");
        assert_eq!(
            tokens,
            vec![
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
            ]
        );
    }

    #[test]
    fn test_logical_ops() {
        let tokens = tokens("a && b || !c");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("a".to_string()),
                Token::And,
                Token::Ident("b".to_string()),
                Token::Or,
                Token::Not,
                Token::Ident("c".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_assoc_delimiters() {
        let tokens = tokens("<| \"a\" -> 1 |>");
        assert_eq!(
            tokens,
            vec![
                Token::LAssoc,
                Token::Str("a".to_string()),
                Token::Rule,
                Token::Integer("1".to_string()),
                Token::RAssoc,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_string_join_op() {
        let tokens = tokens("\"a\" <> \"b\"");
        assert_eq!(
            tokens,
            vec![
                Token::Str("a".to_string()),
                Token::StringJoinOp,
                Token::Str("b".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_double_brackets() {
        let tokens = tokens("list[[1]]");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("list".to_string()),
                Token::LDoubleBracket,
                Token::Integer("1".to_string()),
                Token::RDoubleBracket,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_real_number() {
        let tokens = tokens("3.14");
        assert_eq!(tokens, vec![Token::Real("3.14".to_string()), Token::Eof,]);
    }

    #[test]
    fn test_nested_comments() {
        let tokens = tokens("a (* outer (* inner *) *) b");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("a".to_string()),
                Token::Ident("b".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_string_escape() {
        let tokens = tokens(r#""hello\nworld""#);
        assert_eq!(
            tokens,
            vec![Token::Str("hello\nworld".to_string()), Token::Eof,]
        );
    }

    #[test]
    fn test_fat_arrow() {
        let tokens = tokens("x => y");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("x".to_string()),
                Token::FatArrow,
                Token::Ident("y".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_guard() {
        let tokens = tokens("x_ /; x > 0");
        assert_eq!(
            tokens,
            vec![
                Token::Ident("x_".to_string()),
                Token::ColonSlashSemicolon,
                Token::Ident("x".to_string()),
                Token::Greater,
                Token::Integer("0".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_class_keywords() {
        let toks = tokens("class Foo extends Bar with Baz");
        assert_eq!(
            toks,
            vec![
                Token::Class,
                Token::Ident("Foo".to_string()),
                Token::Extends,
                Token::Ident("Bar".to_string()),
                Token::With,
                Token::Ident("Baz".to_string()),
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_module_import_export() {
        let toks = tokens("module import export as");
        assert_eq!(
            toks,
            vec![
                Token::Module,
                Token::Import,
                Token::Export,
                Token::As,
                Token::Eof,
            ]
        );
    }

    #[test]
    fn test_unexpected_char() {
        // Backtick is now valid in identifiers (e.g., Developer`MachineIntegerQ).
        let result = tokenize("1 \\ 2");
        assert!(result.is_err());
    }

    #[test]
    fn test_unterminated_string() {
        let result = tokenize("\"hello");
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_input() {
        let toks = tokens("");
        assert_eq!(toks, vec![Token::Eof]);
    }

    #[test]
    fn test_whitespace_only() {
        let toks = tokens("   \t\n  ");
        assert_eq!(toks, vec![Token::Eof]);
    }
}
