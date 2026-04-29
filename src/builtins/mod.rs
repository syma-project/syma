pub mod algebraic;
pub mod arithmetic;
pub mod association;
pub mod attributes;
pub mod calendar;
pub mod charting;
pub mod clearing;
pub mod combinatorics;
pub mod comparison;
pub mod dataset;
pub mod developer;
pub mod discrete;
pub mod domains;
pub mod error;
pub mod expression;
pub mod ffi;
pub mod filesystem;
pub mod format;
pub mod graphics;
pub mod help;
pub mod image;
pub mod integration;
pub mod io;
pub mod linalg;
pub mod list;
pub mod localsymbol;
pub mod logical;
pub mod math;
pub mod names;
pub mod noncommutative;
pub mod number_theory;
pub mod numericsolve;
pub mod operators;
pub mod parallel;
pub mod pattern;
pub mod random;
pub mod specialfunctions;
pub mod statistics;
pub mod string;
pub mod symbolic;
pub mod symbolicmanip;
pub mod systeminfo;

pub use attributes::get_attributes;
pub use help::get_help;

use crate::env::{Env, Fixity, LazyProvider, OperatorInfo};
use crate::value::{BuiltinFn, EvalError, Value};
use rug::Float;
use rug::Integer;
use std::collections::HashMap;
use std::sync::Arc;

// ── Shared helpers ──────────────────────────────────────────────────────────

/// Convert a `Value` to `f64`. Returns `None` for non-numeric values.
pub fn to_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        Value::Rational(r) => Some(r.to_f64()),
        Value::Complex { re, im: 0.0 } => Some(*re),
        _ => None,
    }
}

/// Create a `Value::Real` from an `f64`.
pub fn real(v: f64) -> Value {
    Value::Real(Float::with_val(crate::value::DEFAULT_PRECISION, v))
}

/// Check that `args` has exactly `expected` elements.
pub fn require_args(name: &str, args: &[Value], expected: usize) -> Result<(), EvalError> {
    if args.len() != expected {
        return Err(EvalError::Error(format!(
            "{} requires exactly {} argument{}",
            name,
            expected,
            if expected == 1 { "" } else { "s" }
        )));
    }
    Ok(())
}

/// Check that `args` has at least `min` elements.
pub fn require_min_args(name: &str, args: &[Value], min: usize) -> Result<(), EvalError> {
    if args.len() < min {
        return Err(EvalError::Error(format!(
            "{} requires at least {} argument{}",
            name,
            min,
            if min == 1 { "" } else { "s" }
        )));
    }
    Ok(())
}

/// Extract an `f64` from a `Value`, returning a `TypeError` if it is not numeric.
pub fn require_f64(v: &Value, name: &str, pos: usize) -> Result<f64, EvalError> {
    to_f64(v).ok_or_else(|| EvalError::TypeError {
        expected: "a number".to_string(),
        got: format!("{} at argument {} of {}", v.type_name(), pos, name),
    })
}

