pub mod arithmetic;
pub mod association;
pub mod clearing;
pub mod comparison;
pub mod dataset;
pub mod developer;
pub mod discrete;
pub mod domains;
pub mod error;
pub mod ffi;
pub mod filesystem;
pub mod format;
pub mod graphics;
pub mod image;
pub mod io;
pub mod integration;
pub mod linalg;
pub mod list;
pub mod localsymbol;
pub mod logical;
pub mod math;
pub mod names;
pub mod noncommutative;
pub mod number_theory;
pub mod parallel;
pub mod pattern;
pub mod random;
pub mod statistics;
pub mod string;
pub mod systeminfo;
pub mod symbolic;

use crate::env::{Env, Fixity, LazyProvider, OperatorInfo};
use crate::value::{BuiltinFn, EvalError, Value};
use rug::Integer;
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

    // ── Noncommutative Algebra ──
    register_builtin(env, "NonCommutativeMultiply", noncommutative::builtin_nc_multiply);
    register_builtin(env, "Commutator", noncommutative::builtin_commutator);
    register_builtin(env, "Anticommutator", noncommutative::builtin_anticommutator);

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
    register_builtin(env, "Integrate", symbolic::builtin_integrate);

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
    register_builtin(env, "ExpandIntegrand", integration::builtin_expand_integrand);
    register_builtin(env, "ExpandToSum", integration::builtin_expand_to_sum);
    register_builtin(env, "ExpandTrig", integration::builtin_expand_trig);
    register_builtin(env, "ExpandTrigReduce", integration::builtin_expand_trig_reduce);
    register_builtin(env, "ExpandTregExpand", integration::builtin_expand_trig_expand);
    register_builtin(env, "ExpandTrigToExp", integration::builtin_expand_trig_to_exp);
    register_builtin(env, "ExpandLinearProduct", integration::builtin_expand_linear_product);
    register_builtin(env, "ExpandExpression", integration::builtin_expand_expression);
    // Structural helpers
    register_builtin(env, "Dist", integration::builtin_dist);
    register_builtin(env, "Distrib", integration::builtin_distrib);
    register_builtin(env, "RemoveContent", integration::builtin_remove_content);
    // KnownIntegrand stubs
    register_builtin(env, "KnownSineIntegrandQ", integration::builtin_known_sine_integrand_q);
    register_builtin(env, "KnownSecantIntegrandQ", integration::builtin_known_secant_integrand_q);
    register_builtin(env, "KnownTangentIntegrandQ", integration::builtin_known_tangent_integrand_q);
    register_builtin(env, "KnownCotangentIntegrandQ", integration::builtin_known_cotangent_integrand_q);
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
    register_builtin(env, "PowerOfLinearQ", integration::builtin_power_of_linear_q);
    register_builtin(env, "FunctionOfQ", integration::builtin_function_of_q);
    register_builtin(env, "TrigQ", integration::builtin_trig_q);
    register_builtin(env, "HyperbolicQ", integration::builtin_hyperbolic_q);
    register_builtin(env, "InverseFunctionQ", integration::builtin_inverse_function_q);
    register_builtin(env, "PowerQ", integration::builtin_power_q);
    register_builtin(env, "ProductQ", integration::builtin_product_q);
    register_builtin(env, "RationalFunctionQ", integration::builtin_rational_function_q);
    register_builtin(env, "ComplexFreeQ", integration::builtin_complex_free_q);
    register_builtin(env, "CalculusFreeQ", integration::builtin_calculus_free_q);
    register_builtin(env, "IntegralFreeQ", integration::builtin_integral_free_q);
    register_builtin(env, "InverseFunctionFreeQ", integration::builtin_inverse_function_free_q);
    register_builtin(env, "InertTrigQ", integration::builtin_inert_trig_q);
    register_builtin(env, "InertTrigFreeQ", integration::builtin_inert_trig_free_q);
    register_builtin(env, "AlgebraicFunctionQ", integration::builtin_algebraic_function_q);
    register_builtin(env, "IndependentQ", integration::builtin_independent_q);
    register_builtin(env, "PolynomialInQ", integration::builtin_polynomial_in_q);
    // Comparison stubs
    register_builtin(env, "SimplerQ", integration::builtin_simpler_q);
    register_builtin(env, "SimplerSqrtQ", integration::builtin_simpler_sqrt_q);
    register_builtin(env, "SumSimplerQ", integration::builtin_sum_simpler_q);
    register_builtin(env, "NiceSqrtQ", integration::builtin_nice_sqrt_q);
    register_builtin(env, "DerivativeDivides", integration::builtin_derivative_divides);
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
    register_builtin(env, "QuadraticMatchQ", integration::builtin_quadratic_match_q);
    register_builtin(env, "BinomialMatchQ", integration::builtin_binomial_match_q);
    register_builtin(env, "PowerOfLinearMatchQ", integration::builtin_power_of_linear_match_q);
    register_builtin(env, "NormalizePowerOfLinear", integration::builtin_normalize_power_of_linear);
    register_builtin(env, "NormalizeIntegrand", integration::builtin_normalize_integrand);
    register_builtin(env, "FunctionOfLinear", integration::builtin_function_of_linear);
    register_builtin(env, "FunctionOfLog", integration::builtin_function_of_log);
    register_builtin(env, "FunctionOfTrigOfLinearQ", integration::builtin_function_of_trig_of_linear_q);
    register_builtin(env, "FunctionOfExponentialQ", integration::builtin_function_of_exponential_q);
    register_builtin(env, "FunctionOfExponential", integration::builtin_function_of_exponential);
    register_builtin(env, "FunctionExpand", integration::builtin_function_expand);
    register_builtin(env, "SimplifyIntegrand", integration::builtin_simplify_integrand);
    register_builtin(env, "Integral", integration::builtin_integral);
    register_builtin(env, "CannotIntegrate", integration::builtin_cannot_integrate);
    register_builtin(env, "ShowStep", integration::builtin_show_step);
    register_builtin(env, "IntHide", integration::builtin_int_hide);
    register_builtin(env, "PolynomialRemainder", integration::builtin_polynomial_remainder);
    register_builtin(env, "PolynomialQuotient", integration::builtin_polynomial_quotient);
    register_builtin(env, "PolynomialDivide", integration::builtin_polynomial_divide);
    register_builtin(env, "RationalFunctionExpand", integration::builtin_rational_function_expand);
    register_builtin(env, "GeneralizedTrinomialQ", integration::builtin_generalized_trinomial_q);
    register_builtin(env, "GeneralizedBinomialQ", integration::builtin_generalized_binomial_q);
    register_builtin(env, "GeneralizedBinomialMatchQ", integration::builtin_generalized_binomial_match_q);
    register_builtin(env, "GeneralizedTrinomialMatchQ", integration::builtin_generalized_trinomial_match_q);
    register_builtin(env, "GeneralizedTrinomialDegree", integration::builtin_generalized_trinomial_degree);
    register_builtin(env, "BinomialParts", integration::builtin_binomial_parts);
    register_builtin(env, "BinomialDegree", integration::builtin_binomial_degree);
    register_builtin(env, "EulerIntegrandQ", integration::builtin_euler_integrand_q);
    register_builtin(env, "PseudoBinomialPairQ", integration::builtin_pseudo_binomial_pair_q);
    register_builtin(env, "QuotientOfLinearsQ", integration::builtin_quotient_of_linears_q);
    register_builtin(env, "QuotientOfLinearsParts", integration::builtin_quotient_of_linears_parts);
    register_builtin(env, "QuadraticProductQ", integration::builtin_quadratic_product_q);
    register_builtin(env, "LinearPairQ", integration::builtin_linear_pair_q);
    register_builtin(env, "SubstForFractionalPowerOfLinear", integration::builtin_subst_for_fractional_power_of_linear);
    register_builtin(env, "SubstForFractionalPowerQ", integration::builtin_subst_for_fractional_power_q);
    register_builtin(env, "SubstForFractionalPowerOfQuotientOfLinears", integration::builtin_subst_for_fractional_power_of_quotient_of_linears);
    register_builtin(env, "SubstForInverseFunction", integration::builtin_subst_for_inverse_function);
    register_builtin(env, "InverseFunctionOfLinear", integration::builtin_inverse_function_of_linear);
    register_builtin(env, "FunctionOfSquareRootOfQuadratic", integration::builtin_function_of_sqrt_of_quadratic);
    register_builtin(env, "FunctionOfLinear", integration::builtin_function_of_linear_fn);
    register_builtin(env, "TryPureTanSubst", integration::builtin_try_pure_tan_subst);
    register_builtin(env, "SimplerIntegrandQ", integration::builtin_simpler_integrand_q);
    register_builtin(env, "NormalizePseudoBinomial", integration::builtin_normalize_pseudo_binomial);
    register_builtin(env, "PolynomialInSubst", integration::builtin_polynomial_in_subst);
    register_builtin(env, "MinimumMonomialExponent", integration::builtin_minimum_monomial_exponent);
    register_builtin(env, "PowerVariableExpn", integration::builtin_power_variable_expn);
    register_builtin(env, "DistributeDegree", integration::builtin_distribute_degree);
    register_builtin(env, "IntSum", integration::builtin_int_sum);
    register_builtin(env, "SplitProduct", integration::builtin_split_product);
    register_builtin(env, "EveryQ", integration::builtin_every_q);
    register_builtin(env, "RationalFunctionExponents", integration::builtin_rational_function_exponents);

    // ── Discrete Calculus ──
    register_builtin(env, "DiscreteDelta", discrete::builtin_discrete_delta);
    register_builtin(env, "DiscreteShift", discrete::builtin_discrete_shift);
    register_builtin(env, "DiscreteRatio", discrete::builtin_discrete_ratio);
    register_builtin(env, "FactorialPower", discrete::builtin_factorial_power);
    register_builtin(env, "BernoulliB", discrete::builtin_bernoulli_b);
    register_builtin(env, "LinearRecurrence", discrete::builtin_linear_recurrence);
    register_builtin_env(env, "RSolve", discrete::builtin_rsolve);
    register_builtin(env, "RecurrenceTable", discrete::builtin_recurrence_table);

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
    register_builtin(env, "MovingAverage", list::builtin_moving_average);
    register_builtin_env(env, "BlockMap", list::builtin_block_map);
    register_builtin(env, "ListConvolve", list::builtin_list_convolve);
    register_builtin(env, "Nearest", list::builtin_nearest);

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
    register_builtin(env, "FromCharacterCode", string::builtin_from_character_code);
    register_builtin(env, "EditDistance", string::builtin_edit_distance);
    register_builtin(env, "LongestCommonSubsequence", string::builtin_longest_common_subsequence);
    register_builtin(env, "LongestCommonSubString", string::builtin_longest_common_sub_string);
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

    // -- Developer context --
    developer::register(env);

    // -- System information --
    systeminfo::register(env);

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
        _ => return Err(EvalError::Error("First argument to Infix must be a string".to_string())),
    };
    let head = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Err(EvalError::Error("Second argument to Infix must be a symbol".to_string())),
    };
    let precedence = if args.len() == 3 {
        match &args[2] {
            Value::Integer(n) => u32::try_from(n).unwrap_or(180),
            _ => return Err(EvalError::Error("Third argument to Infix must be an integer".to_string())),
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
        _ => return Err(EvalError::Error("First argument to Prefix must be a string".to_string())),
    };
    let head = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Err(EvalError::Error("Second argument to Prefix must be a symbol".to_string())),
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
        _ => return Err(EvalError::Error("First argument to Postfix must be a string".to_string())),
    };
    let head = match &args[1] {
        Value::Symbol(s) => s.clone(),
        _ => return Err(EvalError::Error("Second argument to Postfix must be a symbol".to_string())),
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
        "And" => {
            "And[a, b, ...] or a && b && ... evaluates arguments left to right, returning \
             the first value that is False, or the last value if all are True.\n\
             And[] = True."
        }
        "Or" => {
            "Or[a, b, ...] or a || b || ... evaluates arguments left to right, returning \
             the first value that is True, or the last value if none are True.\n\
             Or[] = False."
        }
        "Not" => "Not[expr] or !expr returns the logical negation of expr.",
        "Xor" => {
            "Xor[a, b, ...] returns True if an odd number of arguments are True.\n\
             Xor[] = False."
        }
        "Nand" => {
            "Nand[a, b, ...] returns False if all arguments are True, True otherwise.\n\
             Nand[] = False."
        }
        "Nor" => {
            "Nor[a, b, ...] returns True if no argument is True, False otherwise.\n\
             Nor[] = True."
        }
        "Implies" => "Implies[p, q] returns True unless p is True and q is False (p → q).",
        "Equivalent" => {
            "Equivalent[a, b, ...] returns True if all arguments have the same truth value.\n\
             Equivalent[] = True."
        }
        "Majority" => {
            "Majority[a, b, c, ...] returns True if more than half of the arguments are True.\n\
             Requires an odd number of arguments."
        }
        "Boole" => "Boole[expr] returns 1 if expr is True, 0 otherwise.",
        "BooleanQ" => "BooleanQ[expr] returns True if expr is True or False, False otherwise.",

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
            "Take[list, n] gives the first n elements of list.\nTake[list, -n] gives the last n elements.\nTake[list, {m, n}] gives elements m through n (inclusive)."
        }
        "Drop" => {
            "Drop[list, n] gives list with the first n elements removed.\nDrop[list, -n] removes the last n elements.\nDrop[list, {m, n}] removes elements m through n (inclusive)."
        }
        "Riffle" => "Riffle[list, x] inserts x between consecutive elements of list.",
        "Transpose" => "Transpose[list] transposes the first two levels of list.",
        "Total" => "Total[list] gives the total of all elements in list.",
        "Sum" => "Sum[expr, {i, min, max}] evaluates the sum of expr as i goes from min to max.",
        "Product" => {
            "Product[expr, {i, min, max}] evaluates a product of expr as i goes from min to max.\n\
             Product[expr, {i, max}] evaluates a product of expr for i from 1 to max."
        }
        "Partition" => {
            "Partition[list, n] splits list into sublists of length n.\nPartition[list, n, d] uses offset d between successive sublists."
        }
        "Split" => "Split[list] splits list into runs of identical adjacent elements.",
        "Gather" => "Gather[list] groups identical elements into sublists.",
        "DeleteDuplicates" => {
            "DeleteDuplicates[list] deletes all duplicates from list, keeping the first occurrence."
        }
        "Insert" => {
            "Insert[list, elem, n] inserts elem at position n in list (1-indexed, negative counts from end)."
        }
        "Delete" => {
            "Delete[list, n] deletes the element at position n in list (1-indexed, negative counts from end)."
        }
        "ReplacePart" => {
            "ReplacePart[list, n, new] replaces the element at position n in list with new."
        }
        "RotateLeft" => "RotateLeft[list, n] rotates the elements of list n positions to the left.",
        "RotateRight" => {
            "RotateRight[list, n] rotates the elements of list n positions to the right."
        }
        "Ordering" => {
            "Ordering[list] returns the positions that would sort list.\nOrdering[list, n] returns the first n positions.\nOrdering[list, -n] returns the last n positions."
        }
        "ConstantArray" => "ConstantArray[val, n] creates a list of n copies of val.",
        "Diagonal" => "Diagonal[matrix] extracts the diagonal elements from a matrix.",
        "Accumulate" => {
            "Accumulate[list] computes the running total (cumulative sum) of list elements."
        }
        "Differences" => "Differences[list] computes the adjacent differences of list elements.",
        "Clip" => {
            "Clip[x] clamps x to the range [-1, 1].\nClip[x, {min, max}] clamps x to the range [min, max]."
        }
        "Chop" => {
            "Chop[expr] replaces approximate real numbers close to 0 with exact 0.\nChop[expr, tol] uses tolerance tol (default 1e-10)."
        }
        "Unitize" => "Unitize[x] returns 0 if x == 0, 1 otherwise.",
        "Ramp" => "Ramp[x] returns max(0, x).",
        "RealAbs" => "RealAbs[x] returns the absolute value of a real number x.",
        "RealSign" => "RealSign[x] returns -1, 0, or 1 for real x.",
        "LogisticSigmoid" => "LogisticSigmoid[x] returns 1/(1+exp(-x)).",
        "NumericalOrder" => {
            "NumericalOrder[x, y] returns -1 if x < y, 0 if x == y, 1 if x > y (numeric comparison)."
        }
        "UnitBox" => "UnitBox[x] returns 1 if |x| < 1/2, 1/2 if |x| == 1/2, 0 otherwise.",
        "UnitTriangle" => "UnitTriangle[x] returns max(0, 1-|x|).",
        "Array" => {
            "Array[f, n] generates {f[1], f[2], ..., f[n]}.\nArray[f, {n}] generates {f[1], f[2], ..., f[n]}.\nArray[f, {n, m}] generates {f[n], f[n+1], ..., f[m]}."
        }
        "SplitBy" => {
            "SplitBy[list, f] splits list into runs where f applied to each element gives identical values."
        }
        "GatherBy" => {
            "GatherBy[list, f] groups elements by the values of f applied to each element."
        }
        "FoldList" => {
            "FoldList[f, init, list] gives all intermediate results of folding f from the left."
        }
        "NestList" => {
            "NestList[f, expr, n] gives all intermediate results of applying f to expr n times."
        }
        "MapApply" => {
            "MapApply[f, expr] (f @@@ expr) replaces heads at level 1, using elements of lists as arguments.\n\
             MapApply[f, {{a,b}, {c,d}}] → {f[a,b], f[c,d]}."
        }
        "MovingAverage" => {
            "MovingAverage[list, n] computes the moving average of list with window size n."
        }
        "BlockMap" => {
            "BlockMap[f, list, n] partitions list into non-overlapping blocks of size n and applies f to each."
        }
        "ListConvolve" => {
            "ListConvolve[kernel, list] computes the convolution of kernel with list.\n\
             ListConvolve[{k1,k2}, {a,b,c}] → {a*k1+b*k2, b*k1+c*k2}."
        }
        "Nearest" => {
            "Nearest[list, x] returns the element in list closest to x.\n\
             Nearest[list, x, n] returns the n closest elements."
        }

        // ── Pattern ──
        "MatchQ" => "MatchQ[expr, pattern] returns True if expr matches pattern.",
        "Head" => "Head[expr] gives the head of expr (e.g., List for {1,2,3}).",
        "TypeOf" => "TypeOf[expr] returns the type name of expr as a string.",
        "FreeQ" => "FreeQ[expr, pattern] returns True if pattern does not appear in expr.",
        "Cases" => {
            "Cases[{e1, e2, ...}, pattern] gives a list of elements that match pattern.\n\
             Cases[list, pattern, levelspec] — not yet supported."
        }
        "DeleteCases" => {
            "DeleteCases[{e1, e2, ...}, pattern] removes elements that match pattern.\n\
             DeleteCases[list, pattern, levelspec] — not yet supported."
        }
        "Dispatch" => "Dispatch[rules] builds a dispatch-indexed rule set for O(1) lookup by head name and argument type patterns. Use for large rule sets like Rubi.",

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

        // ── Extended string operations ──
        "StringPart" => {
            "StringPart[s, n] gives the n-th character in string s (1-indexed).\nStringPart[s, -n] counts from the end."
        }
        "StringPosition" => {
            "StringPosition[s, sub] gives a list of the starting positions where sub appears in s."
        }
        "StringCount" => {
            "StringCount[s, sub] gives the number of times sub appears as a substring of s."
        }
        "StringRepeat" => "StringRepeat[s, n] repeats string s n times.",
        "StringDelete" => "StringDelete[s, sub] deletes all occurrences of sub from s.",
        "StringInsert" => {
            "StringInsert[s, ins, n] inserts string ins into s at position n (1-indexed).\nStringInsert[s, ins, -n] counts from the end."
        }
        "StringRiffle" => {
            "StringRiffle[list, sep] joins the string representations of the elements in list, inserting sep between each."
        }
        "StringFreeQ" => {
            "StringFreeQ[s, sub] returns True if s does NOT contain the substring sub."
        }
        "LetterQ" => "LetterQ[s] returns True if all characters in s are letters.",
        "DigitQ" => "DigitQ[s] returns True if all characters in s are digits.",
        "UpperCaseQ" => "UpperCaseQ[s] returns True if all letters in s are uppercase.",
        "LowerCaseQ" => "LowerCaseQ[s] returns True if all letters in s are lowercase.",
        "TextWords" => "TextWords[s] gives the list of words in string s (split by whitespace).",
        "CharacterCounts" => {
            "CharacterCounts[s] returns a list of {character, count} pairs for each distinct character in s."
        }
        "Alphabet" => {
            "Alphabet[] gives the list of lowercase letters a–z.\nAlphabet[\"Latin\"] gives the same."
        }

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
        "Factorial" => "Factorial[n] or n! gives the factorial of n. For non-integer n, returns Gamma[1 + n].",
        "Gamma" => "Gamma[z] gives the Euler gamma function of z.",
        // ── Symbolic ──
        "Simplify" => "Simplify[expr] attempts to simplify expr. (Currently a pass-through.)",
        "Expand" => "Expand[expr] expands products and powers in expr. (Currently a pass-through.)",
        "D" => "D[f, x] gives the partial derivative of f with respect to x. (Planned.)",
        "Integrate" => {
            "Integrate[f, x] computes the indefinite integral of f with respect to x.\n\
                         Supports: polynomials, sin, cos, exp, tan, sec², csc², sum rule,\n\
                         constant factor extraction, and linear substitution."
        }
        "Factor" => "Factor[expr] factors the polynomial expr. (Planned.)",
        "Solve" => "Solve[eqns, vars] solves equations for variables. (Planned.)",
        "Series" => "Series[expr, {x, x0, n}] computes a power series expansion to order n.\n\
             Returns a SeriesData object that displays with an O[x-x0]^(n+1) remainder term.",

        // ── Discrete Calculus ──
        "DiscreteDelta" => {
            "DiscreteDelta[n1, n2, ...] returns 1 if all arguments are zero, 0 otherwise."
        }
        "DiscreteShift" => {
            "DiscreteShift[expr, n] represents the forward shift of expr with respect to n.\n\
             DiscreteShift[expr, n, h] shifts by step h."
        }
        "DiscreteRatio" => {
            "DiscreteRatio[expr, n] represents the ratio of expr at successive points of n.\n\
             DiscreteRatio[expr, n, h] uses step h."
        }
        "FactorialPower" => {
            "FactorialPower[x, n] gives the falling factorial x^(n) = x*(x-1)*...*(x-n+1).\n\
             FactorialPower[x, n, h] uses step h."
        }
        "BernoulliB" => "BernoulliB[n] gives the n-th Bernoulli number B_n.",
        "LinearRecurrence" => {
            "LinearRecurrence[kernel, init, n] gives the n-th term of a linear recurrence with kernel coefficients and initial values."
        }
        "RecurrenceTable" => {
            "RecurrenceTable[eqns, f, {n, nmin, nmax}] generates a list of values from recurrence equations.\n\
             Example: RecurrenceTable[{a[1] == 1, a[n+1] == 2*a[n]}, a, {n, 1, 5}]"
        }
        "RSolve" => "RSolve[eqn, f[n], n] attempts to solve a recurrence equation for f[n].",

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
        "Eigenvalues" => "Eigenvalues[m] gives the eigenvalues of matrix m via QR iteration.",
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
             Format is detected by extension, or provide an explicit 3rd argument:\n\
             Export[path, data, \"format\"].\n\
             Supported formats: JSON, CSV, TSV, Table, SVG, PNG, Text, WL."
        }
        "Import" => {
            "Import[path] imports data from a file, detecting format from extension.\n\
             Import[path, \"format\"] specifies the format explicitly.\n\
             Supported formats: JSON, CSV, TSV, Table, HTML, PNG, SVG, WL, NB, Text."
        }
        "ImportString" => {
            "ImportString[data, \"format\"] imports string data using the specified format.\n\
             Supported formats: JSON, CSV, TSV, Table, HTML, SVG, Text."
        }
        "ExportString" => {
            "ExportString[data, \"format\"] exports data to a string using the specified format.\n\
             Supported formats: JSON, CSV, TSV, Table, SVG, Text."
        }
        "ReadList" => {
            "ReadList[path] reads all lines from a file and returns them as a list of strings."
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
        "AssociationQ" => "AssociationQ[expr] returns True if expr is a valid association.",
        "Normal" => "Normal[assoc] converts an association to a list of rules.",
        "KeySort" => "KeySort[assoc] sorts the keys of an association alphabetically.",
        "KeySortBy" => "KeySortBy[assoc, f] sorts keys using the ordering function f.",
        "KeyTake" => "KeyTake[assoc, keys] returns an association with only the specified keys.",
        "KeyDrop" => "KeyDrop[assoc, keys] returns an association without the specified keys.",
        "KeySelect" => "KeySelect[assoc, pred] selects entries where pred[key] returns True.",
        "KeyMap" => "KeyMap[f, assoc] applies f to each key in the association.",
        "KeyValueMap" => {
            "KeyValueMap[f, assoc] applies f to each {key, value} pair, returning a list."
        }
        "KeyMemberQ" => "KeyMemberQ[assoc, key] returns True if the key exists in the association.",
        "KeyFreeQ" => {
            "KeyFreeQ[assoc, key] returns True if the key does NOT exist in the association."
        }
        "AssociateTo" => {
            "AssociateTo[assoc, rule] returns a new association with the key->value added.\n\
             AssociateTo[assoc, {rule1, rule2, ...}] adds multiple entries."
        }
        "KeyDropFrom" => {
            "KeyDropFrom[assoc, key] returns a new association with the specified key removed."
        }
        "Counts" => "Counts[list] returns an association counting occurrences of each element.",
        "CountsBy" => "CountsBy[list, f] counts occurrences grouped by f[element].",
        "GroupBy" => "GroupBy[list, f] groups elements of list by f[element].",
        "Merge" => {
            "Merge[{assoc1, assoc2, ...}, combiner] merges associations, using combiner for duplicate keys."
        }
        "KeyUnion" => {
            "KeyUnion[{assoc1, assoc2, ...}] returns the union of keys from multiple associations."
        }
        "KeyIntersection" => {
            "KeyIntersection[{assoc1, assoc2, ...}] returns the intersection of keys."
        }
        "KeyComplement" => {
            "KeyComplement[assoc1, assoc2] returns keys in assoc1 that are not in assoc2."
        }

        // ── Dataset ──
        "Dataset" => {
            "Dataset[data] creates a Dataset wrapper around structured data for pretty display and query operations.\nUse call syntax: ds[All, \"col\"], ds[i], ds[i, \"col\"]."
        }
        "DatasetQ" => "DatasetQ[x] returns True if x is a Dataset, False otherwise.",
        "SortBy" => {
            "SortBy[list, f] sorts list elements by the key produced by applying f to each element."
        }
        "JoinAcross" => {
            "JoinAcross[list1, list2, key] performs an inner join of two lists of associations on the specified key."
        }

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

        // ── Sequence ──
        "Sequence" => {
            "Sequence[expr1, expr2, ...] represents a sequence of arguments \
             that automatically splices into function calls.\n\
             Sequence[] evaporates entirely; Sequence[expr] acts like Identity.\n\
             Most functions automatically splice Sequence; those with \
             SequenceHold or HoldAllComplete do not."
        }

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
        "Return" => {
            "Return[expr] returns expr from the enclosing function.\nReturn[] returns Null."
        }
        "Break" => "Break[] exits the enclosing For, While, or Do loop.",
        "Continue" => "Continue[] skips to the next iteration of the enclosing loop.",
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
        "ParallelSum" => {
            "ParallelSum[expr, {i, min, max}] evaluates a parallel sum of expr as i goes from min to max.\n\
             ParallelSum[expr, {i, max}] evaluates a parallel sum of expr for i from 1 to max."
        }
        "ParallelEvaluate" => {
            "ParallelEvaluate[expr] evaluates expr on each parallel worker, returning a list of results."
        }
        "ParallelTry" => {
            "ParallelTry[list] evaluates each element of list in parallel, returning the first result obtained.\n\
             ParallelTry[f, list] applies f to each element of list in parallel, returning the first result."
        }
        "ParallelProduct" => {
            "ParallelProduct[expr, {i, min, max}] evaluates a parallel product of expr as i goes from min to max.\n\
             ParallelProduct[expr, {i, max}] evaluates a parallel product of expr for i from 1 to max."
        }
        "ParallelDo" => {
            "ParallelDo[expr, {i, min, max}] evaluates expr for i from min to max in parallel, returning Null.\n\
             ParallelDo[expr, {i, max}] evaluates expr for i from 1 to max in parallel, returning Null."
        }
        "ParallelCombine" => {
            "ParallelCombine[f, list] applies binary function f to combine elements of list in parallel, returning a single result."
        }
        "ProcessorCount" => {
            "ProcessorCount returns the number of processor cores on the current computer."
        }
        "AbortKernels" => {
            "AbortKernels[] aborts all running kernel evaluations. (Currently a no-op.)"
        }

        // ── Format/display ──
        "InputForm" => {
            "InputForm[expr] displays expr using infix notation (e.g., `a + b` instead of `Plus[a, b]`)."
        }
        "FullForm" => "FullForm[expr] displays expr in head[arg, ...] notation.",
        "StandardForm" => "StandardForm[expr] displays expr in StandardForm (infix notation with SeriesData special display).",
        "OutputForm" => "OutputForm[expr] displays expr in OutputForm (plain-text, same as StandardForm for terminal).",
        "Short" => {
            "Short[expr] displays expr with top-level truncation (shows at most 5 items).\nShort[expr, n] displays at most n top-level items."
        }
        "Shallow" => {
            "Shallow[expr] displays expr with limited nesting depth (default 3).\nShallow[expr, n] limits nesting to n levels."
        }
        "NumberForm" => "NumberForm[expr, n] displays numbers with n significant digits.",
        "ScientificForm" => {
            "ScientificForm[expr, n] displays numbers in scientific notation with n significant digits."
        }
        "BaseForm" => "BaseForm[expr, base] displays a number in the given base (2–36).",
        "Grid" => "Grid[list] displays a 2D list as an aligned table grid.",
        "Defer" => "Defer[expr] displays expr in its original form. (Currently a display wrapper.)",
        "SyntaxQ" => {
            "SyntaxQ[\"expr\"] returns True if expr is valid Syma syntax, False otherwise. Performs lex + parse only (no evaluation)."
        }
        "SyntaxLength" => {
            "SyntaxLength[\"expr\"] returns the position of the first syntax error, or the length of the string if valid."
        }

        // ── Persistent storage ──
        "LocalSymbol" => {
            "LocalSymbol[\"name\"] reads a persisted value from ~/.syma/LocalSymbols/.\n\
             LocalSymbol[\"name\", default] returns default if the key does not exist.\n\
             LocalSymbol[\"name\"] = value assigns a value to a persistent key.\n\
             Supported value types: Integer, Real, String, Bool, Null, List, Assoc."
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
        "Names" => {
            "Names[] returns a sorted list of all known symbol names.\nNames[\"pattern\"] returns symbol names matching a string pattern, where * matches any sequence of characters and ? matches any single character."
        }

        // ── Symbol Clearing ──
        "Clear" => {
            "Clear[sym1, sym2, ...] removes definitions, values, and attributes for each symbol. Protected symbols are not affected."
        }
        "ClearAll" => {
            "ClearAll[sym1, sym2, ...] removes definitions, values, attributes, and lazy providers for each symbol. Protected symbols are not affected."
        }
        "Unset" => {
            "Unset[sym] removes the value or definition for a symbol without clearing its attributes. Protected symbols are not affected."
        }
        "Remove" => {
            "Remove[sym1, sym2, ...] completely removes symbols from the system, including bindings, attributes, and lazy providers. Removes even Protected symbols."
        }

        // ── Image Processing ──
        "Image" => {
            "Image[data] creates an image from a 2D (grayscale) or 3D (RGB/RGBA) list of values in [0,1].\n\
             Image[data, \"type\"] specifies the storage type (e.g., \"Byte\")."
        }
        "ImageData" => {
            "ImageData[image] extracts pixel data as a list of lists with values in [0,1]."
        }
        "ImageDimensions" => "ImageDimensions[image] returns {width, height} of the image.",
        "ImageType" => {
            "ImageType[image] returns the image type as a string: \"Byte\", \"Bit16\", or \"Real32\"."
        }
        "ImageResize" => {
            "ImageResize[image, {w, h}] resizes image to the given dimensions using Lanczos3 filter.\n\
             ImageResize[image, n] scales width to n pixels preserving aspect ratio."
        }
        "ImageRotate" => {
            "ImageRotate[image, angle] rotates the image by the given angle in degrees (90, 180, 270 supported natively)."
        }
        "ImageAdjust" => {
            "ImageAdjust[image] auto-stretches contrast to the full range.\n\
             ImageAdjust[image, {c, b, g}] adjusts contrast (c), brightness (b), and gamma (g)."
        }
        "Binarize" => {
            "Binarize[image] converts to black and white (threshold at 0.5).\n\
             Binarize[image, t] uses threshold t in [0,1]."
        }
        "ColorConvert" => {
            "ColorConvert[image, \"Grayscale\"] converts an image to grayscale.\n\
             ColorConvert[image, \"RGB\"] converts to RGB."
        }
        "GaussianFilter" => "GaussianFilter[image, r] applies Gaussian blur with sigma = r.",
        "EdgeDetect" => {
            "EdgeDetect[image] applies Sobel edge detection, returning an edge magnitude image."
        }
        "ImageConvolve" => {
            "ImageConvolve[image, kernel] convolves image with a 2D kernel (list of lists, odd dimensions)."
        }

        // ── Number theory help ──
        "ModularInverse" => {
            "ModularInverse[a, m] gives the modular inverse of a modulo m, or returns unevaluated if no inverse exists."
        }
        "PrimeOmega" => {
            "PrimeOmega[n] gives the total number of prime factors of n, counting multiplicities (Ω(n)).\n\
             PrimeOmega[12] = 3 (2^2 * 3^1)."
        }
        "PrimeNu" => {
            "PrimeNu[n] gives the number of distinct prime factors of n (ω(n)).\n\
             PrimeNu[12] = 2 (2 and 3)."
        }
        "DigitCount" => {
            "DigitCount[n] returns a list of digit counts for n in base 10.\n\
             DigitCount[n, base] uses the given base.\n\
             DigitCount[n, base, d] returns the count of digit d."
        }
        "JacobiSymbol" => {
            "JacobiSymbol[a, n] computes the Jacobi symbol (a/n), where n is a positive odd integer.\n\
             Returns -1, 0, or 1."
        }
        "ChineseRemainder" => {
            "ChineseRemainder[{a1, a2, ...}, {n1, n2, ...}] solves the system of congruences\n\
             x ≡ a_i (mod n_i) for pairwise coprime moduli."
        }
        "MultiplicativeOrder" => {
            "MultiplicativeOrder[a, n] gives the smallest positive integer k such that a^k ≡ 1 (mod n).\n\
             Requires gcd(a, n) = 1."
        }
        "PrimitiveRoot" => {
            "PrimitiveRoot[n] gives the smallest primitive root of n, or raises an error\n\
             if no primitive root exists."
        }
        "PerfectNumberQ" => {
            "PerfectNumberQ[n] returns True if n is a perfect number (sum of proper divisors equals n),\n\
             False otherwise."
        }
        "MangoldtLambda" => {
            "MangoldtLambda[n] returns ln(p) if n = p^k for prime p and k ≥ 1, or 0 otherwise.\n\
             The von Mangoldt function Λ(n)."
        }
        "LiouvilleLambda" => {
            "LiouvilleLambda[n] returns (-1)^Ω(n), where Ω(n) is the total number of prime factors\n\
             with multiplicity. The Liouville function λ(n)."
        }
        "DivisorSum" => {
            "DivisorSum[n, form] sums form[d] for all positive divisors d of n.\n\
             The form function must return an integer for each divisor."
        }

        // -- Developer context --
        "$MaxMachineInteger" => {
            "$MaxMachineInteger is the maximum machine-sized integer (2^63 - 1 on 64-bit systems)."
        }
        "MachineIntegerQ" => {
            "MachineIntegerQ[expr] returns True if expr is an integer that fits in a machine-sized integer."
        }
        "ToPackedArray" => {
            "ToPackedArray[list] converts a list of integers or reals to a packed array."
        }
        "FromPackedArray" => {
            "FromPackedArray[packed] converts a packed array back to a regular list."
        }
        "PackedArrayQ" => "PackedArrayQ[expr] returns True if expr is a packed array.",
        "PackedArrayForm" => "PackedArrayForm is an option symbol for PackedArray display.",
        "BesselSimplify" => {
            "BesselSimplify[expr] attempts to simplify Bessel function expressions."
        }
        "GammaSimplify" => "GammaSimplify[expr] attempts to simplify Gamma function expressions.",
        "PolyGammaSimplify" => {
            "PolyGammaSimplify[expr] attempts to simplify PolyGamma expressions."
        }
        "ZetaSimplify" => "ZetaSimplify[expr] attempts to simplify Zeta function expressions.",
        "PolyLogSimplify" => {
            "PolyLogSimplify[expr] attempts to simplify PolyLog function expressions."
        }
        "TrigToRadicals" => {
            "TrigToRadicals[expr] converts trigonometric expressions to radical form."
        }
        "CellInformation" => {
            "CellInformation[expr] returns cell information (notebook frontend not yet available)."
        }
        "NotebookConvert" => {
            "NotebookConvert[source] converts notebooks (notebook frontend not yet available)."
        }
        "ReplaceAllUnheld" => {
            "ReplaceAllUnheld[expr, rules] applies replacement rules without holding (wraps ReplaceAll)."
        }

        // -- System information --
        "$System" => {
            "$System gives the operating system and processor type for the current machine (e.g., \"MacOS-x86-64\")."
        }
        "$Version" => {
            "$Version gives the version information for the current Syma installation."
        }
        "$ReleaseDate" => {
            "$ReleaseDate gives the release date of the Syma version as a string."
        }
        "$Machine" => {
            "$Machine gives the processor type of the current machine (e.g., \"x86-64\")."
        }
        "$MachineName" => {
            "$MachineName gives the network name of the current machine."
        }
        "$OperatingSystem" => {
            "$OperatingSystem gives the name of the operating system (e.g., \"MacOS\", \"Linux\", \"Windows\")."
        }
        "$ProcessorType" => {
            "$ProcessorType gives the processor type (e.g., \"x86-64\", \"aarch64\")."
        }
        "$User" => {
            "$User gives the login name of the current user."
        }
        "$TimeZone" => {
            "$TimeZone gives the local timezone offset from UTC in hours."
        }
        "$SystemId" => {
            "$SystemId gives the system identifier (e.g., \"MacOS\", \"Linux\", \"Windows\")."
        }
        "$Language" => {
            "$Language gives the interface language (default \"English\")."
        }
        "$CommandLine" => {
            "$CommandLine gives True if the session was started from the command line."
        }
        "$InputLine" => {
            "$InputLine gives the text of the current input line, or Null."
        }

        _ => return None,
    })
}

