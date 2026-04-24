# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Syma** — a symbolic-first programming language inspired by Wolfram Language, with OOP structure. Written in Rust (edition 2024). Currently in Phase 1: tree-walk interpreter with REPL.

The full language specification is in `syma-lang.md` (1200+ lines, includes EBNF grammar). That file is the source of truth for syntax and semantics.

> **New to the codebase?** See [`docs/developer-guide.md`](../docs/developer-guide.md) for a comprehensive walkthrough: codebase map, pipeline internals, how to add builtins, how to add language features, pattern matching, testing, and common pitfalls.

## Build & Test Commands

```bash
cargo build              # Build the project
cargo test               # Run all tests (unit + integration)
cargo test lexer         # Run tests for a specific module (lexer, parser, eval, etc.)
cargo test --test cli    # Run only integration tests
cargo run                # Launch the REPL
cargo check              # Fast type-check without compiling
cargo fmt                # Format all code
cargo clippy -- -D warnings  # Lint (zero warnings required)
```

## Architecture

The pipeline is: **Source → Lexer → Parser → AST → Evaluator → Value**

### Module Responsibilities

- **`lexer.rs`** — Tokenizer. Handles Wolfram-style multi-char operators with maximal munch (`//.` before `//` before `/`). `(* *)` comments support nesting. Pattern blanks like `x_Integer` are lexed as single identifiers.
- **`ast.rs`** — AST node definitions. Everything is an `Expr` enum variant. Operators are desugared to `Call` nodes (e.g., `a + b` → `Plus[a, b]`).
- **`parser.rs`** — Recursive descent with precedence climbing. Expression precedence (low→high): pipe (`//`) → at/apply (`@`/`@@`) → rule (`->`/`:>`) → or → and → comparison → add → mul → power → unary → postfix → primary. Pattern parsing mirrors expression parsing with a parallel set of `parse_pattern_*` methods.
- **`value.rs`** — Runtime value types. Key types: atoms, `List`, `Call`, `Assoc` (hash map), `Function` (user-defined with pattern defs), `Builtin`, `PureFunction`, `Object`, `RuleSet`, `Pattern` (wraps unevaluated `Expr`).
- **`eval.rs`** — Tree-walk evaluator. `eval()` dispatches on `Expr` variants. `apply_function()` dispatches on `Value` types (builtin, user function, pure function, symbol lookup). Function definitions accumulate — multiple `f[x_] := ...` defs coexist and are tried in order.
- **`env.rs`** — Lexical scoping via `Rc<RefCell<Scope>>` chains. `child()` creates a new scope inheriting the parent.
- **`pattern.rs`** — Pattern matching engine. Supports blanks (`_`, `x_`, `_Integer`), sequences (`__`, `___`), list destructuring, call patterns, alternatives (`|`), and guards (`/;`). Guards are partially implemented — inner pattern matches but guard condition isn't evaluated yet.
- **`builtins/`** — Core library split into sub-modules by domain:
  - `mod.rs` — Orchestrator: `register_builtins`, `get_help`, `get_attributes`, `add_values_public` re-export
  - `arithmetic.rs` — `Plus`, `Times`, `Power`, `Divide`, `Minus`, `Abs` + helpers (`add_values`, `mul_values`)
  - `comparison.rs` — `Equal`, `Unequal`, `Less`, `Greater`, `LessEqual`, `GreaterEqual`
  - `logical.rs` — `And`, `Or`, `Not`
  - `list.rs` — `Length`, `First`, `Last`, `Rest`, `Append`, `Join`, `Flatten`, `Sort`, `Reverse`, `Part`, `Range`, `Table`, `Map`, `Fold`, `Select`, `Scan`, `Nest`, `Take`, `Drop`, `Riffle`, `Transpose`, `Total`, `MemberQ`, `Count`, `Position`, `Union`, `Intersection`, `Complement`, `Tally`, `PadLeft`, `PadRight`
  - `string.rs` — `StringJoin`, `StringLength`, `ToString`, `ToExpression`, `StringSplit`, `StringReplace`, `StringTake`, `StringDrop`, `StringContainsQ`, `StringReverse`, `ToUpperCase`, `ToLowerCase`, `Characters`, `StringMatchQ`, `StringPadLeft`, `StringPadRight`, `StringTrim`, `StringStartsQ`, `StringEndsQ`
  - `math.rs` — `Sin`, `Cos`, `Tan`, `Log`, `Exp`, `Sqrt`, `Floor`, `Ceiling`, `Round`, `Max`, `Min`, `ArcSin`, `ArcCos`, `ArcTan`, `Log2`, `Log10`, `Mod`, `GCD`, `LCM`, `Factorial`, `FixedPoint` stub
  - `pattern.rs` — `MatchQ`, `Head`, `TypeOf`, `FreeQ`
  - `association.rs` — `Keys`, `Values`, `Lookup`, `KeyExistsQ`
  - `symbolic.rs` — `Simplify`, `Expand`, `D` (differentiation), `Integrate`, `Factor`, `Solve`, `Series`
  - `random.rs` — `RandomInteger`, `RandomReal`, `RandomChoice`
  - `io.rs` — `Print`, `Input`, `Write`, `WriteLine`, `PrintF`, `WriteString`, `ReadString`, `Export`, `Import`
  - `error.rs` — `Throw`, `Error`

### Key Design Decisions

- **Operators → Calls**: The parser desugars all operators into `Call` nodes with PascalCase heads. `a - b` becomes `Plus[a, Times[-1, b]]`. The evaluator then looks up `Plus`, `Times`, etc. as builtins.
- **Pattern-as-Value**: Patterns in rules/function defs are stored as `Value::Pattern(Expr)` — unevaluated AST wrapped in a value. This enables symbolic manipulation.
- **Subtraction desugaring**: `a - b` is parsed as `Plus[a, Times[-1, b]]`, not as a separate operation. Same for unary `-x` → `Times[-1, x]`.
- **`x_` in lexer**: Pattern blanks like `x_Integer` are single `Ident` tokens. The parser's `convert_pattern()` method splits them by `_`.

### What's Not Yet Implemented

- Class/module evaluation (parsed but only stores a symbol marker)
- Import system
- Guard condition evaluation in pattern matching
- Pure function (`#`/`#1`) evaluation is basic — only via `PureFunction` value type
- Bytecode compilation and JIT (Phase 2/3)

## CI

The project uses GitHub Actions (`.github/workflows/ci.yml`) with three jobs:

1. **check** — `cargo check --locked` (fast type-check, runs first)
2. **test** — `cargo test --locked` (all unit + integration tests, depends on check)
3. **lint** — `cargo fmt --check` + `cargo clippy --locked -- -D warnings` (depends on check)

The `rug` crate requires `libgmp-dev` and `clang` on Ubuntu runners. `Cargo.lock` is checked in; all jobs use `--locked` for reproducible builds. Build artifacts are cached via `actions/cache@v4`.

## Syntax Notes for Writing Tests

- `(* comment *)` for comments (nestable)
- `;` separates statements, last expression is the result
- `f[x_] := body` for delayed function def, `x = val` for assignment
- `{1, 2, 3}` is `List[1, 2, 3]`
- Operators: `+`, `-`, `*`, `/`, `^`, `==`, `!=`, `<`, `>`, `<=`, `>=`, `&&`, `||`, `!`
- Rule operators: `->` (immediate), `:>` (delayed), `/.` (replace all), `//.` (replace repeated)
- `//` is postfix pipe, `@` is prefix, `@@` is apply, `/@` is map
