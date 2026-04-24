//! Wolfram Notebook (.nb) parser.
//!
//! Parses the Wolfram Notebook expression format (not the raw XML or binary form,
//! but the textual WL-expression form saved by Mathematica's "Save as...".
//!
//! Extracts executable Wolfram Language code from `Cell[BoxData[...], "Input"]`
//! cells by converting box-language expressions back to source-code text.
//!
//! # Limitations
//!
//! - Only handles Input cells with BoxData (the standard form). Plain-text
//!   Input cells (`Cell["code", "Input"]`) are also supported.
//! - Box-language → code conversion handles the common forms: `RowBox`,
//!   `SuperscriptBox`, `FractionBox`, `SqrtBox`, `StyleBox`, `TagBox`,
//!   `InterpretationBox`, and `FormBox`.
//! - Nested `Cell[TextData[...], "None"]` inside box expressions is handled.
//! - Does **not** handle Initialization cells or auto-generated notebook
//!   metadata (those are not "Input" cells).

// ── WL expression AST ───────────────────────────────────────────────────────

/// A minimal WL expression node for .nb file parsing.
#[derive(Debug, Clone)]
enum WlNode {
    /// A string literal.
    Str(String),
    /// A symbol (identifier).
    Sym(String),
    /// A numeric literal (kept as string to preserve precision).
    Num(String),
    /// `head[arg1, arg2, ...]`
    Call {
        head: Box<WlNode>,
        args: Vec<WlNode>,
    },
    /// `{item1, item2, ...}`
    List(Vec<WlNode>),
    /// `lhs -> rhs` (rule / option spec)
    Rule {
        lhs: Box<WlNode>,
        rhs: Box<WlNode>,
    },
}

fn is_sym(node: &WlNode, name: &str) -> bool {
    matches!(node, WlNode::Sym(s) if s == name)
}

fn sym_name(node: &WlNode) -> Option<&str> {
    match node {
        WlNode::Sym(s) => Some(s.as_str()),
        _ => None,
    }
}

// ── WL expression parser ────────────────────────────────────────────────────