/// Register all built-in functions in the environment.
pub fn register_builtins(env: &Env) {
    // ── Arithmetic ──
    register_builtin(env, "Plus", arithmetic::builtin_plus);
    register_builtin(env, "Times", arithmetic::builtin_times);
    register_builtin(env, "Power", arithmetic::builtin_power);
    register_builtin(env, "Divide", arithmetic::builtin_divide);
    register_builtin(env, "Minus", arithmetic::builtin_minus);
    register_builtin(env, "Abs", arithmetic::builtin_abs);
    register_builtin(env, "Re", arithmetic::builtin_re);
    register_builtin(env, "Im", arithmetic::builtin_im);
    register_builtin(env, "ReIm", arithmetic::builtin_reim);
    register_builtin(env, "Conjugate", arithmetic::builtin_conjugate);
    register_builtin(env, "Arg", arithmetic::builtin_arg);
    register_builtin(env, "Sign", arithmetic::builtin_sign);
    register_builtin(env, "AbsArg", arithmetic::builtin_abs_arg);
    register_builtin(env, "Complex", arithmetic::builtin_complex);
    register_builtin(env, "ComplexQ", arithmetic::builtin_complex_q);

    // ── Noncommutative Algebra ──
    register_builtin(
        env,
        "NonCommutativeMultiply",
        noncommutative::builtin_nc_multiply,
    );
    register_builtin(env, "Commutator", noncommutative::builtin_commutator);
    register_builtin(
        env,
        "Anticommutator",
        noncommutative::builtin_anticommutator,
    );

    // ── Comparison ──
    register_builtin(env, "Equal", comparison::builtin_equal);
    register_builtin(env, "Unequal", comparison::builtin_unequal);
    register_builtin(env, "Less", comparison::builtin_less);
    register_builtin(env, "Greater", comparison::builtin_greater);
    register_builtin(env, "LessEqual", comparison::builtin_less_equal);
    register_builtin(env, "GreaterEqual", comparison::builtin_greater_equal);

    // ── Logical ──
    register_builtin_env(env, "And", logical::builtin_and);
    register_builtin_env(env, "Or", logical::builtin_or);
    register_builtin(env, "Not", logical::builtin_not);
    register_builtin_env(env, "Implies", logical::builtin_implies);
    register_builtin(env, "Xor", logical::builtin_xor);
    register_builtin(env, "Nand", logical::builtin_nand);
    register_builtin(env, "Nor", logical::builtin_nor);
    register_builtin(env, "Equivalent", logical::builtin_equivalent);
    register_builtin(env, "Majority", logical::builtin_majority);
    register_builtin(env, "Boole", logical::builtin_boole);
    register_builtin(env, "BooleanQ", logical::builtin_boolean_q);

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
    register_builtin(env, "Product", list::builtin_product);
    // Extended list operations
    register_builtin(env, "Partition", list::builtin_partition);
    register_builtin(env, "Split", list::builtin_split);
    register_builtin(env, "Gather", list::builtin_gather);
    register_builtin(env, "DeleteDuplicates", list::builtin_delete_duplicates);
    register_builtin(env, "Insert", list::builtin_insert);
    register_builtin(env, "Delete", list::builtin_delete);
    register_builtin(env, "ReplacePart", list::builtin_replace_part);
    register_builtin(env, "RotateLeft", list::builtin_rotate_left);
    register_builtin(env, "RotateRight", list::builtin_rotate_right);
    register_builtin(env, "Ordering", list::builtin_ordering);
    register_builtin(env, "ConstantArray", list::builtin_constant_array);
    register_builtin(env, "Diagonal", list::builtin_diagonal);
    register_builtin(env, "Accumulate", list::builtin_accumulate);
    register_builtin(env, "Differences", list::builtin_differences);
    register_builtin_env(env, "Array", list::builtin_array);
    register_builtin_env(env, "SplitBy", list::builtin_split_by);
    register_builtin_env(env, "GatherBy", list::builtin_gather_by);
    register_builtin_env(env, "FoldList", list::builtin_fold_list);
    register_builtin_env(env, "NestList", list::builtin_nest_list);
    register_builtin_env(env, "Apply", list::builtin_apply);
    register_builtin_env(env, "AllApply", list::builtin_all_apply);
    register_builtin_env(env, "MapAt", list::builtin_map_at);
    register_builtin(env, "ApplyTo", list::builtin_apply_to);
    register_builtin(env, "Thread", list::builtin_thread);
    register_builtin_env(env, "Outer", list::builtin_outer);
    register_builtin_env(env, "Inner", list::builtin_inner);
    register_builtin_env(env, "MapIndexed", list::builtin_map_indexed);
    register_builtin_env(env, "MapThread", list::builtin_map_thread);
    register_builtin_env(env, "NestWhile", list::builtin_nest_while);
    register_builtin_env(env, "NestWhileList", list::builtin_nest_while_list);
    register_builtin_env(env, "FixedPointList", list::builtin_fixed_point_list);

    // ── Rule application ──
    register_builtin_env(env, "ReplaceAll", crate::eval::rules::builtin_replace_all);
    register_builtin_env(
        env,
        "ReplaceRepeated",
        crate::eval::rules::builtin_replace_repeated,
    );

    // ── Pattern ──
    register_builtin_env(env, "MatchQ", pattern::builtin_match_q);
    register_builtin(env, "Head", pattern::builtin_head);
    register_builtin(env, "TypeOf", pattern::builtin_type_of);
    register_builtin_env(env, "FreeQ", pattern::builtin_free_q);
    register_builtin_env(env, "Cases", pattern::builtin_cases);
    register_builtin_env(env, "DeleteCases", pattern::builtin_delete_cases);
    register_builtin(env, "Dispatch", pattern::builtin_dispatch);

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

    // ── Message system ──
    register_builtin(env, "Message", builtin_message);
    register_builtin(env, "MessageName", builtin_message_name);

    // ── Association ──
    register_builtin(env, "Keys", association::builtin_keys);
    register_builtin(env, "Values", association::builtin_values);
    register_builtin(env, "Lookup", association::builtin_lookup);
    register_builtin(env, "KeyExistsQ", association::builtin_key_member_q);
    register_builtin(env, "AssociationQ", association::builtin_association_q);
    register_builtin(env, "Normal", association::builtin_normal);
    register_builtin(env, "KeySort", association::builtin_key_sort);
    register_builtin_env(env, "KeySortBy", association::builtin_key_sort_by);
    register_builtin(env, "KeyTake", association::builtin_key_take);
    register_builtin(env, "KeyDrop", association::builtin_key_drop);
    register_builtin_env(env, "KeySelect", association::builtin_key_select);
    register_builtin_env(env, "KeyMap", association::builtin_key_map);
    register_builtin_env(env, "KeyValueMap", association::builtin_key_value_map);
    register_builtin(env, "KeyMemberQ", association::builtin_key_member_q);
    register_builtin(env, "KeyFreeQ", association::builtin_key_free_q);
    register_builtin(env, "AssociateTo", association::builtin_associate_to);
    register_builtin(env, "KeyDropFrom", association::builtin_key_drop_from);
    register_builtin(env, "Counts", association::builtin_counts);
    register_builtin_env(env, "CountsBy", association::builtin_counts_by);
    register_builtin_env(env, "GroupBy", association::builtin_group_by);
    register_builtin_env(env, "Merge", association::builtin_merge);
    register_builtin(env, "KeyUnion", association::builtin_key_union);
    register_builtin(
        env,
        "KeyIntersection",
        association::builtin_key_intersection,
    );
    register_builtin(env, "KeyComplement", association::builtin_key_complement);

    // ── Dataset ──
    register_builtin(env, "Dataset", dataset::builtin_dataset);
    register_builtin(env, "DatasetQ", dataset::builtin_dataset_q);
    register_builtin_env(env, "SortBy", dataset::builtin_sort_by);
    register_builtin(env, "JoinAcross", dataset::builtin_join_across);

    // ── Symbolic ──
    register_builtin(env, "Simplify", symbolic::builtin_simplify);
    register_builtin(env, "Expand", symbolic::builtin_expand);
    register_builtin_env(env, "D", symbolic::builtin_d);
    register_builtin(env, "Factor", symbolic::builtin_factor);
    register_builtin(env, "Solve", symbolic::builtin_solve);
    register_builtin_env(env, "Series", symbolic::builtin_series);
    register_builtin_env(env, "Integrate", symbolic::builtin_integrate);

    // ── Symbolic Manipulation ──
    register_builtin(env, "Limit", symbolicmanip::builtin_limit);
    register_builtin(env, "Apart", symbolicmanip::builtin_apart);
    register_builtin(env, "Together", symbolicmanip::builtin_together);
    register_builtin(env, "Cancel", symbolicmanip::builtin_cancel);
    register_builtin(env, "Collect", symbolicmanip::builtin_collect);
    register_builtin(env, "NLimit", symbolicmanip::builtin_nlimit);

    // ── Integration Helpers (Rubi predicates and functions) ──
    // Comparison predicates
    register_builtin(env, "EqQ", integration::builtin_eq_q);
    register_builtin(env, "NeQ", integration::builtin_ne_q);
    register_builtin(env, "GtQ", integration::builtin_gt_q);
    register_builtin(env, "LtQ", integration::builtin_lt_q);
    register_builtin(env, "GeQ", integration::builtin_ge_q);
    register_builtin(env, "LeQ", integration::builtin_le_q);
    register_builtin(env, "IGtQ", integration::builtin_igt_q);
    register_builtin(env, "ILtQ", integration::builtin_ilt_q);
    register_builtin(env, "IGeQ", integration::builtin_ige_q);
    register_builtin(env, "ILeQ", integration::builtin_ile_q);
    // Sign predicates
    register_builtin(env, "PosQ", integration::builtin_pos_q);
    register_builtin(env, "NegQ", integration::builtin_neg_q);
    // Type predicates
    register_builtin(env, "TrueQ", integration::builtin_true_q);
    register_builtin(env, "FalseQ", integration::builtin_false_q);
    register_builtin(env, "OddQ", integration::builtin_odd_q);
    register_builtin(env, "HalfIntegerQ", integration::builtin_half_integer_q);
    register_builtin(env, "RationalQ", integration::builtin_rational_q);
    register_builtin(env, "IntegersQ", integration::builtin_integers_q);
    register_builtin(env, "PolyQ", integration::builtin_poly_q);
    register_builtin(env, "PolynomialQ", integration::builtin_polynomial_q);
    register_builtin(env, "AtomQ", integration::builtin_atom_q);
    // Core Rubi helpers
    register_builtin(env, "Subst", integration::builtin_subst);
    register_builtin(env, "SubstFor", integration::builtin_subst_for);
    register_builtin(env, "Unintegrable", integration::builtin_unintegrable);
    register_builtin(env, "ActivateTrig", integration::builtin_activate_trig);
    register_builtin(env, "DeactivateTrig", integration::builtin_deactivate_trig);
    register_builtin(env, "Simp", integration::builtin_simp);
    register_builtin(env, "Rt", integration::builtin_rt);
    register_builtin(env, "FracPart", integration::builtin_frac_part);
    register_builtin(env, "IntPart", integration::builtin_int_part);
    register_builtin(env, "Coefficient", integration::builtin_coefficient);
    register_builtin(env, "Coeff", integration::builtin_coeff);
    register_builtin(env, "FreeFactors", integration::builtin_free_factors);
    register_builtin(env, "NonfreeFactors", integration::builtin_nonfree_factors);
    // Expand helpers
    register_builtin(
        env,
        "ExpandIntegrand",
        integration::builtin_expand_integrand,
    );
    register_builtin(env, "ExpandToSum", integration::builtin_expand_to_sum);
    register_builtin(env, "ExpandTrig", integration::builtin_expand_trig);
    register_builtin(
        env,
        "ExpandTrigReduce",
        integration::builtin_expand_trig_reduce,
    );
    register_builtin(
        env,
        "ExpandTregExpand",
        integration::builtin_expand_trig_expand,
    );
    register_builtin(
        env,
        "ExpandTrigToExp",
        integration::builtin_expand_trig_to_exp,
    );
    register_builtin(
        env,
        "ExpandLinearProduct",
        integration::builtin_expand_linear_product,
    );
    register_builtin(
        env,
        "ExpandExpression",
        integration::builtin_expand_expression,
    );
    // Structural helpers
    register_builtin(env, "Dist", integration::builtin_dist);
    register_builtin(env, "Distrib", integration::builtin_distrib);
    register_builtin(env, "RemoveContent", integration::builtin_remove_content);
    // KnownIntegrand stubs
    register_builtin(
        env,
        "KnownSineIntegrandQ",
        integration::builtin_known_sine_integrand_q,
    );
    register_builtin(
        env,
        "KnownSecantIntegrandQ",
        integration::builtin_known_secant_integrand_q,
    );
    register_builtin(
        env,
        "KnownTangentIntegrandQ",
        integration::builtin_known_tangent_integrand_q,
    );
    register_builtin(
        env,
        "KnownCotangentIntegrandQ",
        integration::builtin_known_cotangent_integrand_q,
    );
    // Misc predicates
    register_builtin(env, "LinearQ", integration::builtin_linear_q);
    register_builtin(env, "SumQ", integration::builtin_sum_q);
    register_builtin(env, "NonsumQ", integration::builtin_nonsum_q);
    register_builtin(env, "Numerator", integration::builtin_numerator);
    register_builtin(env, "Denominator", integration::builtin_denominator);
    register_builtin(env, "Numer", integration::builtin_numer);
    register_builtin(env, "Denom", integration::builtin_denom);
    register_builtin(env, "Exponent", integration::builtin_exponent);
    register_builtin(env, "Sign", integration::builtin_sign);
    register_builtin(env, "PerfectSquareQ", integration::builtin_perfect_square_q);
    register_builtin(env, "BinomialQ", integration::builtin_binomial_q);
    register_builtin(env, "IntBinomialQ", integration::builtin_int_binomial_q);
    register_builtin(env, "QuadraticQ", integration::builtin_quadratic_q);
    register_builtin(env, "TrinomialQ", integration::builtin_trinomial_q);
    register_builtin(
        env,
        "PowerOfLinearQ",
        integration::builtin_power_of_linear_q,
    );
    register_builtin(env, "FunctionOfQ", integration::builtin_function_of_q);
    register_builtin(env, "TrigQ", integration::builtin_trig_q);
    register_builtin(env, "HyperbolicQ", integration::builtin_hyperbolic_q);
    register_builtin(
        env,
        "InverseFunctionQ",
        integration::builtin_inverse_function_q,
    );
    register_builtin(env, "PowerQ", integration::builtin_power_q);
    register_builtin(env, "ProductQ", integration::builtin_product_q);
    register_builtin(
        env,
        "RationalFunctionQ",
        integration::builtin_rational_function_q,
    );
    register_builtin(env, "ComplexFreeQ", integration::builtin_complex_free_q);
    register_builtin(env, "CalculusFreeQ", integration::builtin_calculus_free_q);
    register_builtin(env, "IntegralFreeQ", integration::builtin_integral_free_q);
    register_builtin(
        env,
        "InverseFunctionFreeQ",
        integration::builtin_inverse_function_free_q,
    );
    register_builtin(env, "InertTrigQ", integration::builtin_inert_trig_q);
    register_builtin(
        env,
        "InertTrigFreeQ",
        integration::builtin_inert_trig_free_q,
    );
    register_builtin(
        env,
        "AlgebraicFunctionQ",
        integration::builtin_algebraic_function_q,
    );
    register_builtin(env, "IndependentQ", integration::builtin_independent_q);
    register_builtin(env, "PolynomialInQ", integration::builtin_polynomial_in_q);
    // Comparison stubs
    register_builtin(env, "SimplerQ", integration::builtin_simpler_q);
    register_builtin(env, "SimplerSqrtQ", integration::builtin_simpler_sqrt_q);
    register_builtin(env, "SumSimplerQ", integration::builtin_sum_simpler_q);
    register_builtin(env, "NiceSqrtQ", integration::builtin_nice_sqrt_q);
    register_builtin(
        env,
        "DerivativeDivides",
        integration::builtin_derivative_divides,
    );
    // With/Module/If
    register_builtin_env(env, "With", integration::builtin_with);
    register_builtin_env(env, "Module", integration::builtin_module);
    register_builtin_env(env, "If", integration::builtin_if);
    // Additional stubs for Rubi rules
    register_builtin(env, "Discriminant", integration::builtin_discriminant);
    register_builtin(env, "LinearMatchQ", integration::builtin_linear_match_q);
    register_builtin(env, "IntLinearQ", integration::builtin_int_linear_q);
    register_builtin(env, "IntQuadraticQ", integration::builtin_int_quadratic_q);
    register_builtin(env, "Expon", integration::builtin_expon);
    register_builtin(
        env,
        "QuadraticMatchQ",
        integration::builtin_quadratic_match_q,
    );
    register_builtin(env, "BinomialMatchQ", integration::builtin_binomial_match_q);
    register_builtin(
        env,
        "PowerOfLinearMatchQ",
        integration::builtin_power_of_linear_match_q,
    );
    register_builtin(
        env,
        "NormalizePowerOfLinear",
        integration::builtin_normalize_power_of_linear,
    );
    register_builtin(
        env,
        "NormalizeIntegrand",
        integration::builtin_normalize_integrand,
    );
    register_builtin(
        env,
        "FunctionOfLinear",
        integration::builtin_function_of_linear,
    );
    register_builtin(env, "FunctionOfLog", integration::builtin_function_of_log);
    register_builtin(
        env,
        "FunctionOfTrigOfLinearQ",
        integration::builtin_function_of_trig_of_linear_q,
    );
    register_builtin(
        env,
        "FunctionOfExponentialQ",
        integration::builtin_function_of_exponential_q,
    );
    register_builtin(
        env,
        "FunctionOfExponential",
        integration::builtin_function_of_exponential,
    );
    register_builtin(env, "FunctionExpand", integration::builtin_function_expand);
    register_builtin(
        env,
        "SimplifyIntegrand",
        integration::builtin_simplify_integrand,
    );
    register_builtin(env, "Integral", integration::builtin_integral);
    register_builtin(
        env,
        "CannotIntegrate",
        integration::builtin_cannot_integrate,
    );
    register_builtin(env, "ShowStep", integration::builtin_show_step);
    register_builtin(env, "IntHide", integration::builtin_int_hide);
    register_builtin(
        env,
        "PolynomialRemainder",
        integration::builtin_polynomial_remainder,
    );
    register_builtin(
        env,
        "PolynomialQuotient",
        integration::builtin_polynomial_quotient,
    );
    register_builtin(
        env,
        "PolynomialDivide",
        integration::builtin_polynomial_divide,
    );
    register_builtin(
        env,
        "RationalFunctionExpand",
        integration::builtin_rational_function_expand,
    );
    register_builtin(
        env,
        "GeneralizedTrinomialQ",
        integration::builtin_generalized_trinomial_q,
    );
    register_builtin(
        env,
        "GeneralizedBinomialQ",
        integration::builtin_generalized_binomial_q,
    );
    register_builtin(
        env,
        "GeneralizedBinomialMatchQ",
        integration::builtin_generalized_binomial_match_q,
    );
    register_builtin(
        env,
        "GeneralizedTrinomialMatchQ",
        integration::builtin_generalized_trinomial_match_q,
    );
    register_builtin(
        env,
        "GeneralizedTrinomialDegree",
        integration::builtin_generalized_trinomial_degree,
    );
    register_builtin(env, "BinomialParts", integration::builtin_binomial_parts);
    register_builtin(env, "BinomialDegree", integration::builtin_binomial_degree);
    register_builtin(
        env,
        "EulerIntegrandQ",
        integration::builtin_euler_integrand_q,
    );
    register_builtin(
        env,
        "PseudoBinomialPairQ",
        integration::builtin_pseudo_binomial_pair_q,
    );
    register_builtin(
        env,
        "QuotientOfLinearsQ",
        integration::builtin_quotient_of_linears_q,
    );
    register_builtin(
        env,
        "QuotientOfLinearsParts",
        integration::builtin_quotient_of_linears_parts,
    );
    register_builtin(
        env,
        "QuadraticProductQ",
        integration::builtin_quadratic_product_q,
    );
    register_builtin(env, "LinearPairQ", integration::builtin_linear_pair_q);
    register_builtin(
        env,
        "SubstForFractionalPowerOfLinear",
        integration::builtin_subst_for_fractional_power_of_linear,
    );
    register_builtin(
        env,
        "SubstForFractionalPowerQ",
        integration::builtin_subst_for_fractional_power_q,
    );
    register_builtin(
        env,
        "SubstForFractionalPowerOfQuotientOfLinears",
        integration::builtin_subst_for_fractional_power_of_quotient_of_linears,
    );
    register_builtin(
        env,
        "SubstForInverseFunction",
        integration::builtin_subst_for_inverse_function,
    );
    register_builtin(
        env,
        "InverseFunctionOfLinear",
        integration::builtin_inverse_function_of_linear,
    );
    register_builtin(
        env,
        "FunctionOfSquareRootOfQuadratic",
        integration::builtin_function_of_sqrt_of_quadratic,
    );
    register_builtin(
        env,
        "FunctionOfLinear",
        integration::builtin_function_of_linear_fn,
    );
    register_builtin(
        env,
        "TryPureTanSubst",
        integration::builtin_try_pure_tan_subst,
    );
    register_builtin(
        env,
        "SimplerIntegrandQ",
        integration::builtin_simpler_integrand_q,
    );
    register_builtin(
        env,
        "NormalizePseudoBinomial",
        integration::builtin_normalize_pseudo_binomial,
    );
    register_builtin(
        env,
        "PolynomialInSubst",
        integration::builtin_polynomial_in_subst,
    );
    register_builtin(
        env,
        "MinimumMonomialExponent",
        integration::builtin_minimum_monomial_exponent,
    );
    register_builtin(
        env,
        "PowerVariableExpn",
        integration::builtin_power_variable_expn,
    );
    register_builtin(
        env,
        "DistributeDegree",
        integration::builtin_distribute_degree,
    );
    register_builtin(env, "IntSum", integration::builtin_int_sum);
    register_builtin(env, "SplitProduct", integration::builtin_split_product);
    register_builtin(env, "EveryQ", integration::builtin_every_q);
    register_builtin(
        env,
        "RationalFunctionExponents",
        integration::builtin_rational_function_exponents,
    );

    // ── Discrete Calculus ──
    register_builtin(env, "DiscreteDelta", discrete::builtin_discrete_delta);
    register_builtin(env, "DiscreteShift", discrete::builtin_discrete_shift);
    register_builtin(env, "DiscreteRatio", discrete::builtin_discrete_ratio);
    register_builtin(env, "FactorialPower", discrete::builtin_factorial_power);
    register_builtin(env, "BernoulliB", discrete::builtin_bernoulli_b);
    register_builtin(env, "LinearRecurrence", discrete::builtin_linear_recurrence);
    register_builtin_env(env, "RSolve", discrete::builtin_rsolve);
    register_builtin(env, "RecurrenceTable", discrete::builtin_recurrence_table);

    // ── Combinatorics ──
    register_builtin(env, "Binomial", combinatorics::builtin_binomial);
    register_builtin(env, "Multinomial", combinatorics::builtin_multinomial);
    register_builtin(env, "Factorial2", combinatorics::builtin_factorial2);
    register_builtin(
        env,
        "AlternatingFactorial",
        combinatorics::builtin_alternating_factorial,
    );
    register_builtin(env, "Subfactorial", combinatorics::builtin_subfactorial);
    register_builtin(env, "Permutations", combinatorics::builtin_permutations);
    register_builtin(env, "Subsets", combinatorics::builtin_subsets);
    register_builtin(env, "Tuples", combinatorics::builtin_tuples);
    register_builtin(env, "Arrangements", combinatorics::builtin_arrangements);
    register_builtin(env, "StirlingS1", combinatorics::builtin_stirling_s1);
    register_builtin(env, "StirlingS2", combinatorics::builtin_stirling_s2);
    register_builtin(env, "LucasL", combinatorics::builtin_lucas_l);
    register_builtin(env, "Fibonacci", combinatorics::builtin_fibonacci);
    register_builtin(env, "CatalanNumber", combinatorics::builtin_catalan_number);
    register_builtin(
        env,
        "HarmonicNumber",
        combinatorics::builtin_harmonic_number,
    );
    register_builtin(env, "PartitionsP", combinatorics::builtin_partitions_p);
    register_builtin(env, "PartitionsQ", combinatorics::builtin_partitions_q);
    register_builtin(env, "BellB", combinatorics::builtin_bell_b);

    // ── Calendar / Date & Time ──
    register_builtin(env, "DateObject", calendar::builtin_date_object);
    register_builtin(env, "DateString", calendar::builtin_date_string);
    register_builtin(env, "DateList", calendar::builtin_date_list);
    register_builtin(env, "DatePlus", calendar::builtin_date_plus);
    register_builtin(env, "DateDifference", calendar::builtin_date_difference);
    register_builtin(env, "Now", calendar::builtin_now);
    register_builtin(env, "Today", calendar::builtin_today);
    register_builtin(env, "DayName", calendar::builtin_day_name);
    register_builtin(env, "AbsoluteTime", calendar::builtin_absolute_time);
    register_builtin(env, "LeapYearQ", calendar::builtin_leap_year_q);
    register_builtin(env, "DayCount", calendar::builtin_day_count);
    register_builtin(env, "MonthName", calendar::builtin_month_name);

    // ── Control (evaluator-dependent) ──
    register_builtin_env(env, "FixedPoint", math::builtin_fixed_point);

    // ── Package loading (evaluator-dependent) ──
    register_builtin_env(env, "Needs", builtin_needs);

    // ── Graphics (evaluator-dependent) ──
    register_builtin(env, "Plot", graphics::builtin_plot_stub);

    // ── Attributes ──
    register_builtin_env(env, "SetAttributes", symbolic::builtin_set_attributes);
    register_builtin_env(env, "Attributes", symbolic::builtin_attributes);
    register_builtin_env(env, "ClearAttributes", symbolic::builtin_clear_attributes);

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
    register_builtin(env, "Gamma", math::builtin_gamma);

    // ── Hyperbolic functions ──
    register_builtin(env, "Sinh", math::builtin_sinh);
    register_builtin(env, "Cosh", math::builtin_cosh);
    register_builtin(env, "Tanh", math::builtin_tanh);
    register_builtin(env, "Csch", math::builtin_csch);
    register_builtin(env, "Sech", math::builtin_sech);
    register_builtin(env, "Coth", math::builtin_coth);
    register_builtin(env, "ArcSinh", math::builtin_arcsinh);
    register_builtin(env, "ArcCosh", math::builtin_arccosh);
    register_builtin(env, "ArcTanh", math::builtin_arctanh);
    register_builtin(env, "ArcCsch", math::builtin_arccsch);
    register_builtin(env, "ArcSech", math::builtin_arcsech);
    register_builtin(env, "ArcCoth", math::builtin_arccoth);
    register_builtin(env, "Sinc", math::builtin_sinc);

    // ── Numerical / piecewise ──
    register_builtin(env, "IntegerPart", math::builtin_integer_part);
    register_builtin(env, "FractionalPart", math::builtin_fractional_part);
    register_builtin(env, "Sign", math::builtin_sign);
    register_builtin(env, "UnitStep", math::builtin_unit_step);
    register_builtin(env, "Clip", math::builtin_clip);
    register_builtin(env, "Rescale", math::builtin_rescale);
    register_builtin(env, "Quotient", math::builtin_quotient);
    register_builtin(env, "QuotientRemainder", math::builtin_quotient_remainder);
    register_builtin(env, "KroneckerDelta", math::builtin_kronecker_delta);
    register_builtin(env, "IntegerQ", math::builtin_integer_q);
    register_builtin(env, "EvenQ", math::builtin_even_q);
    register_builtin(env, "PositiveQ", math::builtin_positive_q);
    register_builtin(env, "NegativeQ", math::builtin_negative_q);
    register_builtin(env, "NonNegativeQ", math::builtin_non_negative_q);
    register_builtin(env, "ZeroQ", math::builtin_zero_q);
    register_builtin(env, "Chop", math::builtin_chop);
    register_builtin(env, "Unitize", math::builtin_unitize);
    register_builtin(env, "Ramp", math::builtin_ramp);
    register_builtin(env, "RealAbs", math::builtin_real_abs);
    register_builtin(env, "RealSign", math::builtin_real_sign);
    register_builtin(env, "LogisticSigmoid", math::builtin_logistic_sigmoid);
    register_builtin(env, "NumericalOrder", math::builtin_numerical_order);
    register_builtin(env, "UnitBox", math::builtin_unit_box);
    register_builtin(env, "UnitTriangle", math::builtin_unit_triangle);

    // ── Number theory ──
    register_builtin(env, "PrimeQ", number_theory::builtin_prime_q);
    register_builtin(env, "FactorInteger", number_theory::builtin_factor_integer);
    register_builtin(env, "Divisors", number_theory::builtin_divisors);
    register_builtin(env, "Prime", number_theory::builtin_prime);
    register_builtin(env, "PrimePi", number_theory::builtin_prime_pi);
    register_builtin(env, "NextPrime", number_theory::builtin_next_prime);
    register_builtin(env, "PowerMod", number_theory::builtin_power_mod);
    register_builtin(env, "EulerPhi", number_theory::builtin_euler_phi);
    register_builtin(env, "MoebiusMu", number_theory::builtin_moebius_mu);
    register_builtin(env, "DivisorSigma", number_theory::builtin_divisor_sigma);
    register_builtin(env, "Divisible", number_theory::builtin_divisible);
    register_builtin(env, "CoprimeQ", number_theory::builtin_coprime_q);
    register_builtin(env, "IntegerDigits", number_theory::builtin_integer_digits);
    register_builtin(
        env,
        "ModularInverse",
        number_theory::builtin_modular_inverse,
    );
    register_builtin(env, "PrimeOmega", number_theory::builtin_prime_omega);
    register_builtin(env, "PrimeNu", number_theory::builtin_prime_nu);
    register_builtin(env, "DigitCount", number_theory::builtin_digit_count);
    register_builtin(env, "JacobiSymbol", number_theory::builtin_jacobi_symbol);
    register_builtin(
        env,
        "ChineseRemainder",
        number_theory::builtin_chinese_remainder,
    );
    register_builtin(
        env,
        "MultiplicativeOrder",
        number_theory::builtin_multiplicative_order,
    );
    register_builtin(env, "PrimitiveRoot", number_theory::builtin_primitive_root);
    register_builtin(
        env,
        "PerfectNumberQ",
        number_theory::builtin_perfect_number_q,
    );
    register_builtin(
        env,
        "MangoldtLambda",
        number_theory::builtin_mangoldt_lambda,
    );
    register_builtin(
        env,
        "LiouvilleLambda",
        number_theory::builtin_liouville_lambda,
    );
    register_builtin_env(env, "DivisorSum", number_theory::builtin_divisor_sum);
    register_builtin(env, "PrimePowerQ", number_theory::builtin_prime_power_q);
    register_builtin(env, "SquareFreeQ", number_theory::builtin_square_free_q);
    register_builtin(env, "CompositeQ", number_theory::builtin_composite_q);
    register_builtin(env, "PerfectPowerQ", number_theory::builtin_perfect_power_q);
    register_builtin(
        env,
        "IntegerExponent",
        number_theory::builtin_integer_exponent,
    );
    register_builtin(env, "FromDigits", number_theory::builtin_from_digits);
    register_builtin(env, "ToDigits", number_theory::builtin_to_digits);
    register_builtin(
        env,
        "ContinuedFraction",
        number_theory::builtin_continued_fraction,
    );
    register_builtin(
        env,
        "FromContinuedFraction",
        number_theory::builtin_from_continued_fraction,
    );
    register_builtin(env, "NumberExpand", number_theory::builtin_number_expand);

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
    register_builtin_env(env, "MapApply", list::builtin_map_apply);

    // ── Functional Operators (operators.rs) ──
    register_builtin_env(env, "Composition", operators::builtin_composition);
    register_builtin_env(
        env,
        "RightComposition",
        operators::builtin_right_composition,
    );
    register_builtin_env(env, "Through", operators::builtin_through);
    register_builtin_env(env, "OperatorApply", operators::builtin_operator_apply);
    register_builtin_env(env, "Curry", operators::builtin_curry);
    register_builtin_env(env, "UnCurry", operators::builtin_uncurry);

    // ── Set / List utility (operators.rs) ──
    register_builtin(env, "SubsetQ", operators::builtin_subset_q);
    register_builtin(
        env,
        "SymmetricDifference",
        operators::builtin_symmetric_difference,
    );
    register_builtin_env(env, "SelectFirst", operators::builtin_select_first);
    register_builtin_env(env, "SelectLast", operators::builtin_select_last);
    register_builtin_env(env, "PositionFirst", operators::builtin_position_first);
    register_builtin_env(env, "PositionLast", operators::builtin_position_last);
    register_builtin_env(env, "Replace", operators::builtin_replace);
    register_builtin_env(env, "MapAll", operators::builtin_map_all);
    register_builtin(env, "Undulate", operators::builtin_undulate);
    register_builtin(env, "MovingAverage", list::builtin_moving_average);
    register_builtin_env(env, "BlockMap", list::builtin_block_map);
    register_builtin(env, "ListConvolve", list::builtin_list_convolve);
    register_builtin(env, "Nearest", list::builtin_nearest);
    register_builtin(env, "ArrayPad", list::builtin_array_pad);
    register_builtin(env, "ArrayReshape", list::builtin_array_reshape);
    register_builtin(env, "StringCases", list::builtin_string_cases);

    // ── I/O ──
    register_builtin(env, "Input", io::builtin_input);
    register_builtin(env, "Write", io::builtin_write);
    register_builtin(env, "WriteLine", io::builtin_write_line);
    register_builtin(env, "PrintF", io::builtin_printf);
    register_builtin(env, "WriteString", io::builtin_write_string);
    register_builtin(env, "ReadString", io::builtin_read_string);
    register_builtin(env, "Export", io::builtin_export);
    register_builtin(env, "Import", io::builtin_import);
    register_builtin(env, "ImportString", io::builtin_import_string);
    register_builtin(env, "ExportString", io::builtin_export_string);
    register_builtin(env, "ReadList", io::builtin_read_list);
    register_builtin(env, "FileRead", io::builtin_file_read);
    register_builtin(env, "FileWrite", io::builtin_file_write);
    register_builtin(env, "RunProcess", io::builtin_run_process);

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

    // ── Extended string (StringPart, StringPosition, etc.) ──
    register_builtin(env, "StringPart", string::builtin_string_part);
    register_builtin(env, "StringPosition", string::builtin_string_position);
    register_builtin(env, "StringCount", string::builtin_string_count);
    register_builtin(env, "StringRepeat", string::builtin_string_repeat);
    register_builtin(env, "StringDelete", string::builtin_string_delete);
    register_builtin(env, "StringInsert", string::builtin_string_insert);
    register_builtin(env, "StringRiffle", string::builtin_string_riffle);
    register_builtin(env, "StringFreeQ", string::builtin_string_free_q);
    register_builtin(env, "LetterQ", string::builtin_letter_q);
    register_builtin(env, "DigitQ", string::builtin_digit_q);
    register_builtin(env, "UpperCaseQ", string::builtin_upper_case_q);
    register_builtin(env, "LowerCaseQ", string::builtin_lower_case_q);
    register_builtin(env, "TextWords", string::builtin_text_words);
    register_builtin(env, "CharacterCounts", string::builtin_character_counts);
    register_builtin(env, "Alphabet", string::builtin_alphabet);
    register_builtin(env, "ToCharacterCode", string::builtin_to_character_code);
    register_builtin(
        env,
        "FromCharacterCode",
        string::builtin_from_character_code,
    );
    register_builtin(env, "EditDistance", string::builtin_edit_distance);
    register_builtin(
        env,
        "LongestCommonSubsequence",
        string::builtin_longest_common_subsequence,
    );
    register_builtin(
        env,
        "LongestCommonSubString",
        string::builtin_longest_common_sub_string,
    );
    register_builtin(env, "WordCount", string::builtin_word_count);
    register_builtin(env, "SentenceCount", string::builtin_sentence_count);

    // ── Parallel computation ──
    register_builtin_env(env, "ParallelMap", parallel::builtin_parallel_map);
    register_builtin(env, "ParallelTable", parallel::builtin_parallel_table);
    register_builtin(env, "ParallelSum", parallel::builtin_parallel_sum);
    register_builtin(env, "ParallelEvaluate", parallel::builtin_parallel_evaluate);
    register_builtin(env, "ParallelTry", parallel::builtin_parallel_try);
    register_builtin(env, "ParallelProduct", parallel::builtin_parallel_product);
    register_builtin(env, "ParallelDo", parallel::builtin_parallel_do);
    register_builtin_env(env, "ParallelCombine", parallel::builtin_parallel_combine);
    register_builtin(env, "LaunchKernels", parallel::builtin_launch_kernels);
    register_builtin(env, "CloseKernels", parallel::builtin_close_kernels);
    register_builtin(env, "KernelCount", parallel::builtin_kernel_count);
    register_builtin(env, "ProcessorCount", parallel::builtin_processor_count);
    register_builtin(env, "AbortKernels", parallel::builtin_abort_kernels);

    // ── FFI ──
    register_builtin_env(env, "LoadLibrary", ffi::builtin_load_library);
    register_builtin_env(env, "LoadExtension", ffi::builtin_load_extension);
    register_builtin_env(env, "ExternalEvaluate", ffi::builtin_external_evaluate);
    register_builtin_env(env, "LibraryFunction", ffi::builtin_library_function);
    register_builtin_env(
        env,
        "LibraryFunctionLoad",
        ffi::builtin_library_function_load,
    );

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

    // ── Symbol Names ──
    register_builtin_env(env, "Names", names::builtin_names);

    // ── Assumptions & Domains ──
    domains::register(env);

    // ── Symbol Clearing ──
    register_builtin_env(env, "Clear", clearing::builtin_clear);
    register_builtin_env(env, "ClearAll", clearing::builtin_clear_all);
    register_builtin_env(env, "Unset", clearing::builtin_unset);
    register_builtin_env(env, "Remove", clearing::builtin_remove);

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
    register_builtin(env, "TableForm", format::builtin_table_form);
    register_builtin(env, "MatrixForm", format::builtin_matrix_form);
    register_builtin(env, "PaddedForm", format::builtin_padded_form);
    register_builtin(env, "StringForm", format::builtin_string_form);

    // ── Persistent storage ──
    register_builtin(env, "LocalSymbol", localsymbol::builtin_local_symbol);

    // ── Sequence ──
    register_builtin(env, "Sequence", builtin_sequence);

    // ── Image Processing ──
    register_builtin(env, "Image", image::builtin_image);
    register_builtin(env, "ImageData", image::builtin_image_data);
    register_builtin(env, "ImageDimensions", image::builtin_image_dimensions);
    register_builtin(env, "ImageType", image::builtin_image_type);
    register_builtin(env, "ImageResize", image::builtin_image_resize);
    register_builtin(env, "ImageRotate", image::builtin_image_rotate);
    register_builtin(env, "ImageAdjust", image::builtin_image_adjust);
    register_builtin(env, "Binarize", image::builtin_binarize);
    register_builtin(env, "ColorConvert", image::builtin_color_convert);
    register_builtin(env, "GaussianFilter", image::builtin_gaussian_filter);
    register_builtin(env, "EdgeDetect", image::builtin_edge_detect);
    register_builtin(env, "ImageConvolve", image::builtin_image_convolve);

    // ── Constants (kept symbolic; use N[] for numerical evaluation) ──
    env.set("Pi".to_string(), Value::Symbol("Pi".to_string()));
    env.set("E".to_string(), Value::Symbol("E".to_string()));
    env.set("I".to_string(), Value::Complex { re: 0.0, im: 1.0 });
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
        linalg::SYMBOLS,
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
        graphics::SYMBOLS,
        graphics::SYMBOLS,
        "Graphics",
        graphics::register,
    );

    // Charting — symbols backed by Rust builtins in charting.rs
    register_lazy_package(
        env,
        charting::SYMBOLS,
        charting::SYMBOLS,
        "Charting",
        charting::register,
    );

    // -- Developer context --
    developer::register(env);

    // -- System information --
    systeminfo::register(env);

    // -- Algebraic Numbers --
    algebraic::register(env);

    // -- Special Functions --
    specialfunctions::register_sfs(env);

    // -- Numeric Solve / Optimization --
    register_builtin_env(env, "FindRoot", numericsolve::builtin_find_root);
    register_builtin_env(env, "FindMinimum", numericsolve::builtin_find_minimum);
    register_builtin_env(env, "FindMaximum", numericsolve::builtin_find_maximum);
    register_builtin_env(env, "NMinimize", numericsolve::builtin_nminimize);
    register_builtin_env(env, "NMaximize", numericsolve::builtin_nmaximize);
    register_builtin_env(env, "ArgMin", numericsolve::builtin_argmin);
    register_builtin_env(env, "ArgMax", numericsolve::builtin_argmax);
    register_builtin_env(env, "FindInstance", numericsolve::builtin_find_instance);
    register_builtin_env(env, "NSolve", numericsolve::builtin_nsolve);

    // -- Custom Notation --
    register_builtin_env(env, "Infix", builtin_infix);
    register_builtin_env(env, "Prefix", builtin_prefix);
    register_builtin_env(env, "Postfix", builtin_postfix);

    // ── Add SYMA_HOME/Packages and SystemFiles to module search path ──
    // SystemFiles/Kernel/ allows loading D.syma, Integrate.syma, etc.
    // from disk (user-overridable, no rebuild needed).
    // Packages/ allows `Needs["PackageName"]` to find pure-Syma packages.
    if let Some(syma_home) = std::env::var_os("SYMA_HOME") {
        let base = std::path::Path::new(&syma_home);
        let sysfiles = base.join("SystemFiles");
        if sysfiles.is_dir() {
            env.add_search_path(sysfiles);
        }
        let packages_dir = base.join("Packages");
        if packages_dir.is_dir() {
            env.add_search_path(packages_dir);
        }
    } else {
        // Fall back to ~/.syma/
        if let Some(home) = std::env::var_os("HOME") {
            let base = std::path::Path::new(&home).join(".syma");
            let sysfiles = base.join("SystemFiles");
            if sysfiles.is_dir() {
                env.add_search_path(sysfiles);
            }
            let packages_dir = base.join("Packages");
            if packages_dir.is_dir() {
                env.add_search_path(packages_dir);
            }
        }
    }

    // ── System limits ──
    env.set(
        "$RecursionLimit".to_string(),
        Value::Integer(Integer::from(1024)),
    );

    // ── Special forms (handled in eval_call, not registered as builtins) ──
    // Set attributes for forms that aren't registered via register_builtin
    for (name, attrs) in [
        ("Hold", &["HoldAll", "Locked", "ReadProtected"] as &[&str]),
        (
            "HoldComplete",
            &["HoldAllComplete", "Locked", "ReadProtected"],
        ),
        ("Defer", &["HoldAll", "Locked", "ReadProtected"]),
        ("Set", &["HoldFirst", "Locked", "ReadProtected"]),
        ("SetDelayed", &["HoldAll", "Locked", "ReadProtected"]),
        ("SetAttributes", &["HoldFirst", "Locked", "ReadProtected"]),
        ("ClearAttributes", &["HoldFirst", "Locked", "ReadProtected"]),
        ("Module", &["HoldAll", "Locked", "ReadProtected"]),
        ("With", &["HoldAll", "Locked", "ReadProtected"]),
        ("Block", &["HoldAll", "Locked", "ReadProtected"]),
        ("If", &["HoldAll", "Locked", "ReadProtected"]),
        ("ReleaseHold", &["SequenceHold", "Locked", "ReadProtected"]),
    ] {
        env.set_attributes(name, attrs.iter().map(|s| s.to_string()).collect());
    }
}

