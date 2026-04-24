pub mod arithmetic;
pub mod association;
pub mod comparison;
pub mod error;
pub mod ffi;
pub mod format;
pub mod filesystem;
pub mod graphics;
pub mod io;
pub mod linalg;
pub mod list;
pub mod logical;
pub mod math;
pub mod parallel;
pub mod pattern;
pub mod random;
pub mod statistics;
pub mod string;
pub mod symbolic;

use crate::env::{Env, LazyProvider};
use crate::value::{BuiltinFn, EvalError, Value};
use std::collections::HashMap;
use std::sync::Arc;

/// Register all built-in functions in the environment.
pub fn register_builtins(env: &Env) {
    // ── Arithmetic ──
    register_builtin(env, "Plus", arithmetic::builtin_plus);
    register_builtin(env, "Times", arithmetic::builtin_times);
    register_builtin(env, "Power", arithmetic::builtin_power);
    register_builtin(env, "Divide", arithmetic::builtin_divide);
    register_builtin(env, "Minus", arithmetic::builtin_minus);
    register_builtin(env, "Abs", arithmetic::builtin_abs);

    // ── Comparison ──
    register_builtin(env, "Equal", comparison::builtin_equal);
    register_builtin(env, "Unequal", comparison::builtin_unequal);
    register_builtin(env, "Less", comparison::builtin_less);
    register_builtin(env, "Greater", comparison::builtin_greater);
    register_builtin(env, "LessEqual", comparison::builtin_less_equal);
    register_builtin(env, "GreaterEqual", comparison::builtin_greater_equal);

    // ── Logical ──
    register_builtin(env, "And", logical::builtin_and);
    register_builtin(env, "Or", logical::builtin_or);
    register_builtin(env, "Not", logical::builtin_not);

    // ── List ──
    register_builtin(env, "Length", list::builtin_length);
    register_builtin(env, "First", list::builtin_first);
    register_builtin(env, "Last", list::builtin_last);
    register_builtin(env, "Rest", list::builtin_rest);
    register_builtin(env, "Most", list::builtin_most);
    register_builtin(env, "Append", list::builtin_append);
    register_builtin(env, "Prepend", list::builtin_prepend);
    register_builtin(env, "Join", list::builtin_join);
    register_builtin(env, "Flatten", list::builtin_flatten);
    register_builtin(env, "Sort", list::builtin_sort);
    register_builtin(env, "Reverse", list::builtin_reverse);
    register_builtin(env, "Part", list::builtin_part);
    register_builtin(env, "Range", list::builtin_range);
    register_builtin(env, "Table", list::builtin_table);
    register_builtin_env(env, "Map", list::builtin_map);
    register_builtin_env(env, "Fold", list::builtin_fold);
    register_builtin_env(env, "Select", list::builtin_select);
    register_builtin_env(env, "Scan", list::builtin_scan);
    register_builtin_env(env, "Nest", list::builtin_nest);
    register_builtin(env, "Take", list::builtin_take);
    register_builtin(env, "Drop", list::builtin_drop);
    register_builtin(env, "Riffle", list::builtin_riffle);
    register_builtin(env, "Transpose", list::builtin_transpose);
    register_builtin(env, "Total", list::builtin_total);
    register_builtin(env, "Sum", list::builtin_sum);

    // ── Pattern ──
    register_builtin(env, "MatchQ", pattern::builtin_match_q);
    register_builtin(env, "Head", pattern::builtin_head);
    register_builtin(env, "TypeOf", pattern::builtin_type_of);
    register_builtin(env, "FreeQ", pattern::builtin_free_q);

    // ── String ──
    register_builtin(env, "StringJoin", string::builtin_string_join);
    register_builtin(env, "StringLength", string::builtin_string_length);
    register_builtin(env, "ToString", string::builtin_to_string);
    register_builtin(env, "ToExpression", string::builtin_to_expression);

    // ── Math ──
    register_builtin(env, "Sin", math::builtin_sin);
    register_builtin(env, "Cos", math::builtin_cos);
    register_builtin(env, "Tan", math::builtin_tan);
    register_builtin(env, "Log", math::builtin_log);
    register_builtin(env, "Exp", math::builtin_exp);
    register_builtin(env, "Sqrt", math::builtin_sqrt);
    register_builtin(env, "Floor", math::builtin_floor);
    register_builtin(env, "Ceiling", math::builtin_ceiling);
    register_builtin(env, "Round", math::builtin_round);
    register_builtin(env, "Max", math::builtin_max);
    register_builtin(env, "Min", math::builtin_min);

    // ── I/O ──
    register_builtin(env, "Print", io::builtin_print);

    // ── Association ──
    register_builtin(env, "Keys", association::builtin_keys);
    register_builtin(env, "Values", association::builtin_values);

    // ── Symbolic ──
    register_builtin(env, "Simplify", symbolic::builtin_simplify);
    register_builtin(env, "Expand", symbolic::builtin_expand);
    register_builtin(env, "D", symbolic::builtin_d);
    register_builtin(env, "Factor", symbolic::builtin_factor);
    register_builtin(env, "Solve", symbolic::builtin_solve);
    register_builtin(env, "Series", symbolic::builtin_series);

    // ── Control (evaluator-dependent) ──
    register_builtin_env(env, "FixedPoint", math::builtin_fixed_point);

    // ── Package loading (evaluator-dependent) ──
    register_builtin_env(env, "Needs", builtin_needs);

    // ── Graphics (evaluator-dependent) ──
    register_builtin(env, "Plot", graphics::builtin_plot_stub);

    // ── Attributes ──
    register_builtin_env(env, "SetAttributes", symbolic::builtin_set_attributes);
    register_builtin_env(env, "Attributes", symbolic::builtin_attributes);

    // ── Extended math ──
    register_builtin(env, "ArcSin", math::builtin_arcsin);
    register_builtin(env, "ArcCos", math::builtin_arccos);
    register_builtin(env, "ArcTan", math::builtin_arctan);
    register_builtin(env, "Log2", math::builtin_log2);
    register_builtin(env, "Log10", math::builtin_log10);
    register_builtin(env, "Mod", math::builtin_mod);
    register_builtin(env, "GCD", math::builtin_gcd);
    register_builtin(env, "LCM", math::builtin_lcm);
    register_builtin(env, "Factorial", math::builtin_factorial);

    // ── Reciprocal trig ──
    register_builtin(env, "Csc", math::builtin_csc);
    register_builtin(env, "Sec", math::builtin_sec);
    register_builtin(env, "Cot", math::builtin_cot);

    // ── Inverse reciprocal trig ──
    register_builtin(env, "ArcCsc", math::builtin_arccsc);
    register_builtin(env, "ArcSec", math::builtin_arcsec);
    register_builtin(env, "ArcCot", math::builtin_arccot);

    // ── Haversine ──
    register_builtin(env, "Haversine", math::builtin_haversine);
    register_builtin(env, "InverseHaversine", math::builtin_inverse_haversine);

    // ── Degree-based trig ──
    register_builtin(env, "SinDegrees", math::builtin_sin_degrees);
    register_builtin(env, "CosDegrees", math::builtin_cos_degrees);
    register_builtin(env, "TanDegrees", math::builtin_tan_degrees);
    register_builtin(env, "CscDegrees", math::builtin_csc_degrees);
    register_builtin(env, "SecDegrees", math::builtin_sec_degrees);
    register_builtin(env, "CotDegrees", math::builtin_cot_degrees);

    // ── Inverse trig (degrees) ──
    register_builtin(env, "ArcSinDegrees", math::builtin_arcsin_degrees);
    register_builtin(env, "ArcCosDegrees", math::builtin_arccos_degrees);
    register_builtin(env, "ArcTanDegrees", math::builtin_arctan_degrees);
    register_builtin(env, "ArcCscDegrees", math::builtin_arccsc_degrees);
    register_builtin(env, "ArcSecDegrees", math::builtin_arcsec_degrees);
    register_builtin(env, "ArcCotDegrees", math::builtin_arccot_degrees);

    // ── Random ──
    register_builtin(env, "RandomInteger", random::builtin_random_integer);
    register_builtin(env, "RandomReal", random::builtin_random_real);
    register_builtin(env, "RandomChoice", random::builtin_random_choice);

    // ── Extended string ──
    register_builtin(env, "StringSplit", string::builtin_string_split);
    register_builtin(env, "StringReplace", string::builtin_string_replace);
    register_builtin(env, "StringTake", string::builtin_string_take);
    register_builtin(env, "StringDrop", string::builtin_string_drop);
    register_builtin(env, "StringContainsQ", string::builtin_string_contains_q);
    register_builtin(env, "StringReverse", string::builtin_string_reverse);
    register_builtin(env, "ToUpperCase", string::builtin_to_upper_case);
    register_builtin(env, "ToLowerCase", string::builtin_to_lower_case);

    // ── Extended list ──
    register_builtin(env, "MemberQ", list::builtin_member_q);
    register_builtin(env, "Count", list::builtin_count);
    register_builtin(env, "Position", list::builtin_position);
    register_builtin(env, "Union", list::builtin_union);
    register_builtin(env, "Intersection", list::builtin_intersection);
    register_builtin(env, "Complement", list::builtin_complement);
    register_builtin(env, "Tally", list::builtin_tally);
    register_builtin(env, "PadLeft", list::builtin_pad_left);
    register_builtin(env, "PadRight", list::builtin_pad_right);

    // ── Association extended ──
    register_builtin(env, "Lookup", association::builtin_lookup);
    register_builtin(env, "KeyExistsQ", association::builtin_key_exists_q);

    // ── I/O ──
    register_builtin(env, "Input", io::builtin_input);
    register_builtin(env, "Write", io::builtin_write);
    register_builtin(env, "WriteLine", io::builtin_write_line);
    register_builtin(env, "PrintF", io::builtin_printf);
    register_builtin(env, "WriteString", io::builtin_write_string);
    register_builtin(env, "ReadString", io::builtin_read_string);
    register_builtin(env, "Export", io::builtin_export);
    register_builtin(env, "Import", io::builtin_import);

    // ── Error handling ──
    register_builtin(env, "Throw", error::builtin_throw);
    register_builtin(env, "Error", error::builtin_error);

    // ── Extended string (Characters, StringMatchQ, padding, trimming) ──
    register_builtin(env, "Characters", string::builtin_characters);
    register_builtin(env, "StringMatchQ", string::builtin_string_match_q);
    register_builtin(env, "StringPadLeft", string::builtin_string_pad_left);
    register_builtin(env, "StringPadRight", string::builtin_string_pad_right);
    register_builtin(env, "StringTrim", string::builtin_string_trim);
    register_builtin(env, "StringStartsQ", string::builtin_string_starts_q);
    register_builtin(env, "StringEndsQ", string::builtin_string_ends_q);

    // ── Parallel computation ──
    register_builtin_env(env, "ParallelMap", parallel::builtin_parallel_map);
    register_builtin(env, "ParallelTable", parallel::builtin_parallel_table);
    register_builtin(env, "LaunchKernels", parallel::builtin_launch_kernels);
    register_builtin(env, "CloseKernels", parallel::builtin_close_kernels);
    register_builtin(env, "KernelCount", parallel::builtin_kernel_count);

    // ── FFI ──
    register_builtin_env(env, "LoadLibrary", ffi::builtin_load_library);
    register_builtin_env(env, "LoadExtension", ffi::builtin_load_extension);
    register_builtin_env(env, "ExternalEvaluate", ffi::builtin_external_evaluate);
    register_builtin_env(env, "LibraryFunction", ffi::builtin_library_function);
    register_builtin_env(env, "LibraryFunctionLoad", ffi::builtin_library_function_load);

    // ── File system ──
    register_builtin(env, "FileNameSplit", filesystem::builtin_file_name_split);
    register_builtin(env, "FileNameJoin", filesystem::builtin_file_name_join);
    register_builtin(env, "FileNameTake", filesystem::builtin_file_name_take);
    register_builtin(env, "FileNameDrop", filesystem::builtin_file_name_drop);
    register_builtin(env, "FileBaseName", filesystem::builtin_file_base_name);
    register_builtin(env, "FileExtension", filesystem::builtin_file_extension);
    register_builtin(env, "FileNameDepth", filesystem::builtin_file_name_depth);
    register_builtin(env, "DirectoryName", filesystem::builtin_directory_name);
    register_builtin(env, "ParentDirectory", filesystem::builtin_parent_directory);
    register_builtin(env, "ExpandFileName", filesystem::builtin_expand_file_name);
    register_builtin(env, "FileExistsQ", filesystem::builtin_file_exists_q);
    register_builtin(env, "DirectoryQ", filesystem::builtin_directory_q);
    register_builtin(env, "FileNames", filesystem::builtin_file_names);

    // ── Format/display ──
    register_builtin(env, "InputForm", format::builtin_input_form);
    register_builtin(env, "FullForm", format::builtin_full_form);
    register_builtin(env, "Short", format::builtin_short);
    register_builtin(env, "Shallow", format::builtin_shallow);
    register_builtin(env, "NumberForm", format::builtin_number_form);
    register_builtin(env, "ScientificForm", format::builtin_scientific_form);
    register_builtin(env, "BaseForm", format::builtin_base_form);
    register_builtin(env, "Grid", format::builtin_grid);
    register_builtin(env, "Defer", format::builtin_defer);
    register_builtin(env, "SyntaxQ", format::builtin_syntax_q);
    register_builtin(env, "SyntaxLength", format::builtin_syntax_length);

    // ── Constants (kept symbolic; use N[] for numerical evaluation) ──
    env.set("Pi".to_string(), Value::Symbol("Pi".to_string()));
    env.set("E".to_string(), Value::Symbol("E".to_string()));
    env.set("I".to_string(), Value::Complex { re: 0.0, im: 1.0 });
    env.set("Alice".to_string(), Value::Symbol("Alice".to_string()));
    // Degree = Pi / 180  (radians per degree)
    env.set(
        "Degree".to_string(),
        Value::Real(
            rug::Float::with_val(crate::value::DEFAULT_PRECISION, rug::float::Constant::Pi)
                / 180u32,
        ),
    );

    // ── Lazy package auto-loading ──────────────────────────────────────
    // Register lazy providers so that symbols from loadable packages
    // (LinearAlgebra, Statistics, Graphics) auto-load on first use
    // without requiring an explicit Needs[] call.

    // LinearAlgebra — symbols backed by Rust builtins in linalg.rs
    register_lazy_package(
        env,
        &[
            "Dimensions",
            "Dot",
            "MatrixMultiply",
            "IdentityMatrix",
            "Det",
            "Inverse",
            "Transpose",
            "Tr",
            "Norm",
            "Cross",
            "LinearSolve",
        ],
        linalg::SYMBOLS,
        "LinearAlgebra",
        linalg::register,
    );

    // Statistics — symbols backed by Rust builtins in statistics.rs
    register_lazy_package(
        env,
        &[
            "Mean",
            "Median",
            "Variance",
            "StandardDeviation",
            "Quantile",
            "Covariance",
            "Correlation",
            "RandomVariate",
            "NormalDistribution",
            "UniformDistribution",
            "PoissonDistribution",
        ],
        statistics::SYMBOLS,
        "Statistics",
        statistics::register,
    );

    // Graphics — symbols backed by Rust builtins in graphics.rs
    // (Plot is already registered as a builtin stub + special form in eval.rs)
    register_lazy_package(
        env,
        &["ListPlot", "ListLinePlot", "ExportGraphics", "Graphics"],
        graphics::SYMBOLS,
        "Graphics",
        graphics::register,
    );
}