/// Minimal recursive-descent parser for the WL expression subset used in .nb
/// files.
struct WlParser<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> WlParser<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            src: input.as_bytes(),
            pos: 0,
        }
    }

    /// Peek at the current byte without advancing.
    fn peek(&self) -> Option<u8> {
        self.src.get(self.pos).copied()
    }

    /// Peek ahead by `n` bytes.
    fn peek_n(&self, n: usize) -> Option<u8> {
        self.src.get(self.pos + n).copied()
    }

    /// Advance one byte and return it.
    fn advance(&mut self) -> Option<u8> {
        let b = self.src.get(self.pos).copied();
        if b.is_some() {
            self.pos += 1;
        }
        b
    }

    /// Skip whitespace and comments.
    fn skip_trivia(&mut self) {
        loop {
            self.skip_whitespace();
            if self.peek() == Some(b'(') && self.peek_n(1) == Some(b'*') {
                self.skip_comment();
            } else {
                break;
            }
        }
    }

    fn skip_whitespace(&mut self) {
        while let Some(b) = self.peek() {
            if b.is_ascii_whitespace() || b == 0x0a || b == 0x0d || b == 0x09 {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Skip a `(* ... *)` comment with nesting support.
    fn skip_comment(&mut self) {
        // we already saw '(' so we start at the '*'
        self.advance(); // skip '*'
        let mut depth: u32 = 1;
        while depth > 0 {
            match self.advance() {
                None => break,
                Some(b'(') if self.peek() == Some(b'*') => {
                    depth += 1;
                    self.advance();
                }
                Some(b'*') if self.peek() == Some(b')') => {
                    depth -= 1;
                    self.advance();
                }
                _ => {}
            }
        }
    }

    /// Parse a complete WL expression.
    fn parse_expr(&mut self) -> Result<WlNode, String> {
        let mut node = self.parse_primary()?;
        // Handle -> (rule) and :> (delayed rule) — can follow any expression.
        self.skip_trivia();
        let is_rule = self.peek() == Some(b'-') && self.peek_n(1) == Some(b'>')
            || self.peek() == Some(b':') && self.peek_n(1) == Some(b'>');
        if is_rule {
            self.advance();
            self.advance();
            let rhs = self.parse_expr()?;
            node = WlNode::Rule {
                lhs: Box::new(node),
                rhs: Box::new(rhs),
            };
        }
        Ok(node)
    }

    /// Parse a primary expression (no trailing rule operator).
    fn parse_primary(&mut self) -> Result<WlNode, String> {
        self.skip_trivia();
        match self.peek() {
            Some(b'"') => self.parse_string_literal(),
            Some(b'{') => self.parse_list(),
            Some(b'[') => Err("Unexpected '['".to_string()),
            Some(b'<') => {
                // Could be <| (association) or a symbol starting with <
                if self.peek_n(1) == Some(b'|') {
                    self.advance(); // <
                    self.advance(); // |
                    let mut items = Vec::new();
                    loop {
                        self.skip_trivia();
                        if self.peek() == Some(b'|') && self.peek_n(1) == Some(b'>') {
                            self.advance();
                            self.advance();
                            break;
                        }
                        if !items.is_empty() && self.peek() == Some(b',') {
                            self.advance();
                        }
                        self.skip_trivia();
                        if self.peek() == Some(b'|') && self.peek_n(1) == Some(b'>') {
                            self.advance();
                            self.advance();
                            break;
                        }
                        items.push(self.parse_expr()?);
                    }
                    Ok(WlNode::List(items))
                } else {
                    self.parse_symbol_or_call()
                }
            }
            Some(c)
                if c.is_ascii_digit()
                    || c == b'-' && self.peek_n(1).is_some_and(|n| n.is_ascii_digit()) =>
            {
                self.parse_number()
            }
            Some(b')') => Err("Unexpected ')'".to_string()),
            Some(b'}') => Err("Unexpected '}'".to_string()),
            Some(b']') => Err("Unexpected ']'".to_string()),
            Some(b',') => Err("Unexpected ','".to_string()),
            Some(c) if is_ident_start(c) => self.parse_symbol_or_call(),
            None => Err("Unexpected end of input".to_string()),
            Some(b) => Err(format!(
                "Unexpected byte 0x{:02x} ('{}') at position {}",
                b, b as char, self.pos
            )),
        }
    }

    /// Parse a string literal `"..."` with WL escape sequences.
    fn parse_string_literal(&mut self) -> Result<WlNode, String> {
        assert_eq!(self.advance(), Some(b'"'));
        let mut s = String::new();
        loop {
            match self.advance() {
                None => return Err("Unterminated string literal".to_string()),
                Some(b'"') => break,
                Some(b'\\') => {
                    match self.advance() {
                        None => return Err("Unterminated escape sequence".to_string()),
                        Some(b'"') => s.push('"'),
                        Some(b'n') => s.push('\n'),
                        Some(b't') => s.push('\t'),
                        Some(b'\\') => s.push('\\'),
                        Some(b'r') => s.push('\r'),
                        Some(c) => {
                            // WL allows \[Name] escape sequences (like \[Alpha]), but
                            // these rarely appear in .nb file strings. Just pass through.
                            s.push('\\');
                            s.push(c as char);
                        }
                    }
                }
                Some(c) => s.push(c as char),
            }
        }
        Ok(WlNode::Str(s))
    }

    /// Parse a numeric literal (integer or real, including `*^` notation and
    /// WL backtick precision markers like `` 3.14` `` or `` 3.14`30 ``).
    fn parse_number(&mut self) -> Result<WlNode, String> {
        let start = self.pos;
        // Optional leading minus
        if self.peek() == Some(b'-') {
            self.advance();
        }
        // Integer part
        while self.peek().is_some_and(|c| c.is_ascii_digit()) {
            self.advance();
        }
        // Fractional part
        if self.peek() == Some(b'.') {
            self.advance();
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
            }
        }
        // WL scientific notation: *^ (e.g., "1.5*^3")
        if self.peek() == Some(b'*') && self.peek_n(1) == Some(b'^') {
            self.advance(); // *
            self.advance(); // ^
            if self.peek() == Some(b'-') || self.peek() == Some(b'+') {
                self.advance();
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
            }
        }
        // Regular E-notation (also check)
        if self.peek() == Some(b'e') || self.peek() == Some(b'E') {
            self.advance();
            if self.peek() == Some(b'+') || self.peek() == Some(b'-') {
                self.advance();
            }
            while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                self.advance();
            }
        }
        // WL backtick precision mark: "3.14`" or "3.14`30"
        if self.peek() == Some(b'`') {
            self.advance(); // skip backtick
            // Optional precision digits (or *^ after the backtick)
            if self.peek() == Some(b'*') && self.peek_n(1) == Some(b'^') {
                // Handle `*^ (backtick followed by exponent)
                // Already consumed the backtick
                self.advance(); // *
                self.advance(); // ^
                if self.peek() == Some(b'-') || self.peek() == Some(b'+') {
                    self.advance();
                }
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.advance();
                }
            } else {
                // Just precision digits (or nothing = machine precision)
                while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                    self.advance();
                }
                // Some numbers have trailing . after precision: "3.14`30."
                if self.peek() == Some(b'.') {
                    self.advance();
                }
                // And optional *^ after that
                if self.peek() == Some(b'*') && self.peek_n(1) == Some(b'^') {
                    self.advance(); // *
                    self.advance(); // ^
                    if self.peek() == Some(b'-') || self.peek() == Some(b'+') {
                        self.advance();
                    }
                    while self.peek().is_some_and(|c| c.is_ascii_digit()) {
                        self.advance();
                    }
                }
            }
        }
        let num_str = std::str::from_utf8(&self.src[start..self.pos])
            .map_err(|e| format!("Invalid UTF-8 in number: {}", e))?;
        Ok(WlNode::Num(num_str.to_string()))
    }

    /// Parse a symbol or a function call `head[args...]`.
    fn parse_symbol_or_call(&mut self) -> Result<WlNode, String> {
        let name = self.parse_ident()?;
        // Check for function call: head[...]
        self.skip_trivia();
        if self.peek() == Some(b'[') {
            self.advance(); // skip '['
            let mut args = Vec::new();
            loop {
                self.skip_trivia();
                if self.peek() == Some(b']') {
                    self.advance();
                    break;
                }
                if !args.is_empty() && self.peek() == Some(b',') {
                    self.advance();
                }
                self.skip_trivia();
                if self.peek() == Some(b']') {
                    self.advance();
                    break;
                }
                args.push(self.parse_expr()?);
            }
            Ok(WlNode::Call {
                head: Box::new(WlNode::Sym(name)),
                args,
            })
        } else {
            Ok(WlNode::Sym(name))
        }
    }

    /// Parse a WL identifier (symbol name).
    fn parse_ident(&mut self) -> Result<String, String> {
        self.skip_trivia();
        let start = self.pos;
        while let Some(b) = self.peek() {
            if is_ident_continue(b) {
                self.advance();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(format!("Expected identifier at byte {}", self.pos));
        }
        Ok(std::str::from_utf8(&self.src[start..self.pos])
            .map_err(|e| format!("Invalid UTF-8 in identifier: {}", e))?
            .to_string())
    }

    /// Parse a list literal `{item1, item2, ...}`.
    fn parse_list(&mut self) -> Result<WlNode, String> {
        assert_eq!(self.advance(), Some(b'{'));
        let mut items = Vec::new();
        loop {
            self.skip_trivia();
            if self.peek() == Some(b'}') {
                self.advance();
                break;
            }
            if !items.is_empty() {
                if self.peek() == Some(b',') {
                    self.advance();
                }
                self.skip_trivia();
                if self.peek() == Some(b'}') {
                    self.advance();
                    break;
                }
            }
            items.push(self.parse_expr()?);
        }
        Ok(WlNode::List(items))
    }

    /// Parse all top-level expressions and return the first `Notebook[{...}]`
    /// found. This is the expected entry-point for .nb content.
    fn parse_notebook(&mut self) -> Result<WlNode, String> {
        loop {
            self.skip_trivia();
            if self.peek().is_none() {
                return Err("No Notebook expression found in file".to_string());
            }
            let expr = self.parse_expr()?;
            // Look for Notebook[{...}]
            if let WlNode::Call { head, .. } = &expr
                && is_sym(head, "Notebook")
            {
                return Ok(expr);
            }
            // Skip non-Notebook expressions (metadata, etc.)
        }
    }
}

