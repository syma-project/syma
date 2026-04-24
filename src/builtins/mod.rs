pub mod arithmetic;
pub mod association;
pub mod comparison;
pub mod error;
pub mod ffi;
pub mod filesystem;
pub mod io;
pub mod list;
pub mod logical;
pub mod math;
pub mod parallel;
pub mod pattern;
pub mod random;
pub mod string;
pub mod symbolic;

use crate::env::Env;
use crate::value::{EvalError, Value};

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
    register_builtin(env, "Map", list::builtin_map);
    register_builtin(env, "Fold", list::builtin_fold);
    register_builtin(env, "Select", list::builtin_select);
    register_builtin(env, "Scan", list::builtin_scan);
    register_builtin(env, "Nest", list::builtin_nest);
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
    // Integrate is registered as a lazy provider that loads the Rubi
    // rule-based integration engine on first call.
    #[cfg(feature = "rubi")]
    crate::rubi::register(env);
    register_builtin(env, "Factor", symbolic::builtin_factor);
    register_builtin(env, "Solve", symbolic::builtin_solve);
    register_builtin(env, "Series", symbolic::builtin_series);

    // ── Control (evaluator-dependent) ──
    register_builtin(env, "FixedPoint", math::builtin_fixed_point_stub);

    // ── Attributes (evaluator-dependent) ──
    register_builtin(env, "SetAttributes", symbolic::builtin_set_attributes_stub);
    register_builtin(env, "Attributes", symbolic::builtin_attributes_stub);

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
    register_builtin(env, "ParallelMap", parallel::builtin_parallel_map);
    register_builtin(env, "ParallelTable", parallel::builtin_parallel_table);
    register_builtin(env, "LaunchKernels", parallel::builtin_launch_kernels);
    register_builtin(env, "CloseKernels", parallel::builtin_close_kernels);
    register_builtin(env, "KernelCount", parallel::builtin_kernel_count);

    // ── FFI (env-aware; stubs registered so Help/Head works) ──
    register_builtin(env, "LoadLibrary", ffi::builtin_load_library);
    register_builtin(env, "LoadExtension", ffi::builtin_load_extension);
    register_builtin(env, "ExternalEvaluate", ffi::builtin_external_evaluate);
    // LibraryFunction and LibraryFunctionLoad are handled as special cases in eval.rs.
    // Register stubs so they appear in the environment and can be looked up.
    register_builtin(env, "LibraryFunction", ffi::builtin_load_library);
    register_builtin(env, "LibraryFunctionLoad", ffi::builtin_load_library);

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
}

fn register_builtin(env: &Env, name: &str, func: fn(&[Value]) -> Result<Value, EvalError>) {
    env.set(name.to_string(), Value::Builtin(name.to_string(), func));
    let attrs = get_attributes(name);
    if !attrs.is_empty() {
        env.set_attributes(name, attrs.iter().map(|s| s.to_string()).collect());
    }
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
        _ => vec![],
    }
}