fn register_builtin(env: &Env, name: &str, func: fn(&[Value]) -> Result<Value, EvalError>) {
    env.set(
        name.to_string(),
        Value::Builtin(name.to_string(), BuiltinFn::Pure(func)),
    );
    let attrs = get_attributes(name);
    if !attrs.is_empty() {
        env.set_attributes(name, attrs.iter().map(|s| s.to_string()).collect());
    }
}

/// Register an environment-aware built-in function.
fn register_builtin_env(env: &Env, name: &str, func: fn(&[Value], &Env) -> Result<Value, EvalError>) {
    env.set(
        name.to_string(),
        Value::Builtin(name.to_string(), BuiltinFn::Env(func)),
    );
    let attrs = get_attributes(name);
    if !attrs.is_empty() {
        env.set_attributes(name, attrs.iter().map(|s| s.to_string()).collect());
    }
}

/// Register lazy providers so that symbols from load-on-use packages
/// auto-load the first time they are used as a function.
///
/// `symbol_names` are the symbols backed by Rust builtins (registered by `register_fn`).
/// `all_symbols` is the full SYMBOLS list for the package (used for module registration).
fn register_lazy_package(
    env: &Env,
    symbol_names: &[&str],
    all_symbols: &[&str],
    package_name: &str,
    register_fn: fn(&Env),
) {
    let all_syms: Vec<String> = all_symbols.iter().map(|s| s.to_string()).collect();
    let pkg_name = package_name.to_string();

    for &sym in symbol_names {
        let sym_owned = sym.to_string();
        let all_syms_clone = all_syms.clone();
        let pkg_name_clone = pkg_name.clone();
        env.register_lazy_provider(
            sym,
            LazyProvider::Custom(Arc::new(move |env| {
                // Register all Rust-builtin symbols for this package (idempotent)
                register_fn(env);

                // Register the module so Needs[] idempotency works
                let exports: HashMap<String, Value> = all_syms_clone
                    .iter()
                    .filter_map(|s| env.get(s).map(|v| (s.clone(), v)))
                    .collect();
                env.register_module(
                    pkg_name_clone.clone(),
                    Value::Module {
                        name: pkg_name_clone.clone(),
                        exports,
                    },
                );

                // Return the value of the requested symbol
                // (Sibling lazy providers remain in the map but will never
                // fire since the symbols are now bound in the environment.)
                env.get(&sym_owned).ok_or_else(|| {
                    EvalError::Error(format!(
                        "Symbol '{sym_owned}' not found after loading package '{pkg_name_clone}'"
                    ))
                })
            })),
        );
    }
}