fn is_ident_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'$' || b == b'`'
}

fn is_ident_continue(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$' || b == b'`' || b == b'%'
}

// ── Box-to-code conversion ──────────────────────────────────────────────────

/// Convert a box-language expression tree back to WL source code.
fn box_to_code(node: &WlNode) -> String {
    match node {
        WlNode::Str(s) => s.clone(),
        WlNode::Sym(s) => s.clone(),
        WlNode::Num(s) => s.clone(),
        WlNode::Call { head, args } => {
            let h = sym_name(head).unwrap_or("");
            match h {
                // RowBox / Row: concatenate all items
                "RowBox" | "Row" => {
                    if args.len() == 1 {
                        if let WlNode::List(items) = &args[0] {
                            return items.iter().map(box_to_code).collect();
                        }
                        // Fallback: convert the single arg
                        return box_to_code(&args[0]);
                    }
                    // If it doesn't match expected pattern, fall through
                    generic_call(head, args)
                }

                // Superscript: base^exp
                "SuperscriptBox" => {
                    if args.len() >= 2 {
                        let base = box_to_code(&args[0]);
                        let exp = box_to_code(&args[1]);
                        let exp = if needs_parens_for_power(&args[1]) {
                            format!("({})", exp)
                        } else {
                            exp
                        };
                        format!("{}^{}", base, exp)
                    } else {
                        generic_call(head, args)
                    }
                }

                // Subscript: Subscript[base, sub]
                "SubscriptBox" => {
                    if args.len() >= 2 {
                        let base = box_to_code(&args[0]);
                        let sub = box_to_code(&args[1]);
                        format!("Subscript[{}, {}]", base, sub)
                    } else {
                        generic_call(head, args)
                    }
                }

                // Subsuperscript: Subscript[base, sub]^sup
                "SubsuperscriptBox" => {
                    if args.len() >= 3 {
                        let base = box_to_code(&args[0]);
                        let sub = box_to_code(&args[1]);
                        let sup = box_to_code(&args[2]);
                        format!("Subscript[{}, {}]^{}", base, sub, sup)
                    } else {
                        generic_call(head, args)
                    }
                }

                // Fraction: num/den
                "FractionBox" => {
                    if args.len() >= 2 {
                        let num = box_to_code(&args[0]);
                        let den = box_to_code(&args[1]);
                        format!("({})/({})", num, den)
                    } else {
                        generic_call(head, args)
                    }
                }

                // Sqrt: Sqrt[arg]
                "SqrtBox" => {
                    if !args.is_empty() {
                        format!("Sqrt[{}]", box_to_code(&args[0]))
                    } else {
                        generic_call(head, args)
                    }
                }

                // RadicalBox: (not standard in WL, attempt Power)
                "RadicalBox" => {
                    if args.len() >= 2 {
                        let base = box_to_code(&args[0]);
                        let n = box_to_code(&args[1]);
                        format!("({})^(1/({}))", base, n)
                    } else {
                        generic_call(head, args)
                    }
                }

                // StyleBox: unwrap content, skip style options
                "StyleBox" => {
                    if args.is_empty() {
                        String::new()
                    } else {
                        box_to_code(&args[0])
                    }
                }

                // FormBox: unwrap content, skip form spec
                "FormBox" => {
                    if args.is_empty() {
                        String::new()
                    } else {
                        box_to_code(&args[0])
                    }
                }

                // TagBox: unwrap content, skip tag
                "TagBox" => {
                    if args.is_empty() {
                        String::new()
                    } else {
                        box_to_code(&args[0])
                    }
                }

                // AdjustmentBox: unwrap content
                "AdjustmentBox" => {
                    if args.is_empty() {
                        String::new()
                    } else {
                        box_to_code(&args[0])
                    }
                }

                // InterpretationBox[display, code, ...] — use the code (2nd arg)
                "InterpretationBox" => {
                    if args.len() >= 2 {
                        box_to_code(&args[1])
                    } else if args.is_empty() {
                        String::new()
                    } else {
                        box_to_code(&args[0])
                    }
                }

                // Graphics boxes — skip (not code)
                "GraphicsBox" | "Graphics3DBox" | "GraphicsComplexBox" |
                "GraphicsGroupBox" | "PointBox" | "LineBox" | "PolygonBox" => {
                    String::new()
                }

                // DynamicBox — skip (dynamic frontend content)
                "DynamicBox" | "DynamicModuleBox" | "DynamicWrapperBox" => {
                    String::new()
                }

                // ButtonBox — skip (frontend interaction, not code)
                "ButtonBox" => String::new(),

                // BoxData — unwrap content (this is the standard wrapper for
                // cell content in notebooks)
                "BoxData" => {
                    if args.is_empty() {
                        String::new()
                    } else {
                        box_to_code(&args[0])
                    }
                }

                // TextData — concatenate contained items
                "TextData" => {
                    if args.len() == 1 {
                        if let WlNode::List(items) = &args[0] {
                            return items.iter().map(box_to_code).collect();
                        }
                        box_to_code(&args[0])
                    } else {
                        args.iter().map(box_to_code).collect()
                    }
                }

                // Inline Cell[TextData[...], "None"] — unwrap to text
                "Cell" => {
                    if !args.is_empty() {
                        // If it's a Cell[TextData[...], label], extract text
                        box_to_code(&args[0])
                    } else {
                        String::new()
                    }
                }

                // OverscriptBox / UnderscriptBox
                "OverscriptBox" => {
                    if args.len() >= 2 {
                        let base = box_to_code(&args[0]);
                        let script = box_to_code(&args[1]);
                        format!("Overscript[{}, {}]", base, script)
                    } else {
                        generic_call(head, args)
                    }
                }
                "UnderscriptBox" => {
                    if args.len() >= 2 {
                        let base = box_to_code(&args[0]);
                        let script = box_to_code(&args[1]);
                        format!("Underscript[{}, {}]", base, script)
                    } else {
                        generic_call(head, args)
                    }
                }

                // General fallback for known-but-unsupported box types:
                // Just pass through as-is or return empty
                _ => generic_call(head, args),
            }
        }
        WlNode::List(items) => items.iter().map(box_to_code).collect(),
        WlNode::Rule { lhs, rhs } => {
            format!("{} -> {}", box_to_code(lhs), box_to_code(rhs))
        }
    }
}

/// Default rendering for an unrecognised call: `head[arg1, arg2, ...]`.
fn generic_call(head: &WlNode, args: &[WlNode]) -> String {
    let head_str = box_to_code(head);
    if args.is_empty() {
        format!("{}[]", head_str)
    } else {
        let args_str: Vec<String> = args.iter().map(box_to_code).collect();
        format!("{}[{}]", head_str, args_str.join(", "))
    }
}

/// Check if an expression needs parens when used as a power exponent.
fn needs_parens_for_power(node: &WlNode) -> bool {
    matches!(node, WlNode::Call { .. } | WlNode::List(_) | WlNode::Rule { .. })
}

// ── Public API ──────────────────────────────────────────────────────────────

/// Extract executable WL code from a `.nb` notebook file.
///
/// Returns a string of Wolfram Language code suitable for evaluation,
/// or an error if the file can't be parsed.
pub fn notebook_to_code(contents: &str) -> Result<String, String> {
    let mut parser = WlParser::new(contents);
    let notebook = parser.parse_notebook()?;

    // Walk the Notebook[...] expression to find Input cells
    if let WlNode::Call { head, args } = &notebook
        && is_sym(head, "Notebook") && !args.is_empty()
    {
        // The first argument is the cell list; subsequent args are
        // notebook-level options (PageFooters, PrintingOptions, etc.).
        if let WlNode::List(cells) = &args[0] {
            let mut code_parts: Vec<String> = Vec::new();
            for cell in cells {
                if let WlNode::Call { head: cell_head, args: cell_args } = cell
                    && is_sym(cell_head, "Cell") && cell_args.len() >= 2
                    && matches!(&cell_args[1], WlNode::Str(s) if s == "Input")
                {
                        let cell_code = box_to_code(&cell_args[0]);
                        if !cell_code.trim().is_empty() {
                            code_parts.push(cell_code);
                        }
                }
            }
            return Ok(code_parts.join(";\n"));
        }
    }
    // Notebook with no Input cells is valid — return empty string.
    Ok(String::new())
}

/// Read a `.m` file (WL source code), returning the content as-is.
///
/// `.m` files are plain-text Wolfram Language source files (packages,
/// rule definitions, etc.). They use the same syntax as Syma for the
/// most part (comments with `(* *)`, function calls with `[...]`,
/// rules with `->`, definitions with `:=`).
pub fn wl_source_to_code(contents: &str) -> String {
    contents.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_nb_with_input_cell() {
        let nb = r#"(* Content-type: application/mathematica *)
Notebook[{
Cell[BoxData[
    RowBox[{"1", "+", "2"}]], "Input",
CellLabel->"In[1]:="]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        // Remove spaces: box-to-code concatenation doesn't add spacing
        assert!(code.replace(" ", "").contains("1+2"));
    }

    #[test]
    fn test_nb_with_multiple_input_cells() {
        let nb = r#"Notebook[{
Cell[BoxData[RowBox[{"x", "^", "2"}]], "Input"],
Cell[BoxData[RowBox[{"Integrate", "[",
    RowBox[{RowBox[{"Sin", "[", "x", "]"}], ",", "x"}], "]"}]], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        // Box-to-code produces no extra spaces, so x^2 appears directly
        assert!(code.replace(" ", "").contains("x^2"));
        assert!(code.replace(" ", "").contains("Integrate[Sin[x],x]"));
    }

    #[test]
    fn test_nb_with_non_input_cells() {
        let nb = r#"Notebook[{
Cell[BoxData[StyleBox["Title", FontSize->24]], "Title"],
Cell[BoxData[RowBox[{"a", "+", "b"}]], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        // Only the Input cell code is extracted; spaces depend on concatenation
        assert!(code.replace(" ", "").contains("a+b"));
    }

    #[test]
    fn test_nb_with_fraction_box() {
        let nb = r#"Notebook[{
Cell[BoxData[FractionBox["1",
    RowBox[{"x", "+", "1"}]]], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        // Box-to-code produces no extra spaces; the fraction renders as (1)/(x+1)
        let clean = code.replace(" ", "");
        assert!(clean.contains("1/(x+1)") || clean.contains("(1)/(x+1)"));
    }

    #[test]
    fn test_nb_with_superscript() {
        let nb = r#"Notebook[{
Cell[BoxData[SuperscriptBox["x", "2"]], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        assert_eq!(code.trim(), "x^2");
    }

    #[test]
    fn test_nb_with_nested_subsuperscript() {
        let nb = r#"Notebook[{
Cell[BoxData[SubsuperscriptBox["x", "i", "2"]], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        assert_eq!(code.trim(), "Subscript[x, i]^2");
    }

    #[test]
    fn test_nb_with_sqrt() {
        let nb = r#"Notebook[{
Cell[BoxData[SqrtBox["x"]], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        assert_eq!(code.trim(), "Sqrt[x]");
    }

    #[test]
    fn test_nb_with_interpretation_box() {
        let nb = r#"Notebook[{
Cell[BoxData[InterpretationBox["display", "code"]], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        assert_eq!(code.trim(), "code");
    }

    #[test]
    fn test_nb_complex_expression() {
        let nb = r#"Notebook[{
Cell[BoxData[RowBox[{"Plot", "[",
    RowBox[{
        RowBox[{"Sin", "[", "x", "]"}], ",",
        RowBox[{"{",
            RowBox[{"x", ",",
                RowBox[{"-", "Pi"}], ",", "Pi"}], "}"}]}], "]"}]], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        let clean = code.replace(" ", "");
        assert!(clean.contains("Plot"));
        assert!(clean.contains("Sin[x]"));
        assert!(clean.contains("{x,-Pi,Pi}"));
    }

    #[test]
    fn test_nb_empty_notebook() {
        let nb = "Notebook[{}]";
        let code = notebook_to_code(nb).unwrap();
        assert!(code.trim().is_empty());
    }

    #[test]
    fn test_nb_no_notebook() {
        let nb = "(* just a comment *)";
        assert!(notebook_to_code(nb).is_err());
    }

    #[test]
    fn test_nb_style_box_unwrap() {
        let nb = r#"Notebook[{
Cell[BoxData[StyleBox[RowBox[{"a", "+", "b"}], FontSize->14]], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        assert!(code.replace(" ", "").contains("a+b"));
    }

    #[test]
    fn test_nb_tag_box_unwrap() {
        let nb = r#"Notebook[{
Cell[BoxData[TagBox[RowBox[{"f", "[", "x", "]"}], "Hold"]], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        assert_eq!(code.trim(), "f[x]");
    }

    #[test]
    fn test_plain_text_input_cell() {
        let nb = r#"Notebook[{
Cell["1 + 2 + 3", "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        assert_eq!(code.trim(), "1 + 2 + 3");
    }

    #[test]
    fn test_nb_plain_string_inside_rowbox() {
        // Regression: strings inside boxes should be concatenated verbatim
        let nb = r#"Notebook[{
Cell[BoxData[RowBox[{"\"Hello, \"", "\"World!\""}]], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        assert!(code.contains("Hello") || code.contains("Hello, World"));
    }

    #[test]
    fn test_wl_source_simple() {
        let src = "f[x_] := x^2\nIntegrate[f[x], x]";
        assert_eq!(wl_source_to_code(src), src);
    }

    #[test]
    fn test_parse_nb_with_nested_comments() {
        // NB files have nested comments in metadata
        let nb = r#"(* Header (* with nested *) comment *)
Notebook[{
Cell[BoxData["code"], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        assert_eq!(code.trim(), "code");
    }

    #[test]
    fn test_graphics_box_skipped() {
        let nb = r#"Notebook[{
Cell[BoxData[GraphicsBox[
    {RGBColor[1,0,0], Line[{{0,0},{1,1}}]}
    ]], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        assert!(code.trim().is_empty());
    }

    #[test]
    fn test_dynamic_box_skipped() {
        let nb = r#"Notebook[{
Cell[BoxData[DynamicBox[Typeset`ToBoxes[var]]], "Input"]
}]"#;
        let code = notebook_to_code(nb).unwrap();
        assert!(code.trim().is_empty());
    }
}