// ── Custom Notation builtins ──

/// Infix[op_String, head_Symbol, precedence_Integer]
/// Register a custom infix operator.
/// e.g. Infix["\u2295", CirclePlus, 180]
fn builtin_infix(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "Infix requires 2 or 3 arguments: Infix[op, head, precedence?]".to_string(),
        ));
    }
    let op_str = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::Error(
                "First argument to Infix must be a string".to_string(),
            ));
        }
    };
    let head = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::Error(
                "Second argument to Infix must be a symbol".to_string(),
            ));
        }
    };
    let precedence = if args.len() == 3 {
        match &args[2] {
            Value::Integer(n) => u32::try_from(n).unwrap_or(180),
            _ => {
                return Err(EvalError::Error(
                    "Third argument to Infix must be an integer".to_string(),
                ));
            }
        }
    } else {
        180
    };
    env.register_operator(
        &op_str,
        OperatorInfo {
            head,
            precedence,
            fixity: Fixity::Infix,
        },
    );
    Ok(Value::Null)
}

/// Prefix[op_String, head_Symbol]
/// Register a custom prefix operator.
/// e.g. Prefix["\u00ac", Not]
fn builtin_prefix(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Prefix requires 2 arguments: Prefix[op, head]".to_string(),
        ));
    }
    let op_str = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::Error(
                "First argument to Prefix must be a string".to_string(),
            ));
        }
    };
    let head = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::Error(
                "Second argument to Prefix must be a symbol".to_string(),
            ));
        }
    };
    env.register_operator(
        &op_str,
        OperatorInfo {
            head,
            precedence: 250,
            fixity: Fixity::Prefix,
        },
    );
    Ok(Value::Null)
}