/// `Needs["PackageName"]` — load a standard library package.
fn builtin_needs(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "Needs requires exactly 1 argument".to_string(),
        ));
    }
    let pkg_name = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[0].type_name().to_string(),
            });
        }
    };
    // Already loaded?
    if env.get_module(&pkg_name).is_some() {
        return Ok(Value::Null);
    }
    use std::collections::HashMap;
    match pkg_name.as_str() {
        "LinearAlgebra" => {
            crate::builtins::linalg::register(env);
            let exports: HashMap<String, Value> = crate::builtins::linalg::SYMBOLS
                .iter()
                .filter_map(|&sym| env.get(sym).map(|v| (sym.to_string(), v)))
                .collect();
            let module = Value::Module {
                name: "LinearAlgebra".to_string(),
                exports,
            };
            env.register_module("LinearAlgebra".to_string(), module);
        }
        "Statistics" => {
            crate::builtins::statistics::register(env);
            let exports: HashMap<String, Value> = crate::builtins::statistics::SYMBOLS
                .iter()
                .filter_map(|&sym| env.get(sym).map(|v| (sym.to_string(), v)))
                .collect();
            let module = Value::Module {
                name: "Statistics".to_string(),
                exports,
            };
            env.register_module("Statistics".to_string(), module);
        }
        "Graphics" => {
            crate::builtins::graphics::register(env);
            let exports: HashMap<String, Value> = crate::builtins::graphics::SYMBOLS
                .iter()
                .filter_map(|&sym| env.get(sym).map(|v| (sym.to_string(), v)))
                .collect();
            let module = Value::Module {
                name: "Graphics".to_string(),
                exports,
            };
            env.register_module("Graphics".to_string(), module);
        }
        _ => {
            return Err(EvalError::Error(format!(
                "Package '{}' not found. Built-in packages: LinearAlgebra, Statistics, Graphics.",
                pkg_name
            )));
        }
    }
    Ok(Value::Null)
}

