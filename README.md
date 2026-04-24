# `syma` — Symbolic Language Crate

The core `syma` language crate: lexer, parser, evaluator, pattern engine, builtins, REPL, and tooling.

## Pipeline

```
Source → Lexer → Parser → AST → Evaluator → Value
```

## Module Map

| Module | Files | Responsibility |
|--------|-------|---------------|
| `lexer.rs` | 1 | Tokenizer — Wolfram-style multi-char operators, maximal munch, nested comments, `x_Integer` as single token |
| `parser.rs` | 1 | Recursive-descent precedence climbing parser |
| `ast.rs` | 1 | `Expr` enum — the universal AST node (40+ variants) |
| `eval/` | 5 | `mod.rs` (core dispatch, `apply_function`, `try_match_params`), `rules.rs` (rule application), `numeric.rs` (high-precision N[]), `table.rs` (Table/Sum/ParallelTable/Do), `plot.rs` (Plot → SVG) |
| `value.rs` | 1 | `Value` enum — runtime types (atoms, List, Call, Assoc, Function, Builtin, Sequence, …) |
| `env.rs` | 1 | Lexical scoping via `Rc<RefCell<Scope>>` chains |
| `pattern.rs` | 1 | Pattern matching engine — blanks, sequences (`__`, `___`), guards, alternatives, list/call patterns, backtracking |
| `builtins/` | 17 files | ~100+ builtins: arithmetic, list, string, math, symbolic (Simplify/D/Integrate), pattern, I/O, FFI, parallel, linalg, graphics, … |
| `cli.rs` | 1 | Package scaffolding (`syma new`, `run`, `build`, `test`, `add`, `remove`) |
| `debug.rs` | 1 | DAP (Debug Adapter Protocol) support |
| `kernel.rs` | 1 | JSON-over-stdin/stdout kernel mode (IDE integration) |
| `ffi/` | 4 files | Native library loading, marshalling, extension dispatch |
| `format.rs` | 1 | Terminal formatting helpers |
| `manifest.rs` | 1 | Package manifest (`syma.toml`) parsing |

## Key Design Decisions

- **Operators are function calls** — `a + b` desugars to `Plus[a, b]`. Unary `-x` → `Times[-1, x]`.
- **Pattern-as-value** — Patterns in rules are stored as `Value::Pattern(Expr)` — unevaluated AST.
- **Accumulating definitions** — `f[x_] := body1; f[x_] := body2` — definitions accumulate; tried in order.
- **Lazy Rubi loading** — `Integrate` loads 185 rule files on first call via `OnceLock`.
- **Sequence values** — `x__` binds to `Value::Sequence`, which auto-splats in lists and calls (Wolfram-compatible).
- **High-precision numeric** — `Value::Real` uses `rug::Float` (arbitrary precision); `N[expr, prec]` for numeric evaluation.

## Development

```bash
# Quick cycle
cargo check                    # Fast type-check
cargo test                     # All unit + integration tests
cargo test lexer               # Test a specific module
cargo test --test cli          # Integration tests only
cargo clippy -- -D warnings    # Lint (zero warnings required)
cargo fmt                      # Format
```

### Running

```bash
cargo run                      # Launch REPL
cargo run -- -e "1 + 2"       # Evaluate expression
cargo run -- <file.syma>      # Run a file
```

### Feature Flags

- `rubi` (cargo-xtask only) — Enables the Rubi rule-based integration engine. Adds ~185 `.m` rule files and the `Integrate` builtin.

## Testing Conventions

- **Unit tests**: inline `#[cfg(test)] mod tests` at the bottom of each module.
- **Integration tests**: `tests/cli.rs` runs example `.syma` files from `examples/`.
- Run `cargo xtask test` (or workspace-level `cargo test --workspace`) to run all tests including the `xtask` crate.

## Dependencies

- `rug` — Arbitrary-precision integers and floats (via GMP/MPFR)
- `rustyline` — REPL line editing
- `serde` / `serde_json` — Serialization for JSON I/O and kernel mode
- `fastrand` — Fast non-cryptographic randomness
