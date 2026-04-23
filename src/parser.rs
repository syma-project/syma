/// Parser for Syma language.
///
/// Recursive descent parser with precedence climbing for operators.
/// Implements the EBNF grammar from the language specification.

use crate::ast::*;
use crate::lexer::Token;
use rug::Integer;
use rug::Float;

pub struct Parser {
    tokens: Vec<Token>,
    pos: usize,
}

#[derive(Debug)]
pub struct ParseError {
    pub message: String,
    pub token: Option<Token>,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.token {
            Some(tok) => write!(f, "Parse error: {} (found {})", self.message, tok),
            None => write!(f, "Parse error: {}", self.message),
        }
    }
}

impl Parser {
    pub fn new(tokens: Vec<Token>) -> Self {
        Parser { tokens, pos: 0 }
    }

    fn peek(&self) -> &Token {
        self.tokens.get(self.pos).unwrap_or(&Token::Eof)
    }

    fn advance(&mut self) -> Token {
        let tok = self.tokens.get(self.pos).cloned().unwrap_or(Token::Eof);
        if self.pos < self.tokens.len() {
            self.pos += 1;
        }
        tok
    }

    fn expect(&mut self, expected: &Token) -> Result<(), ParseError> {
        let tok = self.advance();
        if &tok == expected {
            Ok(())
        } else {
            Err(ParseError {
                message: format!("Expected '{}', found '{}'", expected, tok),
                token: Some(tok),
            })
        }
    }

    fn at(&self, tok: &Token) -> bool {
        self.peek() == tok
    }

    // ── Top-level ──

    pub fn parse_program(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut stmts = Vec::new();
        while self.peek() != &Token::Eof {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
            // Optional semicolon
            if self.at(&Token::Semicolon) {
                self.advance();
            }
        }
        Ok(stmts)
    }