/// Re-export for use by eval.rs
pub use arithmetic::add_values_public;

// ── Help documentation ──

/// Look up help text for a built-in symbol.
///
/// Returns the usage string (including attributes) if documented.
pub fn get_help(name: &str) -> Option<&'static str> {
    Some(match name {
        // ── Arithmetic ──
        "Plus" => "Plus[a, b, ...] or a + b + ... computes the sum of its arguments.",
        "Times" => "Times[a, b, ...] or a * b * ... computes the product of its arguments.",
        "Power" => "Power[a, b] or a ^ b gives a raised to the power b.",
        "Divide" => "Divide[a, b] or a / b gives a divided by b.",
        "Minus" => "Minus[x] or -x gives the negation of x.",
        "Abs" => "Abs[x] gives the absolute value of x.",

        // ── Comparison ──
        "Equal" => "Equal[a, b] or a == b returns True if a and b are equal.",
        "Unequal" => "Unequal[a, b] or a != b returns True if a and b are not equal.",
        "Less" => "Less[a, b] or a < b returns True if a is strictly less than b.",
        "Greater" => "Greater[a, b] or a > b returns True if a is strictly greater than b.",
        "LessEqual" => "LessEqual[a, b] or a <= b returns True if a is less than or equal to b.",
        "GreaterEqual" => {
            "GreaterEqual[a, b] or a >= b returns True if a is greater than or equal to b."
        }

        // ── Logical ──
        "And" => "And[a, b, ...] or a && b && ... returns True if all arguments are True.",
        "Or" => "Or[a, b, ...] or a || b || ... returns True if any argument is True.",
        "Not" => "Not[expr] or !expr returns the logical negation of expr.",

        // ── List ──
        "Length" => "Length[expr] gives the number of elements in expr.",
        "First" => "First[expr] gives the first element of expr.",
        "Last" => "Last[expr] gives the last element of expr.",
        "Rest" => "Rest[expr] gives expr with the first element removed.",
        "Most" => "Most[expr] gives expr with the last element removed.",
        "Append" => "Append[expr, elem] returns expr with elem appended.",
        "Prepend" => "Prepend[expr, elem] returns expr with elem prepended.",
        "Join" => "Join[list1, list2, ...] concatenates lists together.",
        "Flatten" => "Flatten[expr] flattens nested lists into a single list.",
        "Sort" => "Sort[list] sorts the elements of list into canonical order.",
        "Reverse" => "Reverse[expr] reverses the order of elements in expr.",
        "Part" => "Part[expr, i] or expr[[i]] gives the i-th part of expr.",
        "Range" => {
            "Range[n] gives {1, 2, ..., n}.\nRange[min, max] gives {min, min+1, ..., max}.\nRange[min, max, step] uses the given step."
        }
        "Table" => {
            "Table[expr, n] generates a list of n copies of expr.\nTable[expr, {i, max}] evaluates expr for i from 1 to max.\nTable[expr, {i, min, max}] evaluates expr for i from min to max.\nTable[expr, {i, min, max, step}] uses the given step.\nTable[expr, {i, {val1, val2, ...}}] uses successive values from the list.\nTable[expr, {i, imin, imax}, {j, jmin, jmax}, ...] gives a nested list."
        }
        "Map" => "Map[f, expr] or f /@ expr applies f to each element at level 1 of expr.",
        "Fold" => {
            "Fold[f, x, list] or Fold[f, list] folds a function from the left.\nFold[f, x, list] starts with initial value x.\nFold[f, list] uses the first element of list as the initial value."
        }
        "Select" => "Select[list, crit] picks elements of list for which crit returns True.",
        "Scan" => "Scan[f, expr] evaluates f applied to each element of expr, returning Null.",
        "Nest" => "Nest[f, expr, n] applies f to expr n times.",
        "Take" => {
            "Take[list, n] gives the first n elements of list.\nTake[list, -n] gives the last n elements."
        }
        "Drop" => {
            "Drop[list, n] gives list with the first n elements removed.\nDrop[list, -n] removes the last n elements."
        }
        "Riffle" => "Riffle[list, x] inserts x between consecutive elements of list.",
        "Transpose" => "Transpose[list] transposes the first two levels of list.",
        "Total" => "Total[list] gives the total of all elements in list.",
        "Sum" => "Sum[expr, {i, min, max}] evaluates the sum of expr as i goes from min to max.",

        // ── Pattern ──
        "MatchQ" => "MatchQ[expr, pattern] returns True if expr matches pattern.",
        "Head" => "Head[expr] gives the head of expr (e.g., List for {1,2,3}).",
        "TypeOf" => "TypeOf[expr] returns the type name of expr as a string.",
        "FreeQ" => "FreeQ[expr, pattern] returns True if pattern does not appear in expr.",

        // ── String ──
        "StringJoin" => "StringJoin[s1, s2, ...] or s1 <> s2 <> ... concatenates strings.",
        "StringLength" => "StringLength[s] gives the number of characters in string s.",
        "ToString" => "ToString[expr] converts expr to a string representation.",
        "ToExpression" => "ToExpression[s] parses string s as Syma code and evaluates it.",
        "StringSplit" => "StringSplit[s] splits string s into a list of substrings.",
        "StringReplace" => "StringReplace[s, rules] applies string replacement rules.",
        "StringTake" => "StringTake[s, n] gives the first n characters of s.",
        "StringDrop" => "StringDrop[s, n] gives s with the first n characters removed.",
        "StringContainsQ" => "StringContainsQ[s, sub] returns True if s contains substring sub.",
        "StringReverse" => "StringReverse[s] reverses the characters in string s.",
        "ToUpperCase" => "ToUpperCase[s] converts string s to uppercase.",
        "ToLowerCase" => "ToLowerCase[s] converts string s to lowercase.",
        "Characters" => "Characters[s] gives a list of the characters in string s.",
        "StringMatchQ" => "StringMatchQ[s, pattern] returns True if s matches the string pattern.",
        "StringPadLeft" => "StringPadLeft[s, n] pads string s on the left to length n.",
        "StringPadRight" => "StringPadRight[s, n] pads string s on the right to length n.",
        "StringTrim" => "StringTrim[s] removes whitespace from the beginning and end of s.",
        "StringStartsQ" => "StringStartsQ[s, prefix] returns True if s starts with prefix.",
        "StringEndsQ" => "StringEndsQ[s, suffix] returns True if s ends with suffix.",

        // ── Math ──
        "Sin" => "Sin[z] gives the sine of z.",
        "Cos" => "Cos[z] gives the cosine of z.",
        "Tan" => "Tan[z] gives the tangent of z.",
        "Log" => {
            "Log[z] gives the natural logarithm of z (logarithm to base e).\nLog[b, z] gives the logarithm to base b."
        }
        "Exp" => "Exp[z] gives the exponential of z (e^z).",
        "Sqrt" => "Sqrt[z] gives the square root of z.",
        "Floor" => "Floor[x] gives the greatest integer less than or equal to x.",
        "Ceiling" => "Ceiling[x] gives the least integer greater than or equal to x.",
        "Round" => "Round[x] rounds x to the nearest integer.",
        "Max" => "Max[x, y, ...] gives the numerically largest of the arguments.",
        "Min" => "Min[x, y, ...] gives the numerically smallest of the arguments.",
        "ArcSin" => "ArcSin[z] gives the inverse sine of z.",
        "ArcCos" => "ArcCos[z] gives the inverse cosine of z.",
        "ArcTan" => "ArcTan[z] gives the inverse tangent of z.",
        "Csc" => "Csc[z] gives the cosecant of z (1/Sin[z]).",
        "Sec" => "Sec[z] gives the secant of z (1/Cos[z]).",
        "Cot" => "Cot[z] gives the cotangent of z (1/Tan[z]).",
        "ArcCsc" => "ArcCsc[z] gives the inverse cosecant of z.",
        "ArcSec" => "ArcSec[z] gives the inverse secant of z.",
        "ArcCot" => "ArcCot[z] gives the inverse cotangent of z.",
        "Haversine" => "Haversine[z] gives the haversine of z, (1 - Cos[z])/2.",
        "InverseHaversine" => {
            "InverseHaversine[z] gives the inverse haversine of z, 2 ArcSin[Sqrt[z]]."
        }
        "SinDegrees" => "SinDegrees[θ] gives the sine of θ degrees.",
        "CosDegrees" => "CosDegrees[θ] gives the cosine of θ degrees.",
        "TanDegrees" => "TanDegrees[θ] gives the tangent of θ degrees.",
        "CscDegrees" => "CscDegrees[θ] gives the cosecant of θ degrees.",
        "SecDegrees" => "SecDegrees[θ] gives the secant of θ degrees.",
        "CotDegrees" => "CotDegrees[θ] gives the cotangent of θ degrees.",
        "ArcSinDegrees" => "ArcSinDegrees[z] gives the inverse sine of z in degrees.",
        "ArcCosDegrees" => "ArcCosDegrees[z] gives the inverse cosine of z in degrees.",
        "ArcTanDegrees" => "ArcTanDegrees[z] gives the inverse tangent of z in degrees.",
        "ArcCscDegrees" => "ArcCscDegrees[z] gives the inverse cosecant of z in degrees.",
        "ArcSecDegrees" => "ArcSecDegrees[z] gives the inverse secant of z in degrees.",
        "ArcCotDegrees" => "ArcCotDegrees[z] gives the inverse cotangent of z in degrees.",
        "Log2" => "Log2[z] gives the base-2 logarithm of z.",
        "Log10" => "Log10[z] gives the base-10 logarithm of z.",
        "Mod" => "Mod[m, n] gives the remainder when m is divided by n.",
        "GCD" => "GCD[n1, n2, ...] gives the greatest common divisor of the arguments.",
        "LCM" => "LCM[n1, n2, ...] gives the least common multiple of the arguments.",
        "Factorial" => "Factorial[n] or n! gives the factorial of n.",

        // ── Symbolic ──
        "Simplify" => "Simplify[expr] attempts to simplify expr. (Currently a pass-through.)",
        "Expand" => "Expand[expr] expands products and powers in expr. (Currently a pass-through.)",
        "D" => "D[f, x] gives the partial derivative of f with respect to x. (Planned.)",
        "Integrate" => "Integrate[f, x] computes the indefinite integral of f. (Planned.)",
        "Factor" => "Factor[expr] factors the polynomial expr. (Planned.)",
        "Solve" => "Solve[eqns, vars] solves equations for variables. (Planned.)",
        "Series" => "Series[expr, {x, x0, n}] computes a power series expansion. (Planned.)",

        // ── Control ──
        "FixedPoint" => {
            "FixedPoint[f, expr] applies f repeatedly until the result no longer changes.\nFixedPoint[f, expr, n] performs at most n iterations."
        }

        // ── Package loading ──
        "Needs" => {
            "Needs[\"PackageName\"] loads a standard library package and makes its symbols available.\n\
             Built-in packages: LinearAlgebra, Statistics, Graphics.\n\
             Returns Null if the package is already loaded."
        }

        // ── LinearAlgebra ──
        "Dimensions" => {
            "Dimensions[m] gives the dimensions of a matrix or vector as a list {rows, cols}."
        }
        "Dot" => "Dot[a, b] or a . b computes the dot product of vectors or matrix multiplication.",
        "MatrixMultiply" => "MatrixMultiply[a, b] is an alias for Dot[a, b].",
        "IdentityMatrix" => "IdentityMatrix[n] gives the n×n identity matrix.",
        "Det" => "Det[m] computes the determinant of a square matrix.",
        "Inverse" => "Inverse[m] computes the inverse of a square matrix.",
        "Tr" => "Tr[m] gives the trace (sum of diagonal elements) of a matrix.",
        "Norm" => "Norm[v] gives the Euclidean norm of a vector or Frobenius norm of a matrix.",
        "Cross" => "Cross[a, b] computes the cross product of two 3D vectors.",
        "LinearSolve" => "LinearSolve[A, b] solves the linear system A·x = b for x.",
        "Eigenvalues" => "Eigenvalues[m] gives the eigenvalues of matrix m. (Symbolic stub.)",
        "MatrixPower" => "MatrixPower[m, n] gives the n-th matrix power of m.",
        "ArrayFlatten" => {
            "ArrayFlatten[{{m11, m12}, {m21, m22}}] flattens a matrix of matrices into a single matrix."
        }

        // ── Statistics ──
        "Mean" => "Mean[list] gives the arithmetic mean of the elements in list.",
        "Median" => "Median[list] gives the median of the elements in list.",
        "Variance" => {
            "Variance[list] gives the sample variance (with Bessel's correction, n-1 denominator)."
        }
        "StandardDeviation" => "StandardDeviation[list] gives the sample standard deviation.",
        "Quantile" => "Quantile[list, q] gives the q-th quantile of list (0 ≤ q ≤ 1).",
        "Covariance" => "Covariance[list1, list2] gives the sample covariance of two lists.",
        "Correlation" => {
            "Correlation[list1, list2] gives the Pearson correlation coefficient of two lists."
        }
        "RandomVariate" => {
            "RandomVariate[dist, n] generates n random values from distribution dist."
        }
        "NormalDistribution" => {
            "NormalDistribution[μ, σ] represents a normal distribution with mean μ and standard deviation σ."
        }
        "UniformDistribution" => {
            "UniformDistribution[min, max] represents a uniform distribution on [min, max]."
        }
        "PoissonDistribution" => {
            "PoissonDistribution[λ] represents a Poisson distribution with rate λ."
        }
        "GeometricMean" => "GeometricMean[list] gives the geometric mean of the elements in list.",
        "HarmonicMean" => "HarmonicMean[list] gives the harmonic mean of the elements in list.",
        "Skewness" => "Skewness[list] gives the skewness of the elements in list.",
        "Kurtosis" => "Kurtosis[list] gives the excess kurtosis of the elements in list.",
        "BinCounts" => "BinCounts[list, width] counts elements in bins of the given width.",
        "HistogramList" => {
            "HistogramList[list, n] gives {binEdges, counts} for n equal-width bins."
        }

        // ── Graphics ──
        "Plot" => {
            "Plot[f, {x, xmin, xmax}] plots f as a function of x from xmin to xmax.\n\
             Options: ImageSize → {width, height}, Axes → True, PlotRange → {ymin, ymax}."
        }
        "ListPlot" => {
            "ListPlot[data] plots a list of points.\n\
             ListPlot[{{x1,y1}, {x2,y2}, ...}] plots (x,y) pairs.\n\
             ListPlot[{y1, y2, ...}] plots points at x = 1, 2, ...."
        }
        "ListLinePlot" => {
            "ListLinePlot[data] plots data as connected line segments.\n\
             Accepts the same formats as ListPlot."
        }
        "ExportGraphics" => "ExportGraphics[path, svg] writes an SVG string to the file at path.",
        "Graphics" => "Graphics[primitives, options] wraps graphical primitives for rendering.",
        "Show" => "Show[graphics, options] displays graphics with updated options.",
        "Line" => {
            "Line[{{x1,y1}, {x2,y2}, ...}] represents a line primitive connecting the given points."
        }
        "Point" => "Point[{x, y}] represents a point primitive at the given coordinates.",
        "Circle" => "Circle[{cx, cy}, r] represents a circle primitive with center and radius.",
        "Rectangle" => "Rectangle[{xmin, ymin}, {xmax, ymax}] represents a rectangle primitive.",
        "RGBColor" => "RGBColor[r, g, b] specifies a color with red, green, blue components (0–1).",
        "Hue" => {
            "Hue[h] specifies a color with hue h (0–1), saturation and brightness default to 1."
        }
        "Thickness" => "Thickness[t] specifies line thickness.",
        "PointSize" => "PointSize[r] specifies point radius.",
        "Opacity" => "Opacity[a] specifies opacity (0 = transparent, 1 = opaque).",
        "Directive" => "Directive[style1, style2, ...] combines multiple graphics directives.",

        // ── I/O ──
        "Print" => "Print[expr] prints expr followed by a newline to standard output.",
        "Input" => "Input[] reads a line of input from the user.",
        "Write" => "Write[stream, expr] writes expr to an output stream.",
        "WriteLine" => "WriteLine[stream, s] writes string s followed by a newline.",
        "PrintF" => "PrintF[fmt, args...] prints formatted output.",
        "WriteString" => {
            "WriteString[path, data] writes the string data to the file at path, creating or overwriting it."
        }
        "ReadString" => {
            "ReadString[path] reads the entire file at path and returns it as a string."
        }
        "Export" => {
            "Export[path, data] exports data to a file.\n\
             Format is detected by extension:\n\
             1. `.json` — serialises the value to JSON.\n\
             2. Everything else — writes data.to_string() as plain text."
        }
        "Import" => {
            "Import[path] imports data from a file.\n\
             Format is detected by extension:\n\
             1. `.json` — parses JSON into a Syma value.\n\
             2. Everything else — returns the file contents as a string."
        }

        // ── Error handling ──
        "Throw" => "Throw[expr] throws expr as an exception, to be caught by Catch.",
        "Error" => "Error[msg] raises an error with the given message.",

        // ── Association ──
        "Keys" => "Keys[assoc] gives a list of the keys in an association.",
        "Values" => "Values[assoc] gives a list of the values in an association.",
        "Lookup" => {
            "Lookup[assoc, key] gives the value associated with key, or Missing if not found."
        }
        "KeyExistsQ" => "KeyExistsQ[assoc, key] returns True if key exists in the association.",

        // ── Random ──
        "RandomInteger" => {
            "RandomInteger[n] gives a random integer between 0 and n.\nRandomInteger[{min, max}] gives a random integer between min and max."
        }
        "RandomReal" => {
            "RandomReal[] gives a random real number between 0 and 1.\nRandomReal[{min, max}] gives a random real between min and max."
        }
        "RandomChoice" => "RandomChoice[list] gives a pseudorandom element from list.",

        // ── Constants ──
        "Pi" => {
            "Pi is the constant \u{03c0} (3.14159...), the ratio of a circle's circumference to its diameter."
        }
        "E" => "E is Euler's number e (2.71828...), the base of the natural logarithm.",
        "I" => "I is the imaginary unit, satisfying I^2 = -1.",
        "Degree" => {
            "Degree is the constant Pi/180, used to convert degrees to radians. E.g., 30 Degree = Pi/6."
        }
        "Null" => "Null represents the absence of an expression or result.",
        "True" => "True represents the logical value true.",
        "False" => "False represents the logical value false.",

        // ── Control flow ──
        "If" => {
            "If[cond, t, f] evaluates t if cond is True, f if False.\nIf[cond, t] evaluates t if cond is True, returns Null otherwise."
        }
        "Which" => {
            "Which[test1, val1, test2, val2, ...] evaluates each test in order and returns the val corresponding to the first True test."
        }
        "Switch" => {
            "Switch[expr, form1, val1, form2, val2, ...] evaluates expr and returns the val matching the first matching form."
        }
        "For" => "For[init, test, step, body] executes a for loop.",
        "While" => "While[test, body] evaluates body while test is True.",
        "Do" => {
            "Do[expr, {i, max}] evaluates expr max times.\nDo[expr, {i, min, max}] evaluates expr for i from min to max."
        }
        "Function" => "Function[{params}, body] creates a pure function with named parameters.",
        "Hold" => "Hold[expr] prevents evaluation of expr.",
        "HoldComplete" => {
            "HoldComplete[expr] prevents evaluation and attribute processing of expr."
        }
        "Catch" => "Catch[expr] evaluates expr, returning any value passed to Throw.",
        "N" => {
            "N[expr] evaluates expr numerically.\nN[expr, prec] uses prec decimal digits of precision."
        }

        // ── Parallel ──
        "ParallelMap" => {
            "ParallelMap[f, list] applies f to each element of list in parallel, returning a list of results."
        }
        "ParallelTable" => {
            "ParallelTable[expr, {i, min, max}] evaluates expr for i from min to max in parallel.\n\
             ParallelTable[expr, {i, max}] evaluates expr for i from 1 to max in parallel."
        }
        "KernelCount" => "KernelCount returns the number of available parallel worker threads.",
        "LaunchKernels" => {
            "LaunchKernels[] returns the current kernel count.\nLaunchKernels[n] sets the number of parallel workers to n."
        }
        "CloseKernels" => "CloseKernels[] resets the parallel worker pool. Returns Null.",

        // ── Format/display ──
        "InputForm" => {
            "InputForm[expr] displays expr using infix notation (e.g., `a + b` instead of `Plus[a, b]`)."
        }
        "FullForm" => "FullForm[expr] displays expr in head[arg, ...] notation.",
        "Short" => {
            "Short[expr] displays expr with top-level truncation (shows at most 5 items).\nShort[expr, n] displays at most n top-level items."
        }
        "Shallow" => {
            "Shallow[expr] displays expr with limited nesting depth (default 3).\nShallow[expr, n] limits nesting to n levels."
        }
        "NumberForm" => {
            "NumberForm[expr, n] displays numbers with n significant digits."
        }
        "ScientificForm" => {
            "ScientificForm[expr, n] displays numbers in scientific notation with n significant digits."
        }
        "BaseForm" => {
            "BaseForm[expr, base] displays a number in the given base (2–36)."
        }
        "Grid" => "Grid[list] displays a 2D list as an aligned table grid.",
        "Defer" => "Defer[expr] displays expr in its original form. (Currently a display wrapper.)",
        "SyntaxQ" => {
            "SyntaxQ[\"expr\"] returns True if expr is valid Syma syntax, False otherwise. Performs lex + parse only (no evaluation)."
        }
        "SyntaxLength" => {
            "SyntaxLength[\"expr\"] returns the position of the first syntax error, or the length of the string if valid."
        }

        // ── File system ──
        "FileNameSplit" => {
            "FileNameSplit[\"path\"] splits a file name into a list of its components."
        }
        "FileNameJoin" => {
            "FileNameJoin[{\"comp1\", \"comp2\", ...}] joins path components into a file name."
        }
        "FileNameTake" => "FileNameTake[\"path\", n] gives the last n components of the path.",
        "FileNameDrop" => {
            "FileNameDrop[\"path\", n] gives the path with the last n components removed."
        }
        "FileBaseName" => "FileBaseName[\"path\"] gives the file name without its extension.",
        "FileExtension" => "FileExtension[\"path\"] gives the file extension (e.g., \"txt\").",
        "FileNameDepth" => "FileNameDepth[\"path\"] gives the number of path components.",
        "DirectoryName" => "DirectoryName[\"path\"] gives the directory portion of the path.",
        "ParentDirectory" => "ParentDirectory[\"path\"] gives the parent directory of the path.",
        "ExpandFileName" => "ExpandFileName[\"path\"] resolves the path to an absolute file name.",
        "FileExistsQ" => "FileExistsQ[\"path\"] returns True if the file exists.",
        "DirectoryQ" => "DirectoryQ[\"path\"] returns True if the path is an existing directory.",
        "FileNames" => {
            "FileNames[] lists files in the current directory.\nFileNames[\"pattern\"] lists files matching the glob pattern.\nFileNames[\"pattern\", {\"dir1\", \"dir2\"}] searches in the given directories."
        }

        _ => return None,
    })
}