/// Get known attributes for a built-in function.
pub fn get_attributes(name: &str) -> Vec<&'static str> {
    // Helper to build the common pattern: Listable + NumericFunction + Locked + ReadProtected
    fn lnlr() -> Vec<&'static str> {
        vec!["Listable", "Locked", "NumericFunction", "ReadProtected"]
    }
    fn llr() -> Vec<&'static str> {
        vec!["Listable", "Locked", "ReadProtected"]
    }
    match name {
        "Plus" | "Times" | "Min" | "Max" => vec![
            "Flat",
            "Listable",
            "Locked",
            "NumericFunction",
            "OneIdentity",
            "Orderless",
            "ReadProtected",
        ],
        "Power" => vec!["Listable", "Locked", "NumericFunction", "ReadProtected"],
        "Divide" | "Minus" | "Abs" => lnlr(),
        "NonCommutativeMultiply" => vec![
            "Flat",
            "Locked",
            "OneIdentity",
            "ReadProtected",
        ],
        "Commutator" | "Anticommutator" => vec!["Locked", "ReadProtected"],
        "Sin" | "Cos" | "Tan" | "Log" | "Exp" | "Sqrt" | "Floor" | "Ceiling" | "Round" => lnlr(),
        "ArcSin" | "ArcCos" | "ArcTan" | "Log2" | "Log10" => lnlr(),
        "Csc" | "Sec" | "Cot" | "ArcCsc" | "ArcSec" | "ArcCot" => lnlr(),
        "Haversine" | "InverseHaversine" => lnlr(),
        "SinDegrees" | "CosDegrees" | "TanDegrees" | "CscDegrees" | "SecDegrees" | "CotDegrees" => {
            lnlr()
        }
        "ArcSinDegrees" | "ArcCosDegrees" | "ArcTanDegrees" | "ArcCscDegrees" | "ArcSecDegrees"
        | "ArcCotDegrees" => lnlr(),
        "Factorial" => llr(),
        "And" | "Or" => vec![
            "Flat",
            "HoldAll",
            "Locked",
            "OneIdentity",
            "Orderless",
            "ReadProtected",
        ],
        "Not" => llr(),
        "Xor" => vec![
            "Flat",
            "Listable",
            "Locked",
            "OneIdentity",
            "Orderless",
            "ReadProtected",
        ],
        "Nand" => llr(),
        "Nor" => llr(),
        "Implies" => vec!["HoldFirst", "Locked", "ReadProtected"],
        "Equivalent" => vec![
            "Flat",
            "Listable",
            "Locked",
            "OneIdentity",
            "Orderless",
            "ReadProtected",
        ],
        "Boole" => llr(),
        "Chop" => llr(),
        "Unitize" => llr(),
        "Ramp" => vec!["Listable", "Locked", "NumericFunction", "ReadProtected"],
        "RealAbs" => vec!["Listable", "Locked", "NumericFunction", "ReadProtected"],
        "RealSign" => llr(),
        "LogisticSigmoid" => vec!["Listable", "Locked", "NumericFunction", "ReadProtected"],
        "UnitBox" => llr(),
        "UnitTriangle" => llr(),
        "Majority" => vec![],
        "BooleanQ" => vec![],
        "Hold" => vec!["HoldAll", "Locked", "ReadProtected"],
        "HoldComplete" => vec!["HoldAllComplete", "Locked", "ReadProtected"],
        "Defer" => vec!["HoldAll", "Locked", "ReadProtected"],
        "MessageName" => vec!["HoldFirst", "Locked", "ReadProtected"],
        // -- Sequence --
        "Sequence" => vec!["HoldAll", "Locked", "ReadProtected", "SequenceHold"],
        // -- Scoping/conditionals (HoldAll so body is not pre-evaluated) --
        "With" | "Module" | "Block" => vec!["HoldAll", "Locked", "ReadProtected"],
        "If" => vec!["HoldAll", "Locked", "ReadProtected"],
        // -- Calculus (HoldAll so expressions are not pre-evaluated) --
        "Integrate" => vec!["HoldAll", "Locked", "ReadProtected"],
        "ReplaceAll" | "ReplaceRepeated" => vec!["Locked", "ReadProtected", "SequenceHold"],
        // -- Equation solvers (need HoldAll so equations aren't evaluated before solving) --
        "Solve" | "RSolve" => vec!["HoldAll", "Locked", "ReadProtected"],
        // -- Constants --
        "Pi" | "E" | "Degree" => vec!["Constant", "Locked", "ReadProtected"],
        // -- Math functions (Listable + NumericFunction) --
        "Mod" | "GCD" | "LCM" | "IntegerPart" | "FractionalPart" | "Sign" | "UnitStep" | "Clip"
        | "Rescale" | "Quotient" | "KroneckerDelta" => {
            vec!["Listable", "Locked", "NumericFunction", "ReadProtected"]
        }
        // -- Predicates (Listable only) --
        "IntegerQ" | "PrimeQ" | "EvenQ" | "OddQ" | "Divisible" | "CoprimeQ" | "PrimeOmega"
        | "PrimeNu" => llr(),
        // -- String functions (Listable only) --
        "StringLength" | "StringReverse" | "StringContainsQ" | "StringStartsQ" | "StringEndsQ"
        | "StringFreeQ" | "ToUpperCase" | "ToLowerCase" => llr(),
        // -- Developer context --
        "BesselSimplify" | "GammaSimplify" | "PolyGammaSimplify" | "ZetaSimplify"
        | "PolyLogSimplify" | "TrigToRadicals" => llr(),
        // -- Symbol Names --
        "Names" => vec!["Locked", "ReadProtected"],
        // -- Symbol Clearing --
        "Clear" | "ClearAll" | "Remove" => vec!["Locked", "ReadProtected"],
        "Unset" => vec!["HoldFirst", "Locked", "ReadProtected"],
        _ => vec![],
    }
}