    fn parse_statement(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Import => self.parse_import(),
            Token::Export => self.parse_export(),
            Token::Class => self.parse_class_def(),
            Token::Module => self.parse_module_def(),
            Token::Mixin => self.parse_mixin_def(),
            Token::RuleKw => self.parse_rule_def(),
            Token::Match => self.parse_match(),
            Token::Try => self.parse_try(),
            Token::Throw => self.parse_throw(),
            _ => {
                let expr = self.parse_expression()?;
                // Check for assignment or function definition
                match self.peek() {
                    Token::Assign => {
                        self.advance();
                        let rhs = self.parse_expression()?;
                        Ok(Expr::Assign {
                            lhs: Box::new(expr),
                            rhs: Box::new(rhs),
                        })
                    }
                    Token::DelayedAssign => {
                        // f[x_] := body — function definition
                        self.advance(); // consume :=
                        if let Expr::Call { head, args } = expr {
                            if let Expr::Symbol(name) = *head {
                                let params: Vec<Expr> = args.into_iter()
                                    .map(Self::convert_pattern)
                                    .collect();
                                let body = self.parse_expression()?;
                                Ok(Expr::FuncDef {
                                    name,
                                    params,
                                    body: Box::new(body),
                                    delayed: true,
                                })
                            } else {
                                Err(ParseError {
                                    message: "Invalid function definition (head not a symbol)".to_string(),
                                    token: Some(Token::DelayedAssign),
                                })
                            }
                        } else {
                            Err(ParseError {
                                message: format!("Invalid function definition (expr is {:?})", expr),
                                token: Some(Token::DelayedAssign),
                            })
                        }
                    }
                    _ => Ok(expr),
                }
            }
        }
    }

    fn parse_import(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::Import)?;
        let mut module = vec![self.expect_ident()?];
        while self.at(&Token::Dot) {
            self.advance();
            // Check for selective import: .{A, B, C}
            if self.at(&Token::LBrace) {
                self.advance();
                let mut selective = vec![self.expect_ident()?];
                while self.at(&Token::Comma) {
                    self.advance();
                    selective.push(self.expect_ident()?);
                }
                self.expect(&Token::RBrace)?;
                let alias = if self.at(&Token::As) {
                    self.advance();
                    Some(self.expect_ident()?)
                } else {
                    None
                };
                return Ok(Expr::Import {
                    module,
                    selective: Some(selective),
                    alias,
                });
            }
            module.push(self.expect_ident()?);
        }
        let alias = if self.at(&Token::As) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };
        Ok(Expr::Import {
            module,
            selective: None,
            alias,
        })
    }

    fn parse_export(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::Export)?;
        let mut names = vec![self.expect_ident()?];
        while self.at(&Token::Comma) {
            self.advance();
            names.push(self.expect_ident()?);
        }
        Ok(Expr::Export(names))
    }

    fn parse_class_def(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::Class)?;
        let name = self.expect_ident()?;

        let parent = if self.at(&Token::Extends) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        let mixins = if self.at(&Token::With) {
            self.advance();
            let mut mixins = vec![self.expect_ident()?];
            while self.at(&Token::Comma) {
                self.advance();
                mixins.push(self.expect_ident()?);
            }
            mixins
        } else {
            vec![]
        };

        self.expect(&Token::LBrace)?;
        let mut members = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            members.push(self.parse_member_def()?);
        }
        self.expect(&Token::RBrace)?;

        Ok(Expr::ClassDef {
            name,
            parent,
            mixins,
            members,
        })
    }

    fn parse_member_def(&mut self) -> Result<MemberDef, ParseError> {
        match self.peek().clone() {
            Token::Field => self.parse_field_def(),
            Token::Method => self.parse_method_def(),
            Token::Constructor => self.parse_constructor_def(),
            Token::AtTransform => self.parse_transform_def(),
            _ => Err(ParseError {
                message: "Expected field, method, constructor, or @transform".to_string(),
                token: Some(self.peek().clone()),
            }),
        }
    }

    fn parse_field_def(&mut self) -> Result<MemberDef, ParseError> {
        self.expect(&Token::Field)?;
        let name = self.expect_ident()?;

        let type_hint = if self.at(&Token::Colon) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        let default = if self.at(&Token::Assign) {
            self.advance();
            Some(self.parse_expression()?)
        } else {
            None
        };

        Ok(MemberDef::Field {
            name,
            type_hint,
            default,
        })
    }

    fn parse_method_def(&mut self) -> Result<MemberDef, ParseError> {
        self.expect(&Token::Method)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LBracket)?;

        let params = if self.at(&Token::RBracket) {
            vec![]
        } else {
            self.parse_pattern_list()?
        };
        self.expect(&Token::RBracket)?;

        let return_type = if self.at(&Token::Colon) {
            self.advance();
            Some(self.expect_ident()?)
        } else {
            None
        };

        let body = if self.at(&Token::DelayedAssign) {
            self.advance();
            MethodBody::Expr(self.parse_expression()?)
        } else if self.at(&Token::Assign) {
            self.advance();
            MethodBody::Expr(self.parse_expression()?)
        } else {
            self.expect(&Token::LBrace)?;
            let mut stmts = Vec::new();
            while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
                stmts.push(self.parse_statement()?);
                if self.at(&Token::Semicolon) {
                    self.advance();
                }
            }
            self.expect(&Token::RBrace)?;
            MethodBody::Block(stmts)
        };

        Ok(MemberDef::Method {
            name,
            params,
            return_type,
            body,
        })
    }

    fn parse_constructor_def(&mut self) -> Result<MemberDef, ParseError> {
        self.expect(&Token::Constructor)?;
        self.expect(&Token::LBracket)?;

        let params = if self.at(&Token::RBracket) {
            vec![]
        } else {
            self.parse_pattern_list()?
        };
        self.expect(&Token::RBracket)?;

        self.expect(&Token::LBrace)?;
        let mut body = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            body.push(self.parse_statement()?);
            if self.at(&Token::Semicolon) {
                self.advance();
            }
        }
        self.expect(&Token::RBrace)?;

        Ok(MemberDef::Constructor { params, body })
    }

    fn parse_transform_def(&mut self) -> Result<MemberDef, ParseError> {
        self.expect(&Token::AtTransform)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LBrace)?;
        let mut rules = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            let pattern = self.parse_pattern()?;
            let rhs = if self.at(&Token::Rule) {
                self.advance();
                self.parse_expression()?
            } else if self.at(&Token::DelayedRule) {
                self.advance();
                self.parse_expression()?
            } else {
                return Err(ParseError {
                    message: "Expected -> or :> in transform rule".to_string(),
                    token: Some(self.peek().clone()),
                });
            };
            rules.push((pattern, rhs));
        }
        self.expect(&Token::RBrace)?;

        Ok(MemberDef::Transform { name, rules })
    }

    fn parse_module_def(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::Module)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LBrace)?;

        let exports = if self.at(&Token::Export) {
            self.advance();
            let mut names = vec![self.expect_ident()?];
            while self.at(&Token::Comma) {
                self.advance();
                names.push(self.expect_ident()?);
            }
            if self.at(&Token::Semicolon) {
                self.advance();
            }
            names
        } else {
            vec![]
        };

        let mut body = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            body.push(self.parse_statement()?);
            if self.at(&Token::Semicolon) {
                self.advance();
            }
        }
        self.expect(&Token::RBrace)?;

        Ok(Expr::ModuleDef { name, exports, body })
    }

    fn parse_mixin_def(&mut self) -> Result<Expr, ParseError> {
        // Mixin is parsed as a class without constructor
        self.expect(&Token::Mixin)?;
        let name = self.expect_ident()?;
        self.expect(&Token::LBrace)?;
        let mut members = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            members.push(self.parse_member_def()?);
        }
        self.expect(&Token::RBrace)?;

        Ok(Expr::ClassDef {
            name,
            parent: None,
            mixins: vec![],
            members,
        })
    }

    fn parse_rule_def(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::RuleKw)?;
        let name = self.expect_ident()?;
        self.expect(&Token::Assign)?;
        self.expect(&Token::LBrace)?;

        let mut rules = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            let lhs = self.parse_pattern()?;
            let rhs = if self.at(&Token::Rule) {
                self.advance();
                self.parse_expression()?
            } else if self.at(&Token::DelayedRule) {
                self.advance();
                self.parse_expression()?
            } else {
                return Err(ParseError {
                    message: "Expected -> or :> in rule definition".to_string(),
                    token: Some(self.peek().clone()),
                });
            };
            rules.push((lhs, rhs));
        }
        self.expect(&Token::RBrace)?;

        Ok(Expr::RuleDef { name, rules })
    }

    fn parse_match(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::Match)?;
        let expr = self.parse_expression()?;
        self.expect(&Token::LBrace)?;

        let mut branches = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            let pattern = self.parse_pattern()?;
            self.expect(&Token::FatArrow)?;
            let result = self.parse_expression()?;
            if self.at(&Token::Semicolon) {
                self.advance();
            }
            branches.push(MatchBranch { pattern, result });
        }
        self.expect(&Token::RBrace)?;

        Ok(Expr::Match { expr: Box::new(expr), branches })
    }

    fn parse_try(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::Try)?;
        self.expect(&Token::LBrace)?;
        let mut try_body = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            try_body.push(self.parse_statement()?);
            if self.at(&Token::Semicolon) {
                self.advance();
            }
        }
        self.expect(&Token::RBrace)?;

        self.expect(&Token::Catch)?;
        let err_var = self.expect_ident()?;
        self.expect(&Token::LBrace)?;
        let mut catch_body = Vec::new();
        while !self.at(&Token::RBrace) && !self.at(&Token::Eof) {
            catch_body.push(self.parse_statement()?);
            if self.at(&Token::Semicolon) {
                self.advance();
            }
        }
        self.expect(&Token::RBrace)?;

        // Simplified: store as a Call for now
        Ok(Expr::Call {
            head: Box::new(Expr::Symbol("TryCatch".to_string())),
            args: vec![
                Expr::Sequence(try_body),
                Expr::Symbol(err_var),
                Expr::Sequence(catch_body),
            ],
        })
    }

    fn parse_throw(&mut self) -> Result<Expr, ParseError> {
        self.expect(&Token::Throw)?;
        let expr = self.parse_expression()?;
        Ok(Expr::Call {
            head: Box::new(Expr::Symbol("Throw".to_string())),
            args: vec![expr],
        })
    }

    // ── Expressions (precedence climbing) ──

    pub fn parse_expression(&mut self) -> Result<Expr, ParseError> {
        self.parse_pipe_expr()
    }

    fn parse_pipe_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_at_expr()?;
        while self.at(&Token::Pipe) {
            self.advance();
            let right = self.parse_at_expr()?;
            left = Expr::Pipe {
                expr: Box::new(left),
                func: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_at_expr(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_rule_expr()?;
        if self.at(&Token::At) {
            self.advance();
            let right = self.parse_at_expr()?; // right-associative
            Ok(Expr::Prefix {
                func: Box::new(left),
                arg: Box::new(right),
            })
        } else if self.at(&Token::ApplyOp) {
            self.advance();
            let right = self.parse_at_expr()?; // right-associative
            Ok(Expr::Apply {
                func: Box::new(left),
                expr: Box::new(right),
            })
        } else {
            Ok(left)
        }
    }

    fn parse_rule_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_or_expr()?;
        loop {
            if self.at(&Token::Rule) {
                self.advance();
                let right = self.parse_rule_expr()?; // right-associative
                left = Expr::Rule {
                    lhs: Box::new(left),
                    rhs: Box::new(right),
                };
            } else if self.at(&Token::DelayedRule) {
                self.advance();
                let right = self.parse_rule_expr()?; // right-associative
                left = Expr::RuleDelayed {
                    lhs: Box::new(left),
                    rhs: Box::new(right),
                };
            } else if self.at(&Token::ReplaceAll) {
                self.advance();
                let right = self.parse_rule_expr()?; // need full rule: x_ -> 42
                left = Expr::ReplaceAll {
                    expr: Box::new(left),
                    rules: Box::new(right),
                };
            } else if self.at(&Token::ReplaceRepeated) {
                self.advance();
                let right = self.parse_rule_expr()?; // need full rule: x_ -> 42
                left = Expr::ReplaceRepeated {
                    expr: Box::new(left),
                    rules: Box::new(right),
                };
            } else {
                break;
            }
        }
        Ok(left)
    }

    fn parse_or_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_and_expr()?;
        while self.at(&Token::Or) {
            self.advance();
            let right = self.parse_and_expr()?;
            left = Expr::Call {
                head: Box::new(Expr::Symbol("Or".to_string())),
                args: vec![left, right],
            };
        }
        Ok(left)
    }

    fn parse_and_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_comp_expr()?;
        while self.at(&Token::And) {
            self.advance();
            let right = self.parse_comp_expr()?;
            left = Expr::Call {
                head: Box::new(Expr::Symbol("And".to_string())),
                args: vec![left, right],
            };
        }
        Ok(left)
    }

    fn parse_comp_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_add_expr()?;
        loop {
            let op = match self.peek() {
                Token::Equal => "Equal",
                Token::Unequal => "Unequal",
                Token::Less => "Less",
                Token::Greater => "Greater",
                Token::LessEqual => "LessEqual",
                Token::GreaterEqual => "GreaterEqual",
                Token::StringJoinOp => "StringJoin",
                _ => break,
            };
            self.advance();
            let right = self.parse_add_expr()?;
            left = Expr::Call {
                head: Box::new(Expr::Symbol(op.to_string())),
                args: vec![left, right],
            };
        }
        Ok(left)
    }

    fn parse_add_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_mul_expr()?;
        loop {
            let op = match self.peek() {
                Token::Plus => "Plus",
                Token::Minus => "Plus", // a - b = Plus[a, Times[-1, b]]
                _ => break,
            };
            let is_minus = self.peek() == &Token::Minus;
            self.advance();
            let right = self.parse_mul_expr()?;
            if is_minus {
                // a - b = Plus[a, Times[-1, b]]
                let neg = Expr::Call {
                    head: Box::new(Expr::Symbol("Times".to_string())),
                    args: vec![Expr::Integer(Integer::from(-1)), right],
                };
                left = Expr::Call {
                    head: Box::new(Expr::Symbol(op.to_string())),
                    args: vec![left, neg],
                };
            } else {
                left = Expr::Call {
                    head: Box::new(Expr::Symbol(op.to_string())),
                    args: vec![left, right],
                };
            }
        }
        Ok(left)
    }

    fn parse_mul_expr(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_pow_expr()?;
        loop {
            match self.peek() {
                Token::Star => {
                    self.advance();
                    let right = self.parse_pow_expr()?;
                    left = Expr::Call {
                        head: Box::new(Expr::Symbol("Times".to_string())),
                        args: vec![left, right],
                    };
                }
                Token::Slash => {
                    self.advance();
                    let right = self.parse_pow_expr()?;
                    left = Expr::Call {
                        head: Box::new(Expr::Symbol("Divide".to_string())),
                        args: vec![left, right],
                    };
                }
                Token::MapOp => {
                    self.advance();
                    let right = self.parse_pow_expr()?;
                    left = Expr::Map {
                        func: Box::new(left),
                        list: Box::new(right),
                    };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_pow_expr(&mut self) -> Result<Expr, ParseError> {
        let base = self.parse_unary_expr()?;
        if self.at(&Token::Caret) {
            self.advance();
            let exp = self.parse_pow_expr()?; // right-associative
            Ok(Expr::Call {
                head: Box::new(Expr::Symbol("Power".to_string())),
                args: vec![base, exp],
            })
        } else {
            Ok(base)
        }
    }

    fn parse_unary_expr(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Minus => {
                self.advance();
                let expr = self.parse_unary_expr()?;
                Ok(Expr::Call {
                    head: Box::new(Expr::Symbol("Times".to_string())),
                    args: vec![Expr::Integer(Integer::from(-1)), expr],
                })
            }
            Token::Not => {
                self.advance();
                let expr = self.parse_unary_expr()?;
                Ok(Expr::Call {
                    head: Box::new(Expr::Symbol("Not".to_string())),
                    args: vec![expr],
                })
            }
            Token::Quote => {
                self.advance();
                let expr = self.parse_unary_expr()?;
                Ok(Expr::Call {
                    head: Box::new(Expr::Symbol("Hold".to_string())),
                    args: vec![expr],
                })
            }
            Token::Tilde => {
                self.advance();
                let expr = self.parse_unary_expr()?;
                Ok(Expr::Call {
                    head: Box::new(Expr::Symbol("Splice".to_string())),
                    args: vec![expr],
                })
            }
            _ => self.parse_postfix_expr(),
        }
    }

    fn parse_postfix_expr(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_primary_expr()?;

        loop {
            match self.peek().clone() {
                // Member access: .ident
                Token::Dot => {
                    self.advance();
                    let member = self.expect_ident()?;

                    // Check if followed by [ for method call
                    if self.at(&Token::LBracket) {
                        self.advance();
                        let args = if self.at(&Token::RBracket) {
                            vec![]
                        } else {
                            self.parse_expr_list()?
                        };
                        self.expect(&Token::RBracket)?;
                        // p.method[args] = method[p, args]
                        let mut call_args = vec![expr];
                        call_args.extend(args);
                        expr = Expr::Call {
                            head: Box::new(Expr::Symbol(member)),
                            args: call_args,
                        };
                    } else {
                        // p.field = Part[p, "field"] or field[p]
                        expr = Expr::Call {
                            head: Box::new(Expr::Symbol(member)),
                            args: vec![expr],
                        };
                    }
                }

                // Function/builtin call: [args]
                Token::LBracket => {
                    self.advance();
                    let args = if self.at(&Token::RBracket) {
                        vec![]
                    } else {
                        self.parse_expr_list()?
                    };
                    self.expect(&Token::RBracket)?;
                    expr = Expr::Call {
                        head: Box::new(expr),
                        args,
                    };
                }

                // Part access: [[index]]
                Token::LDoubleBracket => {
                    self.advance();
                    let indices = self.parse_expr_list()?;
                    self.expect(&Token::RDoubleBracket)?;
                    let mut args = vec![expr];
                    args.extend(indices);
                    expr = Expr::Call {
                        head: Box::new(Expr::Symbol("Part".to_string())),
                        args,
                    };
                }

                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_primary_expr(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            // Atoms
            Token::Integer(n) => {
                self.advance();
                let val = Integer::from_str_radix(&n, 10)
                    .map_err(|_| ParseError { message: format!("Invalid integer: {}", n), token: None })?;
                Ok(Expr::Integer(val))
            }
            Token::Real(r) => {
                self.advance();
                let val = Float::parse(&r)
                    .map(|v| Float::with_val(128, v))
                    .map_err(|_| ParseError { message: format!("Invalid real: {}", r), token: None })?;
                Ok(Expr::Real(val))
            }
            Token::Str(s) => {
                self.advance();
                Ok(Expr::Str(s))
            }
            Token::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            Token::Null => {
                self.advance();
                Ok(Expr::Null)
            }
            Token::Ident(s) => {
                self.advance();
                // Check for assignment: ident = expr
                // (handled at statement level, not here)
                Ok(Expr::Symbol(s))
            }

            // Slot
            Token::Slot => {
                self.advance();
                Ok(Expr::Slot(None))
            }
            Token::SlotN(n) => {
                self.advance();
                Ok(Expr::Slot(Some(n)))
            }

            // Parenthesized expression
            Token::LParen => {
                self.advance();
                let expr = self.parse_expression()?;
                self.expect(&Token::RParen)?;
                Ok(expr)
            }

            // List literal: {a, b, c}
            Token::LBrace => {
                self.advance();
                if self.at(&Token::RBrace) {
                    self.advance();
                    return Ok(Expr::List(vec![]));
                }
                let items = self.parse_expr_list()?;
                self.expect(&Token::RBrace)?;
                Ok(Expr::List(items))
            }

            // Association: <|"key" -> val, ...|>
            Token::LAssoc => {
                self.advance();
                let mut entries = Vec::new();
                while !self.at(&Token::RAssoc) && !self.at(&Token::Eof) {
                    let key = match self.advance() {
                        Token::Str(s) => s,
                        Token::Ident(s) => s,
                        tok => {
                            return Err(ParseError {
                                message: "Expected string or ident as association key".to_string(),
                                token: Some(tok),
                            });
                        }
                    };
                    self.expect(&Token::Rule)?;
                    let val = self.parse_expression()?;
                    entries.push((key, val));
                    if self.at(&Token::Comma) {
                        self.advance();
                    }
                }
                self.expect(&Token::RAssoc)?;
                Ok(Expr::Assoc(entries))
            }

            // Match expression
            Token::Match => self.parse_match(),

            // If expression
            Token::If => {
                self.advance();
                self.expect(&Token::LBracket)?;
                let condition = self.parse_expression()?;
                self.expect(&Token::Comma)?;
                let then_branch = self.parse_expression()?;
                let else_branch = if self.at(&Token::Comma) {
                    self.advance();
                    Some(Box::new(self.parse_expression()?))
                } else {
                    None
                };
                self.expect(&Token::RBracket)?;
                Ok(Expr::If {
                    condition: Box::new(condition),
                    then_branch: Box::new(then_branch),
                    else_branch,
                })
            }

            // Which expression
            Token::Which => {
                self.advance();
                self.expect(&Token::LBracket)?;
                let mut pairs = Vec::new();
                loop {
                    let cond = self.parse_expression()?;
                    self.expect(&Token::Comma)?;
                    let val = self.parse_expression()?;
                    pairs.push((cond, val));
                    if !self.at(&Token::Comma) {
                        break;
                    }
                    self.advance();
                }
                self.expect(&Token::RBracket)?;
                Ok(Expr::Which { pairs })
            }

            // Switch expression
            Token::Switch => {
                self.advance();
                self.expect(&Token::LBracket)?;
                let expr = self.parse_expression()?;
                let mut cases = Vec::new();
                while self.at(&Token::Comma) {
                    self.advance();
                    let pattern = self.parse_expression()?;
                    self.expect(&Token::Comma)?;
                    let value = self.parse_expression()?;
                    cases.push((pattern, value));
                }
                self.expect(&Token::RBracket)?;
                Ok(Expr::Switch {
                    expr: Box::new(expr),
                    cases,
                })
            }

            // For loop
            Token::For => {
                self.advance();
                self.expect(&Token::LBracket)?;
                let init = self.parse_expression()?;
                self.expect(&Token::Comma)?;
                let condition = self.parse_expression()?;
                self.expect(&Token::Comma)?;
                let step = self.parse_expression()?;
                self.expect(&Token::Comma)?;
                let body = self.parse_expression()?;
                self.expect(&Token::RBracket)?;
                Ok(Expr::For {
                    init: Box::new(init),
                    condition: Box::new(condition),
                    step: Box::new(step),
                    body: Box::new(body),
                })
            }

            // While loop
            Token::While => {
                self.advance();
                self.expect(&Token::LBracket)?;
                let condition = self.parse_expression()?;
                self.expect(&Token::Comma)?;
                let body = self.parse_expression()?;
                self.expect(&Token::RBracket)?;
                Ok(Expr::While {
                    condition: Box::new(condition),
                    body: Box::new(body),
                })
            }

            // Do loop
            Token::Do => {
                self.advance();
                self.expect(&Token::LBracket)?;
                let body = self.parse_expression()?;
                self.expect(&Token::Comma)?;
                self.expect(&Token::LBrace)?;
                let var = self.expect_ident()?;
                self.expect(&Token::Comma)?;
                let first = self.parse_expression()?;
                let iterator = if self.at(&Token::Comma) {
                    self.advance();
                    let second = self.parse_expression()?;
                    self.expect(&Token::RBrace)?;
                    IteratorSpec::Range {
                        var,
                        min: Box::new(first),
                        max: Box::new(second),
                    }
                } else {
                    self.expect(&Token::RBrace)?;
                    IteratorSpec::List {
                        var,
                        list: Box::new(first),
                    }
                };
                self.expect(&Token::RBracket)?;
                Ok(Expr::Do {
                    body: Box::new(body),
                    iterator,
                })
            }

            // Function[{params}, body]
            Token::Function => {
                self.advance();
                self.expect(&Token::LBracket)?;

                let params = if self.at(&Token::LBrace) {
                    self.advance();
                    let mut params = vec![self.expect_ident()?];
                    while self.at(&Token::Comma) {
                        self.advance();
                        params.push(self.expect_ident()?);
                    }
                    self.expect(&Token::RBrace)?;
                    params
                } else {
                    vec![self.expect_ident()?]
                };

                self.expect(&Token::Comma)?;
                let body = self.parse_expression()?;
                self.expect(&Token::RBracket)?;

                Ok(Expr::Function {
                    params,
                    body: Box::new(body),
                })
            }

            // Hold[expr]
            Token::Hold => {
                self.advance();
                self.expect(&Token::LBracket)?;
                let expr = self.parse_expression()?;
                self.expect(&Token::RBracket)?;
                Ok(Expr::Hold(Box::new(expr)))
            }

            // HoldComplete[expr]
            Token::HoldComplete => {
                self.advance();
                self.expect(&Token::LBracket)?;
                let expr = self.parse_expression()?;
                self.expect(&Token::RBracket)?;
                Ok(Expr::HoldComplete(Box::new(expr)))
            }

            // Try expression
            Token::Try => self.parse_try(),

            // Throw expression
            Token::Throw => self.parse_throw(),

            tok => Err(ParseError {
                message: format!("Unexpected token in expression: {:?}", tok),
                token: Some(tok),
            }),
        }
    }

    // ── Pattern parsing ──

    fn parse_pattern(&mut self) -> Result<Expr, ParseError> {
        let pattern = self.parse_pattern_pipe()?;

        // Check for guard: pattern /; condition
        if self.at(&Token::ColonSlashSemicolon) {
            self.advance();
            let condition = self.parse_expression()?;
            Ok(Expr::PatternGuard {
                pattern: Box::new(pattern),
                condition: Box::new(condition),
            })
        } else {
            Ok(pattern)
        }
    }

    fn parse_pattern_pipe(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_pattern_at()?;
        while self.at(&Token::Pipe) {
            self.advance();
            let right = self.parse_pattern_at()?;
            left = Expr::Pipe {
                expr: Box::new(left),
                func: Box::new(right),
            };
        }
        Ok(left)
    }

    fn parse_pattern_at(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_pattern_rule()?;
        if self.at(&Token::At) {
            self.advance();
            let right = self.parse_pattern_at()?;
            Ok(Expr::Prefix {
                func: Box::new(left),
                arg: Box::new(right),
            })
        } else {
            Ok(left)
        }
    }

    fn parse_pattern_rule(&mut self) -> Result<Expr, ParseError> {
        let left = self.parse_pattern_or()?;
        if self.at(&Token::Rule) {
            self.advance();
            let right = self.parse_pattern_rule()?;
            Ok(Expr::Rule {
                lhs: Box::new(left),
                rhs: Box::new(right),
            })
        } else if self.at(&Token::DelayedRule) {
            self.advance();
            let right = self.parse_pattern_rule()?;
            Ok(Expr::RuleDelayed {
                lhs: Box::new(left),
                rhs: Box::new(right),
            })
        } else {
            Ok(left)
        }
    }

    fn parse_pattern_or(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_pattern_and()?;
        while self.at(&Token::Or) {
            self.advance();
            let right = self.parse_pattern_and()?;
            left = Expr::Call {
                head: Box::new(Expr::Symbol("Alternatives".to_string())),
                args: vec![left, right],
            };
        }
        Ok(left)
    }

    fn parse_pattern_and(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_pattern_comp()?;
        while self.at(&Token::And) {
            self.advance();
            let right = self.parse_pattern_comp()?;
            left = Expr::Call {
                head: Box::new(Expr::Symbol("And".to_string())),
                args: vec![left, right],
            };
        }
        Ok(left)
    }

    fn parse_pattern_comp(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_pattern_add()?;
        loop {
            let op = match self.peek() {
                Token::Equal => "Equal",
                Token::Unequal => "Unequal",
                Token::Less => "Less",
                Token::Greater => "Greater",
                Token::LessEqual => "LessEqual",
                Token::GreaterEqual => "GreaterEqual",
                Token::StringJoinOp => "StringJoin",
                _ => break,
            };
            self.advance();
            let right = self.parse_pattern_add()?;
            left = Expr::Call {
                head: Box::new(Expr::Symbol(op.to_string())),
                args: vec![left, right],
            };
        }
        Ok(left)
    }

    fn parse_pattern_add(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_pattern_mul()?;
        loop {
            let op = match self.peek() {
                Token::Plus => "Plus",
                Token::Minus => "Plus",
                _ => break,
            };
            let is_minus = self.peek() == &Token::Minus;
            self.advance();
            let right = self.parse_pattern_mul()?;
            if is_minus {
                let neg = Expr::Call {
                    head: Box::new(Expr::Symbol("Times".to_string())),
                    args: vec![Expr::Integer(Integer::from(-1)), right],
                };
                left = Expr::Call {
                    head: Box::new(Expr::Symbol(op.to_string())),
                    args: vec![left, neg],
                };
            } else {
                left = Expr::Call {
                    head: Box::new(Expr::Symbol(op.to_string())),
                    args: vec![left, right],
                };
            }
        }
        Ok(left)
    }

    fn parse_pattern_mul(&mut self) -> Result<Expr, ParseError> {
        let mut left = self.parse_pattern_pow()?;
        loop {
            match self.peek() {
                Token::Star => {
                    self.advance();
                    let right = self.parse_pattern_pow()?;
                    left = Expr::Call {
                        head: Box::new(Expr::Symbol("Times".to_string())),
                        args: vec![left, right],
                    };
                }
                Token::Slash => {
                    self.advance();
                    let right = self.parse_pattern_pow()?;
                    left = Expr::Call {
                        head: Box::new(Expr::Symbol("Divide".to_string())),
                        args: vec![left, right],
                    };
                }
                _ => break,
            }
        }
        Ok(left)
    }

    fn parse_pattern_pow(&mut self) -> Result<Expr, ParseError> {
        let base = self.parse_pattern_unary()?;
        if self.at(&Token::Caret) {
            self.advance();
            let exp = self.parse_pattern_pow()?; // right-associative
            Ok(Expr::Call {
                head: Box::new(Expr::Symbol("Power".to_string())),
                args: vec![base, exp],
            })
        } else {
            Ok(base)
        }
    }

    fn parse_pattern_unary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            Token::Minus => {
                self.advance();
                let expr = self.parse_pattern_unary()?;
                Ok(Expr::Call {
                    head: Box::new(Expr::Symbol("Times".to_string())),
                    args: vec![Expr::Integer(Integer::from(-1)), expr],
                })
            }
            _ => self.parse_pattern_postfix(),
        }
    }

    fn parse_pattern_postfix(&mut self) -> Result<Expr, ParseError> {
        let mut expr = self.parse_pattern_primary()?;

        loop {
            match self.peek().clone() {
                Token::Dot => {
                    self.advance();
                    let member = self.expect_ident()?;
                    if self.at(&Token::LBracket) {
                        self.advance();
                        let args = if self.at(&Token::RBracket) {
                            vec![]
                        } else {
                            self.parse_pattern_list()?
                        };
                        self.expect(&Token::RBracket)?;
                        let mut call_args = vec![expr];
                        call_args.extend(args);
                        expr = Expr::Call {
                            head: Box::new(Expr::Symbol(member)),
                            args: call_args,
                        };
                    } else {
                        expr = Expr::Call {
                            head: Box::new(Expr::Symbol(member)),
                            args: vec![expr],
                        };
                    }
                }
                Token::LBracket => {
                    self.advance();
                    let args = if self.at(&Token::RBracket) {
                        vec![]
                    } else {
                        self.parse_pattern_list()?
                    };
                    self.expect(&Token::RBracket)?;
                    expr = Expr::Call {
                        head: Box::new(expr),
                        args,
                    };
                }
                Token::LDoubleBracket => {
                    self.advance();
                    let indices = self.parse_pattern_list()?;
                    self.expect(&Token::RDoubleBracket)?;
                    let mut args = vec![expr];
                    args.extend(indices);
                    expr = Expr::Call {
                        head: Box::new(Expr::Symbol("Part".to_string())),
                        args,
                    };
                }
                _ => break,
            }
        }

        Ok(expr)
    }

    fn parse_pattern_primary(&mut self) -> Result<Expr, ParseError> {
        match self.peek().clone() {
            // Blank: _
            Token::Ident(s) if s == "_" => {
                self.advance();
                let type_constraint = self.try_parse_type_suffix()?;
                Ok(Expr::Blank { type_constraint })
            }

            // BlankSequence: __
            Token::Ident(s) if s == "__" => {
                self.advance();
                let type_constraint = self.try_parse_type_suffix()?;
                Ok(Expr::BlankSequence {
                    name: None,
                    type_constraint,
                })
            }

            // BlankNullSequence: ___
            Token::Ident(s) if s == "___" => {
                self.advance();
                let type_constraint = self.try_parse_type_suffix()?;
                Ok(Expr::BlankNullSequence {
                    name: None,
                    type_constraint,
                })
            }

            // Named blank: ident_ or ident__ or ident___
            Token::Ident(name) => {
                self.advance();

                // Check if next token starts with _
                // This is tricky because _ is part of the identifier in our lexer
                // For now, check if the identifier ends with _
                if name.ends_with('_') {
                    let base = &name[..name.len() - 1];
                    let type_constraint = self.try_parse_type_suffix()?;
                    if base.is_empty() {
                        Ok(Expr::Blank { type_constraint })
                    } else {
                        Ok(Expr::NamedBlank {
                            name: base.to_string(),
                            type_constraint,
                        })
                    }
                } else if name.ends_with("__") && name.len() > 2 {
                    let base = &name[..name.len() - 2];
                    let type_constraint = self.try_parse_type_suffix()?;
                    Ok(Expr::BlankSequence {
                        name: Some(base.to_string()),
                        type_constraint,
                    })
                } else if name.ends_with("___") && name.len() > 3 {
                    let base = &name[..name.len() - 3];
                    let type_constraint = self.try_parse_type_suffix()?;
                    Ok(Expr::BlankNullSequence {
                        name: Some(base.to_string()),
                        type_constraint,
                    })
                } else {
                    Ok(Expr::Symbol(name))
                }
            }

            // Literals
            Token::Integer(n) => {
                self.advance();
                let val = Integer::from_str_radix(&n, 10)
                    .map_err(|_| ParseError { message: format!("Invalid integer: {}", n), token: None })?;
                Ok(Expr::Integer(val))
            }
            Token::Real(r) => {
                self.advance();
                let val = Float::parse(&r)
                    .map(|v| Float::with_val(128, v))
                    .map_err(|_| ParseError { message: format!("Invalid real: {}", r), token: None })?;
                Ok(Expr::Real(val))
            }
            Token::Str(s) => {
                self.advance();
                Ok(Expr::Str(s))
            }
            Token::True => {
                self.advance();
                Ok(Expr::Bool(true))
            }
            Token::False => {
                self.advance();
                Ok(Expr::Bool(false))
            }
            Token::Null => {
                self.advance();
                Ok(Expr::Null)
            }

            // Parenthesized pattern (alternatives)
            Token::LParen => {
                self.advance();
                let mut patterns = vec![self.parse_pattern()?];
                while self.at(&Token::Ident("|".to_string())) {
                    self.advance();
                    patterns.push(self.parse_pattern()?);
                }
                self.expect(&Token::RParen)?;

                if patterns.len() == 1 {
                    Ok(patterns.into_iter().next().unwrap())
                } else {
                    Ok(Expr::Call {
                        head: Box::new(Expr::Symbol("Alternatives".to_string())),
                        args: patterns,
                    })
                }
            }

            // List pattern: {pat1, pat2, ...}
            Token::LBrace => {
                self.advance();
                if self.at(&Token::RBrace) {
                    self.advance();
                    return Ok(Expr::List(vec![]));
                }
                let patterns = self.parse_pattern_list()?;
                self.expect(&Token::RBrace)?;
                Ok(Expr::List(patterns))
            }

            tok => Err(ParseError {
                message: "Unexpected token in pattern".to_string(),
                token: Some(tok),
            }),
        }
    }

    fn try_parse_type_suffix(&mut self) -> Result<Option<Symbol>, ParseError> {
        // After a blank (_, __, ___), check if the next token is a type name
        // In our current lexer, _Integer is one token. So this is handled in parse_pattern_primary.
        // For now, return None.
        Ok(None)
    }

    // ── Helpers ──

    fn parse_expr_list(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut items = vec![self.parse_expression()?];
        while self.at(&Token::Comma) {
            self.advance();
            items.push(self.parse_expression()?);
        }
        Ok(items)
    }

    fn parse_pattern_list(&mut self) -> Result<Vec<Expr>, ParseError> {
        let mut items = vec![self.parse_pattern()?];
        while self.at(&Token::Comma) {
            self.advance();
            items.push(self.parse_pattern()?);
        }
        Ok(items)
    }

    /// Convert a symbol that looks like a pattern (e.g. `x_`, `x_Integer`, `_`)
    /// into the appropriate pattern AST node.
    fn convert_pattern(expr: Expr) -> Expr {
        match expr {
            Expr::Symbol(ref s) if s.contains('_') => {
                if s == "_" {
                    Expr::Blank { type_constraint: None }
                } else if let Some(pos) = s.find('_') {
                    let name = &s[..pos];
                    let rest = &s[pos + 1..];
                    if rest.is_empty() {
                        // x_ → NamedBlank { name: "x", type_constraint: None }
                        Expr::NamedBlank {
                            name: name.to_string(),
                            type_constraint: None,
                        }
                    } else {
                        // x_Integer → NamedBlank { name: "x", type_constraint: Some("Integer") }
                        Expr::NamedBlank {
                            name: name.to_string(),
                            type_constraint: Some(rest.to_string()),
                        }
                    }
                } else {
                    expr
                }
            }
            _ => expr,
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.advance() {
            Token::Ident(s) => Ok(s),
            tok => Err(ParseError {
                message: format!("Expected identifier, found '{}'", tok),
                token: Some(tok),
            }),
        }
    }
}

/// Convenience function to parse tokens into an AST.
pub fn parse(tokens: Vec<Token>) -> Result<Vec<Expr>, ParseError> {
    Parser::new(tokens).parse_program()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::tokenize;

    fn parse_str(input: &str) -> Vec<Expr> {
        let tokens = tokenize(input).unwrap();
        parse(tokens).unwrap()
    }

    fn parse_one(input: &str) -> Expr {
        let stmts = parse_str(input);
        assert_eq!(stmts.len(), 1, "Expected 1 statement, got {}", stmts.len());
        stmts.into_iter().next().unwrap()
    }

    #[test]
    fn test_integer_literal() {
        assert_eq!(parse_one("42"), Expr::Integer(Integer::from(42)));
    }

    #[test]
    fn test_real_literal() {
        let expected = Float::parse("3.14").map(|v| Float::with_val(128, v)).unwrap();
        assert_eq!(parse_one("3.14"), Expr::Real(expected));
    }

    #[test]
    fn test_string_literal() {
        assert_eq!(parse_one(r#""hello""#), Expr::Str("hello".to_string()));
    }

    #[test]
    fn test_bool_literals() {
        assert_eq!(parse_one("True"), Expr::Bool(true));
        assert_eq!(parse_one("False"), Expr::Bool(false));
    }

    #[test]
    fn test_null_literal() {
        assert_eq!(parse_one("Null"), Expr::Null);
    }

    #[test]
    fn test_symbol() {
        assert_eq!(parse_one("x"), Expr::Symbol("x".to_string()));
    }

    #[test]
    fn test_addition() {
        let expr = parse_one("1 + 2");
        match expr {
            Expr::Call { head, args } => {
                assert_eq!(*head, Expr::Symbol("Plus".to_string()));
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], Expr::Integer(Integer::from(1)));
                assert_eq!(args[1], Expr::Integer(Integer::from(2)));
            }
            _ => panic!("Expected Call, got {:?}", expr),
        }
    }

    #[test]
    fn test_subtraction() {
        let expr = parse_one("5 - 3");
        match expr {
            Expr::Call { head, args } => {
                assert_eq!(*head, Expr::Symbol("Plus".to_string()));
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], Expr::Integer(Integer::from(5)));
                match &args[1] {
                    Expr::Call { head, args } => {
                        assert_eq!(**head, Expr::Symbol("Times".to_string()));
                        assert_eq!(args[0], Expr::Integer(Integer::from(-1)));
                        assert_eq!(args[1], Expr::Integer(Integer::from(3)));
                    }
                    _ => panic!("Expected Times call"),
                }
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_multiplication() {
        let expr = parse_one("2 * 3");
        match expr {
            Expr::Call { head, args } => {
                assert_eq!(*head, Expr::Symbol("Times".to_string()));
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_division() {
        let expr = parse_one("6 / 2");
        match expr {
            Expr::Call { head, args } => {
                assert_eq!(*head, Expr::Symbol("Divide".to_string()));
                assert_eq!(args.len(), 2);
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_power() {
        let expr = parse_one("2^3");
        match expr {
            Expr::Call { head, args } => {
                assert_eq!(*head, Expr::Symbol("Power".to_string()));
                assert_eq!(args[0], Expr::Integer(Integer::from(2)));
                assert_eq!(args[1], Expr::Integer(Integer::from(3)));
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_power_right_associative() {
        let expr = parse_one("2^3^4");
        match expr {
            Expr::Call { head, args } => {
                assert_eq!(*head, Expr::Symbol("Power".to_string()));
                assert_eq!(args[0], Expr::Integer(Integer::from(2)));
                match &args[1] {
                    Expr::Call { head, args } => {
                        assert_eq!(**head, Expr::Symbol("Power".to_string()));
                        assert_eq!(args[0], Expr::Integer(Integer::from(3)));
                        assert_eq!(args[1], Expr::Integer(Integer::from(4)));
                    }
                    _ => panic!("Expected nested Power"),
                }
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_comparison() {
        let expr = parse_one("1 == 2");
        match expr {
            Expr::Call { head, .. } => {
                assert_eq!(*head, Expr::Symbol("Equal".to_string()));
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_less_than() {
        let expr = parse_one("x < y");
        match expr {
            Expr::Call { head, .. } => {
                assert_eq!(*head, Expr::Symbol("Less".to_string()));
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_logical_and() {
        let expr = parse_one("True && False");
        match expr {
            Expr::Call { head, .. } => {
                assert_eq!(*head, Expr::Symbol("And".to_string()));
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_logical_or() {
        let expr = parse_one("True || False");
        match expr {
            Expr::Call { head, .. } => {
                assert_eq!(*head, Expr::Symbol("Or".to_string()));
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_not() {
        let expr = parse_one("!True");
        match expr {
            Expr::Call { head, .. } => {
                assert_eq!(*head, Expr::Symbol("Not".to_string()));
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_function_call() {
        let expr = parse_one("f[1, 2]");
        match expr {
            Expr::Call { head, args } => {
                assert_eq!(*head, Expr::Symbol("f".to_string()));
                assert_eq!(args.len(), 2);
                assert_eq!(args[0], Expr::Integer(Integer::from(1)));
                assert_eq!(args[1], Expr::Integer(Integer::from(2)));
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_list_literal() {
        let expr = parse_one("{1, 2, 3}");
        match expr {
            Expr::List(items) => {
                assert_eq!(items.len(), 3);
                assert_eq!(items[0], Expr::Integer(Integer::from(1)));
                assert_eq!(items[1], Expr::Integer(Integer::from(2)));
                assert_eq!(items[2], Expr::Integer(Integer::from(3)));
            }
            _ => panic!("Expected List"),
        }
    }

    #[test]
    fn test_empty_list() {
        let expr = parse_one("{}");
        assert_eq!(expr, Expr::List(vec![]));
    }

    #[test]
    fn test_assignment() {
        let expr = parse_one("x = 5");
        match expr {
            Expr::Assign { lhs, rhs } => {
                assert_eq!(*lhs, Expr::Symbol("x".to_string()));
                assert_eq!(*rhs, Expr::Integer(Integer::from(5)));
            }
            _ => panic!("Expected Assign"),
        }
    }

    #[test]
    fn test_function_definition() {
        let expr = parse_one("f[x_] := x^2");
        match expr {
            Expr::FuncDef { name, params, body, delayed } => {
                assert_eq!(name, "f");
                assert_eq!(params.len(), 1);
                assert!(delayed);
                match *body {
                    Expr::Call { head, .. } => {
                        assert_eq!(*head, Expr::Symbol("Power".to_string()));
                    }
                    _ => panic!("Expected Call in body"),
                }
            }
            _ => panic!("Expected FuncDef"),
        }
    }

    #[test]
    fn test_if_expression() {
        let expr = parse_one("If[True, 1, 2]");
        match expr {
            Expr::If { condition, then_branch, else_branch } => {
                assert_eq!(*condition, Expr::Bool(true));
                assert_eq!(*then_branch, Expr::Integer(Integer::from(1)));
                assert_eq!(*else_branch.unwrap(), Expr::Integer(Integer::from(2)));
            }
            _ => panic!("Expected If"),
        }
    }

    #[test]
    fn test_rule() {
        let expr = parse_one("a -> b");
        match expr {
            Expr::Rule { lhs, rhs } => {
                assert_eq!(*lhs, Expr::Symbol("a".to_string()));
                assert_eq!(*rhs, Expr::Symbol("b".to_string()));
            }
            _ => panic!("Expected Rule"),
        }
    }

    #[test]
    fn test_delayed_rule() {
        let expr = parse_one("a :> b");
        match expr {
            Expr::RuleDelayed { lhs, rhs } => {
                assert_eq!(*lhs, Expr::Symbol("a".to_string()));
                assert_eq!(*rhs, Expr::Symbol("b".to_string()));
            }
            _ => panic!("Expected RuleDelayed"),
        }
    }

    #[test]
    fn test_pipe() {
        let expr = parse_one("x // f");
        match expr {
            Expr::Pipe { expr, func } => {
                assert_eq!(*expr, Expr::Symbol("x".to_string()));
                assert_eq!(*func, Expr::Symbol("f".to_string()));
            }
            _ => panic!("Expected Pipe"),
        }
    }

    #[test]
    fn test_prefix() {
        let expr = parse_one("f @ x");
        match expr {
            Expr::Prefix { func, arg } => {
                assert_eq!(*func, Expr::Symbol("f".to_string()));
                assert_eq!(*arg, Expr::Symbol("x".to_string()));
            }
            _ => panic!("Expected Prefix"),
        }
    }

    #[test]
    fn test_sequence() {
        let stmts = parse_str("a; b; c");
        assert_eq!(stmts.len(), 3);
        assert_eq!(stmts[0], Expr::Symbol("a".to_string()));
        assert_eq!(stmts[1], Expr::Symbol("b".to_string()));
        assert_eq!(stmts[2], Expr::Symbol("c".to_string()));
    }

    #[test]
    fn test_parenthesized() {
        let expr = parse_one("(1 + 2)");
        match expr {
            Expr::Call { head, .. } => {
                assert_eq!(*head, Expr::Symbol("Plus".to_string()));
            }
            _ => panic!("Expected Call"),
        }
    }

    #[test]
    fn test_unary_minus() {
        let expr = parse_one("-x");
        match expr {
            Expr::Call { head, args } => {
                assert_eq!(*head, Expr::Symbol("Times".to_string()));
                assert_eq!(args[0], Expr::Integer(Integer::from(-1)));
                assert_eq!(args[1], Expr::Symbol("x".to_string()));
            }
            _ => panic!("Expected Times call"),
        }
    }

    #[test]
    fn test_hold() {
        let expr = parse_one("Hold[x]");
        match expr {
            Expr::Hold(e) => {
                assert_eq!(*e, Expr::Symbol("x".to_string()));
            }
            _ => panic!("Expected Hold"),
        }
    }

    #[test]
    fn test_parse_error_unexpected_token() {
        let tokens = vec![Token::RBracket, Token::Eof];
        let result = parse(tokens);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_input() {
        let stmts = parse_str("");
        assert_eq!(stmts.len(), 0);
    }
}