/// Get known attributes for a built-in function.
pub fn get_attributes(name: &str) -> Vec<&'static str> {
    match name {
        "Plus" | "Times" | "Min" | "Max" => vec![
            "Flat",
            "Listable",
            "NumericFunction",
            "OneIdentity",
            "Orderless",
        ],
        "Power" => vec!["Listable", "NumericFunction"],
        "Divide" | "Minus" | "Abs" => vec!["Listable", "NumericFunction"],
        "Sin" | "Cos" | "Tan" | "Log" | "Exp" | "Sqrt" | "Floor" | "Ceiling" | "Round" => {
            vec!["Listable", "NumericFunction"]
        }
        "ArcSin" | "ArcCos" | "ArcTan" | "Log2" | "Log10" => vec!["Listable", "NumericFunction"],
        "Csc" | "Sec" | "Cot" | "ArcCsc" | "ArcSec" | "ArcCot" => {
            vec!["Listable", "NumericFunction"]
        }
        "Haversine" | "InverseHaversine" => vec!["Listable", "NumericFunction"],
        "SinDegrees" | "CosDegrees" | "TanDegrees" | "CscDegrees" | "SecDegrees" | "CotDegrees" => {
            vec!["Listable", "NumericFunction"]
        }
        "ArcSinDegrees" | "ArcCosDegrees" | "ArcTanDegrees" | "ArcCscDegrees" | "ArcSecDegrees"
        | "ArcCotDegrees" => vec!["Listable", "NumericFunction"],
        "Factorial" => vec!["Listable"],
        "And" | "Or" => vec!["Flat", "HoldAll", "Listable", "OneIdentity", "Orderless"],
        "Not" => vec!["Listable"],
        "Hold" => vec!["HoldAll"],
        "HoldComplete" => vec!["HoldAllComplete"],
        "Defer" => vec!["HoldAll"],
        _ => vec![],
    }
}