/// Postfix[op_String, head_Symbol]
/// Register a custom postfix operator.
/// e.g. Postfix["!", Factorial]
fn builtin_postfix(args: &[Value], env: &Env) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Postfix requires 2 arguments: Postfix[op, head]".to_string(),
        ));
    }
    let op_str = match &args[0] {
        Value::Str(s) => s.clone(),
        _ => {
            return Err(EvalError::Error(
                "First argument to Postfix must be a string".to_string(),
            ));
        }
    };
    let head = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => {
            return Err(EvalError::Error(
                "Second argument to Postfix must be a symbol".to_string(),
            ));
        }
    };
    env.register_operator(
        &op_str,
        OperatorInfo {
            head,
            precedence: 300,
            fixity: Fixity::Postfix,
        },
    );
    Ok(Value::Null)
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
fn register_builtin_env(
    env: &Env,
    name: &str,
    func: fn(&[Value], &Env) -> Result<Value, EvalError>,
) {
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
                        locals: std::collections::HashMap::new(),
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
                locals: HashMap::new(),
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
                locals: std::collections::HashMap::new(),
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
                locals: std::collections::HashMap::new(),
            };
            env.register_module("Graphics".to_string(), module);
        }
        "Charting" => {
            crate::builtins::charting::register(env);
            let exports: HashMap<String, Value> = crate::builtins::charting::SYMBOLS
                .iter()
                .filter_map(|&sym| env.get(sym).map(|v| (sym.to_string(), v)))
                .collect();
            let module = Value::Module {
                name: "Charting".to_string(),
                exports,
                locals: std::collections::HashMap::new(),
            };
            env.register_module("Charting".to_string(), module);
        }
        "Developer" => {
            // Already registered eagerly during startup; just register the module.
            let exports: HashMap<String, Value> = crate::builtins::developer::SYMBOLS
                .iter()
                .filter_map(|&sym| env.get(sym).map(|v| (sym.to_string(), v)))
                .collect();
            let module = Value::Module {
                name: "Developer".to_string(),
                exports,
                locals: std::collections::HashMap::new(),
            };
            env.register_module("Developer".to_string(), module);
        }
        _ => {
            // Strip trailing backtick context marker ("Combinatorics`" -> "Combinatorics")
            let clean_name = pkg_name.trim_end_matches('`');
            let module_val = crate::eval::load_module_from_file(clean_name, env)?;
            // Import all exported symbols AND internal helpers into the current environment
            if let Value::Module {
                exports, locals, ..
            } = &module_val
            {
                for (sym, val) in exports {
                    env.set(sym.clone(), val.clone());
                }
                for (sym, val) in locals {
                    env.set(sym.clone(), val.clone());
                }
            }
            return Ok(module_val);
        }
    }
    Ok(Value::Null)
}

// ── Sequence ──

/// `Sequence[expr1, expr2, ...]` — wraps arguments into a sequence that
/// automatically splices into function calls.
///
/// Sequence objects are automatically flattened out in all functions except
/// those with attribute SequenceHold or HoldAllComplete.
/// `Sequence[]` evaporates entirely; `Sequence[expr]` acts like Identity.
fn builtin_sequence(args: &[Value]) -> Result<Value, EvalError> {
    Ok(Value::Sequence(args.to_vec()))
}

/// Re-export for use by eval.rs
pub use arithmetic::add_values_public;
pub use arithmetic::mul_values_public;
pub use arithmetic::sub_values_public;

// ── Message system ──

/// `Message[sym::tag, args...]` — emit a formatted message to stderr.
///
/// The first argument must be a `MessageName[sym, "tag"]` call (produced by
/// the `sym::tag` syntax).  Remaining arguments are substituted into the
/// message template at `` `1` ``, `` `2` `` etc.
fn builtin_message(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "Message requires at least 1 argument".to_string(),
        ));
    }
    // First arg should be MessageName[sym, "tag"] (produced by sym::tag syntax)
    let tag = match &args[0] {
        Value::Call {
            head,
            args: mn_args,
        } if head == "MessageName" && mn_args.len() == 2 => {
            let sym = extract_held_symbol_name(&mn_args[0]);
            let tag_str = match &mn_args[1] {
                Value::Str(s) => s.clone(),
                other => other.to_string(),
            };
            format!("{}::{}", sym, tag_str)
        }
        Value::Str(s) => s.clone(), // allow plain string key as convenience
        other => other.to_string(),
    };
    let fmt_args: Vec<String> = args[1..].iter().map(|v| v.to_string()).collect();
    crate::messages::emit(&tag, &fmt_args);
    Ok(Value::Null)
}

/// `MessageName[sym, "tag"]` — produces a symbolic message name object.
///
/// Returns `MessageName[sym, "tag"]` as a `Call` value for use with `Message`.
/// The symbol argument is held (not evaluated) due to the `HoldFirst` attribute.
fn builtin_message_name(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "MessageName requires exactly 2 arguments".to_string(),
        ));
    }
    // Extract the symbol name; it arrives as Value::Pattern(Expr::Symbol(...)) due to HoldFirst
    let sym_name = extract_held_symbol_name(&args[0]);
    let tag = match &args[1] {
        Value::Str(s) => s.clone(),
        other => other.to_string(),
    };
    // Return the symbolic MessageName form so Message can look it up
    Ok(Value::Call {
        head: "MessageName".to_string(),
        args: vec![Value::Symbol(sym_name), Value::Str(tag)],
    })
}

/// Extract the symbol name from a held or direct symbol value.
fn extract_held_symbol_name(v: &Value) -> String {
    match v {
        Value::Symbol(s) => s.clone(),
        Value::Pattern(crate::ast::Expr::Symbol(s)) => s.clone(),
        Value::Builtin(name, _) => name.clone(),
        other => other.to_string(),
    }
}
