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
    // ── Data-driven builtin registration ──
    // Pure builtins (no env parameter)
    const PURE_BUILTINS: &[(&str, fn(&[Value]) -> Result<Value, EvalError>)] = &[
        ("AbortKernels", parallel::builtin_abort_kernels),
        ("Abs", arithmetic::builtin_abs),
        ("AbsArg", arithmetic::builtin_abs_arg),
        ("AbsoluteTime", calendar::builtin_absolute_time),
        ("Accumulate", list::builtin_accumulate),
        ("ActivateTrig", integration::builtin_activate_trig),
        ("Alphabet", string::builtin_alphabet),
        ("Apart", symbolicmanip::builtin_apart),
        ("Append", list::builtin_append),
        ("ApplyTo", list::builtin_apply_to),
        ("ArcCos", math::builtin_arccos),
        ("ArcCosDegrees", math::builtin_arccos_degrees),
        ("ArcCosh", math::builtin_arccosh),
        ("ArcCot", math::builtin_arccot),
        ("ArcCotDegrees", math::builtin_arccot_degrees),
        ("ArcCoth", math::builtin_arccoth),
        ("ArcCsc", math::builtin_arccsc),
        ("ArcCscDegrees", math::builtin_arccsc_degrees),
        ("ArcCsch", math::builtin_arccsch),
        ("ArcSec", math::builtin_arcsec),
        ("ArcSecDegrees", math::builtin_arcsec_degrees),
        ("ArcSech", math::builtin_arcsech),
        ("ArcSin", math::builtin_arcsin),
        ("ArcSinDegrees", math::builtin_arcsin_degrees),
        ("ArcSinh", math::builtin_arcsinh),
        ("ArcTan", math::builtin_arctan),
        ("ArcTanDegrees", math::builtin_arctan_degrees),
        ("ArcTanh", math::builtin_arctanh),
        ("Arg", arithmetic::builtin_arg),
        ("Arrangements", combinatorics::builtin_arrangements),
        ("ArrayPad", list::builtin_array_pad),
        ("ArrayReshape", list::builtin_array_reshape),
        ("AssociateTo", association::builtin_associate_to),
        ("AssociationQ", association::builtin_association_q),
        ("AtomQ", integration::builtin_atom_q),
        ("BaseForm", format::builtin_base_form),
        ("BellB", combinatorics::builtin_bell_b),
        ("BernoulliB", discrete::builtin_bernoulli_b),
        ("Binarize", image::builtin_binarize),
        ("Binomial", combinatorics::builtin_binomial),
        ("BinomialDegree", integration::builtin_binomial_degree),
        ("BinomialMatchQ", integration::builtin_binomial_match_q),
        ("BinomialParts", integration::builtin_binomial_parts),
        ("BinomialQ", integration::builtin_binomial_q),
        ("Boole", logical::builtin_boole),
        ("BooleanQ", logical::builtin_boolean_q),
        ("CalculusFreeQ", integration::builtin_calculus_free_q),
        ("Cancel", symbolicmanip::builtin_cancel),
        ("CatalanNumber", combinatorics::builtin_catalan_number),
        ("Ceiling", math::builtin_ceiling),
        ("CharacterCounts", string::builtin_character_counts),
        ("Characters", string::builtin_characters),
        ("Chop", math::builtin_chop),
        ("Clip", math::builtin_clip),
        ("CloseKernels", parallel::builtin_close_kernels),
        ("Coeff", integration::builtin_coeff),
        ("Coefficient", integration::builtin_coefficient),
        ("Collect", symbolicmanip::builtin_collect),
        ("ColorConvert", image::builtin_color_convert),
        ("Commutator", noncommutative::builtin_commutator),
        ("Complement", list::builtin_complement),
        ("Complex", arithmetic::builtin_complex),
        ("ComplexFreeQ", integration::builtin_complex_free_q),
        ("ComplexQ", arithmetic::builtin_complex_q),
        ("CompositeQ", number_theory::builtin_composite_q),
        ("Conjugate", arithmetic::builtin_conjugate),
        ("ConstantArray", list::builtin_constant_array),
        ("CoprimeQ", number_theory::builtin_coprime_q),
        ("Cos", math::builtin_cos),
        ("CosDegrees", math::builtin_cos_degrees),
        ("Cosh", math::builtin_cosh),
        ("Cot", math::builtin_cot),
        ("CotDegrees", math::builtin_cot_degrees),
        ("Coth", math::builtin_coth),
        ("Count", list::builtin_count),
        ("Counts", association::builtin_counts),
        ("Csc", math::builtin_csc),
        ("CscDegrees", math::builtin_csc_degrees),
        ("Csch", math::builtin_csch),
        ("Dataset", dataset::builtin_dataset),
        ("DatasetQ", dataset::builtin_dataset_q),
        ("DateDifference", calendar::builtin_date_difference),
        ("DateList", calendar::builtin_date_list),
        ("DateObject", calendar::builtin_date_object),
        ("DatePlus", calendar::builtin_date_plus),
        ("DateString", calendar::builtin_date_string),
        ("DayCount", calendar::builtin_day_count),
        ("DayName", calendar::builtin_day_name),
        ("DeactivateTrig", integration::builtin_deactivate_trig),
        ("Defer", format::builtin_defer),
        ("Delete", list::builtin_delete),
        ("DeleteDuplicates", list::builtin_delete_duplicates),
        ("Denom", integration::builtin_denom),
        ("Denominator", integration::builtin_denominator),
        ("Diagonal", list::builtin_diagonal),
        ("Differences", list::builtin_differences),
        ("DigitCount", number_theory::builtin_digit_count),
        ("DigitQ", string::builtin_digit_q),
        ("DirectoryName", filesystem::builtin_directory_name),
        ("DirectoryQ", filesystem::builtin_directory_q),
        ("DiscreteDelta", discrete::builtin_discrete_delta),
        ("DiscreteRatio", discrete::builtin_discrete_ratio),
        ("DiscreteShift", discrete::builtin_discrete_shift),
        ("Discriminant", integration::builtin_discriminant),
        ("Dispatch", pattern::builtin_dispatch),
        ("Dist", integration::builtin_dist),
        ("Distrib", integration::builtin_distrib),
        ("Divide", arithmetic::builtin_divide),
        ("Divisible", number_theory::builtin_divisible),
        ("DivisorSigma", number_theory::builtin_divisor_sigma),
        ("Divisors", number_theory::builtin_divisors),
        ("Drop", list::builtin_drop),
        ("EdgeDetect", image::builtin_edge_detect),
        ("EditDistance", string::builtin_edit_distance),
        ("EqQ", integration::builtin_eq_q),
        ("Equal", comparison::builtin_equal),
        ("Equivalent", logical::builtin_equivalent),
        ("Error", error::builtin_error),
        ("EulerPhi", number_theory::builtin_euler_phi),
        ("EvenQ", math::builtin_even_q),
        ("EveryQ", integration::builtin_every_q),
        ("Exp", math::builtin_exp),
        ("Expand", symbolic::builtin_expand),
        ("ExpandFileName", filesystem::builtin_expand_file_name),
        ("ExpandToSum", integration::builtin_expand_to_sum),
        ("ExpandTrig", integration::builtin_expand_trig),
        ("Expon", integration::builtin_expon),
        ("Exponent", integration::builtin_exponent),
        ("Export", io::builtin_export),
        ("ExportString", io::builtin_export_string),
        ("Factor", symbolic::builtin_factor),
        ("FactorInteger", number_theory::builtin_factor_integer),
        ("Factorial", math::builtin_factorial),
        ("Factorial2", combinatorics::builtin_factorial2),
        ("FactorialPower", discrete::builtin_factorial_power),
        ("FalseQ", integration::builtin_false_q),
        ("Fibonacci", combinatorics::builtin_fibonacci),
        ("FileBaseName", filesystem::builtin_file_base_name),
        ("FileExistsQ", filesystem::builtin_file_exists_q),
        ("FileExtension", filesystem::builtin_file_extension),
        ("FileNameDepth", filesystem::builtin_file_name_depth),
        ("FileNameDrop", filesystem::builtin_file_name_drop),
        ("FileNameJoin", filesystem::builtin_file_name_join),
        ("FileNameSplit", filesystem::builtin_file_name_split),
        ("FileNameTake", filesystem::builtin_file_name_take),
        ("FileNames", filesystem::builtin_file_names),
        ("FileRead", io::builtin_file_read),
        ("FileWrite", io::builtin_file_write),
        ("First", list::builtin_first),
        ("Flatten", list::builtin_flatten),
        ("Floor", math::builtin_floor),
        ("FracPart", integration::builtin_frac_part),
        ("FractionalPart", math::builtin_fractional_part),
        ("FreeFactors", integration::builtin_free_factors),
        ("FromDigits", number_theory::builtin_from_digits),
        ("FullForm", format::builtin_full_form),
        ("FunctionExpand", integration::builtin_function_expand),
        ("FunctionOfLog", integration::builtin_function_of_log),
        ("FunctionOfQ", integration::builtin_function_of_q),
        ("GCD", math::builtin_gcd),
        ("Gamma", math::builtin_gamma),
        ("Gather", list::builtin_gather),
        ("GaussianFilter", image::builtin_gaussian_filter),
        ("GeQ", integration::builtin_ge_q),
        ("Greater", comparison::builtin_greater),
        ("GreaterEqual", comparison::builtin_greater_equal),
        ("Grid", format::builtin_grid),
        ("GtQ", integration::builtin_gt_q),
        ("HalfIntegerQ", integration::builtin_half_integer_q),
        ("Haversine", math::builtin_haversine),
        ("Head", pattern::builtin_head),
        ("HyperbolicQ", integration::builtin_hyperbolic_q),
        ("IGeQ", integration::builtin_ige_q),
        ("IGtQ", integration::builtin_igt_q),
        ("ILeQ", integration::builtin_ile_q),
        ("ILtQ", integration::builtin_ilt_q),
        ("Im", arithmetic::builtin_im),
        ("Image", image::builtin_image),
        ("ImageAdjust", image::builtin_image_adjust),
        ("ImageConvolve", image::builtin_image_convolve),
        ("ImageData", image::builtin_image_data),
        ("ImageDimensions", image::builtin_image_dimensions),
        ("ImageResize", image::builtin_image_resize),
        ("ImageRotate", image::builtin_image_rotate),
        ("ImageType", image::builtin_image_type),
        ("Import", io::builtin_import),
        ("ImportString", io::builtin_import_string),
        ("IndependentQ", integration::builtin_independent_q),
        ("InertTrigQ", integration::builtin_inert_trig_q),
        ("Input", io::builtin_input),
        ("InputForm", format::builtin_input_form),
        ("Insert", list::builtin_insert),
        ("IntBinomialQ", integration::builtin_int_binomial_q),
        ("IntHide", integration::builtin_int_hide),
        ("IntLinearQ", integration::builtin_int_linear_q),
        ("IntPart", integration::builtin_int_part),
        ("IntQuadraticQ", integration::builtin_int_quadratic_q),
        ("IntSum", integration::builtin_int_sum),
        ("IntegerDigits", number_theory::builtin_integer_digits),
        ("IntegerPart", math::builtin_integer_part),
        ("IntegerQ", math::builtin_integer_q),
        ("IntegersQ", integration::builtin_integers_q),
        ("Integral", integration::builtin_integral),
        ("IntegralFreeQ", integration::builtin_integral_free_q),
        ("Intersection", list::builtin_intersection),
        ("InverseHaversine", math::builtin_inverse_haversine),
        ("JacobiSymbol", number_theory::builtin_jacobi_symbol),
        ("Join", list::builtin_join),
        ("JoinAcross", dataset::builtin_join_across),
        ("KernelCount", parallel::builtin_kernel_count),
        ("KeyComplement", association::builtin_key_complement),
        ("KeyDrop", association::builtin_key_drop),
        ("KeyDropFrom", association::builtin_key_drop_from),
        ("KeyExistsQ", association::builtin_key_member_q),
        ("KeyFreeQ", association::builtin_key_free_q),
        ("KeyMemberQ", association::builtin_key_member_q),
        ("KeySort", association::builtin_key_sort),
        ("KeyTake", association::builtin_key_take),
        ("KeyUnion", association::builtin_key_union),
        ("Keys", association::builtin_keys),
        ("KroneckerDelta", math::builtin_kronecker_delta),
        ("LCM", math::builtin_lcm),
        ("Last", list::builtin_last),
        ("LaunchKernels", parallel::builtin_launch_kernels),
        ("LeQ", integration::builtin_le_q),
        ("LeapYearQ", calendar::builtin_leap_year_q),
        ("Length", list::builtin_length),
        ("Less", comparison::builtin_less),
        ("LessEqual", comparison::builtin_less_equal),
        ("LetterQ", string::builtin_letter_q),
        ("Limit", symbolicmanip::builtin_limit),
        ("LinearMatchQ", integration::builtin_linear_match_q),
        ("LinearPairQ", integration::builtin_linear_pair_q),
        ("LinearQ", integration::builtin_linear_q),
        ("LinearRecurrence", discrete::builtin_linear_recurrence),
        ("ListConvolve", list::builtin_list_convolve),
        ("LocalSymbol", localsymbol::builtin_local_symbol),
        ("Log", math::builtin_log),
        ("Log10", math::builtin_log10),
        ("Log2", math::builtin_log2),
        ("LogisticSigmoid", math::builtin_logistic_sigmoid),
        ("Lookup", association::builtin_lookup),
        ("LowerCaseQ", string::builtin_lower_case_q),
        ("LtQ", integration::builtin_lt_q),
        ("LucasL", combinatorics::builtin_lucas_l),
        ("Majority", logical::builtin_majority),
        ("MatrixForm", format::builtin_matrix_form),
        ("Max", math::builtin_max),
        ("MemberQ", list::builtin_member_q),
        ("Message", builtin_message),
        ("MessageName", builtin_message_name),
        ("Min", math::builtin_min),
        ("Minus", arithmetic::builtin_minus),
        ("Mod", math::builtin_mod),
        ("MoebiusMu", number_theory::builtin_moebius_mu),
        ("MonthName", calendar::builtin_month_name),
        ("Most", list::builtin_most),
        ("MovingAverage", list::builtin_moving_average),
        ("Multinomial", combinatorics::builtin_multinomial),
        ("NLimit", symbolicmanip::builtin_nlimit),
        ("Nand", logical::builtin_nand),
        ("NeQ", integration::builtin_ne_q),
        ("Nearest", list::builtin_nearest),
        ("NegQ", integration::builtin_neg_q),
        ("NegativeQ", math::builtin_negative_q),
        ("NextPrime", number_theory::builtin_next_prime),
        ("NiceSqrtQ", integration::builtin_nice_sqrt_q),
        ("NonNegativeQ", math::builtin_non_negative_q),
        ("NonfreeFactors", integration::builtin_nonfree_factors),
        ("NonsumQ", integration::builtin_nonsum_q),
        ("Nor", logical::builtin_nor),
        ("Normal", association::builtin_normal),
        ("Not", logical::builtin_not),
        ("Now", calendar::builtin_now),
        ("NumberExpand", number_theory::builtin_number_expand),
        ("NumberForm", format::builtin_number_form),
        ("Numer", integration::builtin_numer),
        ("Numerator", integration::builtin_numerator),
        ("NumericalOrder", math::builtin_numerical_order),
        ("OddQ", integration::builtin_odd_q),
        ("Order", comparison::builtin_order),
        ("Ordering", list::builtin_ordering),
        ("PadLeft", list::builtin_pad_left),
        ("PadRight", list::builtin_pad_right),
        ("PaddedForm", format::builtin_padded_form),
        ("ParallelDo", parallel::builtin_parallel_do),
        ("ParallelEvaluate", parallel::builtin_parallel_evaluate),
        ("ParallelProduct", parallel::builtin_parallel_product),
        ("ParallelSum", parallel::builtin_parallel_sum),
        ("ParallelTable", parallel::builtin_parallel_table),
        ("ParallelTry", parallel::builtin_parallel_try),
        ("ParentDirectory", filesystem::builtin_parent_directory),
        ("Part", list::builtin_part),
        ("Partition", list::builtin_partition),
        ("PartitionsP", combinatorics::builtin_partitions_p),
        ("PartitionsQ", combinatorics::builtin_partitions_q),
        ("PerfectPowerQ", number_theory::builtin_perfect_power_q),
        ("PerfectSquareQ", integration::builtin_perfect_square_q),
        ("Permutations", combinatorics::builtin_permutations),
        ("Plot", graphics::builtin_plot_stub),
        ("Plus", arithmetic::builtin_plus),
        ("PolyQ", integration::builtin_poly_q),
        ("PolynomialInQ", integration::builtin_polynomial_in_q),
        ("PolynomialQ", integration::builtin_polynomial_q),
        ("PosQ", integration::builtin_pos_q),
        ("Position", list::builtin_position),
        ("PositiveQ", math::builtin_positive_q),
        ("Power", arithmetic::builtin_power),
        ("PowerMod", number_theory::builtin_power_mod),
        ("PowerQ", integration::builtin_power_q),
        ("Prepend", list::builtin_prepend),
        ("Prime", number_theory::builtin_prime),
        ("PrimeNu", number_theory::builtin_prime_nu),
        ("PrimeOmega", number_theory::builtin_prime_omega),
        ("PrimePi", number_theory::builtin_prime_pi),
        ("PrimePowerQ", number_theory::builtin_prime_power_q),
        ("PrimeQ", number_theory::builtin_prime_q),
        ("PrimitiveRoot", number_theory::builtin_primitive_root),
        ("Print", io::builtin_print),
        ("PrintF", io::builtin_printf),
        ("ProcessorCount", parallel::builtin_processor_count),
        ("Product", list::builtin_product),
        ("ProductQ", integration::builtin_product_q),
        ("QuadraticQ", integration::builtin_quadratic_q),
        ("Quotient", math::builtin_quotient),
        ("QuotientRemainder", math::builtin_quotient_remainder),
        ("Ramp", math::builtin_ramp),
        ("RandomChoice", random::builtin_random_choice),
        ("RandomInteger", random::builtin_random_integer),
        ("RandomReal", random::builtin_random_real),
        ("Range", list::builtin_range),
        ("RationalQ", integration::builtin_rational_q),
        ("Re", arithmetic::builtin_re),
        ("ReIm", arithmetic::builtin_reim),
        ("ReadList", io::builtin_read_list),
        ("ReadString", io::builtin_read_string),
        ("RealAbs", math::builtin_real_abs),
        ("RealSign", math::builtin_real_sign),
        ("RecurrenceTable", discrete::builtin_recurrence_table),
        ("RemoveContent", integration::builtin_remove_content),
        ("ReplacePart", list::builtin_replace_part),
        ("Rescale", math::builtin_rescale),
        ("Rest", list::builtin_rest),
        ("Reverse", list::builtin_reverse),
        ("Riffle", list::builtin_riffle),
        ("RotateLeft", list::builtin_rotate_left),
        ("RotateRight", list::builtin_rotate_right),
        ("Round", math::builtin_round),
        ("Rt", integration::builtin_rt),
        ("RunProcess", io::builtin_run_process),
        ("SameQ", comparison::builtin_same_q),
        ("ScientificForm", format::builtin_scientific_form),
        ("Sec", math::builtin_sec),
        ("SecDegrees", math::builtin_sec_degrees),
        ("Sech", math::builtin_sech),
        ("SentenceCount", string::builtin_sentence_count),
        ("Sequence", builtin_sequence),
        ("Shallow", format::builtin_shallow),
        ("Short", format::builtin_short),
        ("ShowStep", integration::builtin_show_step),
        ("Sign", arithmetic::builtin_sign),
        ("Simp", integration::builtin_simp),
        ("SimplerQ", integration::builtin_simpler_q),
        ("SimplerSqrtQ", integration::builtin_simpler_sqrt_q),
        ("Simplify", symbolic::builtin_simplify),
        ("Sin", math::builtin_sin),
        ("SinDegrees", math::builtin_sin_degrees),
        ("Sinc", math::builtin_sinc),
        ("Sinh", math::builtin_sinh),
        ("Solve", symbolic::builtin_solve),
        ("Sort", list::builtin_sort),
        ("Split", list::builtin_split),
        ("SplitProduct", integration::builtin_split_product),
        ("Sqrt", math::builtin_sqrt),
        ("SquareFreeQ", number_theory::builtin_square_free_q),
        ("StirlingS1", combinatorics::builtin_stirling_s1),
        ("StirlingS2", combinatorics::builtin_stirling_s2),
        ("StringCases", list::builtin_string_cases),
        ("StringContainsQ", string::builtin_string_contains_q),
        ("StringCount", string::builtin_string_count),
        ("StringDelete", string::builtin_string_delete),
        ("StringDrop", string::builtin_string_drop),
        ("StringEndsQ", string::builtin_string_ends_q),
        ("StringForm", format::builtin_string_form),
        ("StringFreeQ", string::builtin_string_free_q),
        ("StringInsert", string::builtin_string_insert),
        ("StringJoin", string::builtin_string_join),
        ("StringLength", string::builtin_string_length),
        ("StringMatchQ", string::builtin_string_match_q),
        ("StringPadLeft", string::builtin_string_pad_left),
        ("StringPadRight", string::builtin_string_pad_right),
        ("StringPart", string::builtin_string_part),
        ("StringPosition", string::builtin_string_position),
        ("StringRepeat", string::builtin_string_repeat),
        ("StringReplace", string::builtin_string_replace),
        ("StringReverse", string::builtin_string_reverse),
        ("StringRiffle", string::builtin_string_riffle),
        ("StringSplit", string::builtin_string_split),
        ("StringStartsQ", string::builtin_string_starts_q),
        ("StringTake", string::builtin_string_take),
        ("StringTrim", string::builtin_string_trim),
        ("Subfactorial", combinatorics::builtin_subfactorial),
        ("SubsetQ", operators::builtin_subset_q),
        ("Subsets", combinatorics::builtin_subsets),
        ("Subst", integration::builtin_subst),
        ("SubstFor", integration::builtin_subst_for),
        ("Sum", list::builtin_sum),
        ("SumQ", integration::builtin_sum_q),
        ("SumSimplerQ", integration::builtin_sum_simpler_q),
        ("SyntaxLength", format::builtin_syntax_length),
        ("SyntaxQ", format::builtin_syntax_q),
        ("Table", list::builtin_table),
        ("TableForm", format::builtin_table_form),
        ("Take", list::builtin_take),
        ("Tally", list::builtin_tally),
        ("Tan", math::builtin_tan),
        ("TanDegrees", math::builtin_tan_degrees),
        ("Tanh", math::builtin_tanh),
        ("TextWords", string::builtin_text_words),
        ("Thread", list::builtin_thread),
        ("Throw", error::builtin_throw),
        ("Times", arithmetic::builtin_times),
        ("ToCharacterCode", string::builtin_to_character_code),
        ("ToDigits", number_theory::builtin_to_digits),
        ("ToExpression", string::builtin_to_expression),
        ("ToLowerCase", string::builtin_to_lower_case),
        ("ToString", string::builtin_to_string),
        ("ToUpperCase", string::builtin_to_upper_case),
        ("Today", calendar::builtin_today),
        ("Together", symbolicmanip::builtin_together),
        ("Total", list::builtin_total),
        ("Transpose", list::builtin_transpose),
        ("TrigQ", integration::builtin_trig_q),
        ("TrinomialQ", integration::builtin_trinomial_q),
        ("TrueQ", integration::builtin_true_q),
        ("Tuples", combinatorics::builtin_tuples),
        ("TypeOf", pattern::builtin_type_of),
        ("Undulate", operators::builtin_undulate),
        ("Unequal", comparison::builtin_unequal),
        ("Unintegrable", integration::builtin_unintegrable),
        ("Union", list::builtin_union),
        ("UnitBox", math::builtin_unit_box),
        ("UnitStep", math::builtin_unit_step),
        ("UnitTriangle", math::builtin_unit_triangle),
        ("Unitize", math::builtin_unitize),
        ("UpperCaseQ", string::builtin_upper_case_q),
        ("Values", association::builtin_values),
        ("WordCount", string::builtin_word_count),
        ("Write", io::builtin_write),
        ("WriteLine", io::builtin_write_line),
        ("WriteString", io::builtin_write_string),
        ("Xor", logical::builtin_xor),
        ("ZeroQ", math::builtin_zero_q),
    ];

    for (name, func) in PURE_BUILTINS {
        register_builtin(env, name, *func);
    }

    // Env-aware builtins (need environment access)
    const ENV_BUILTINS: &[(&str, fn(&[Value], &Env) -> Result<Value, EvalError>)] = &[
        ("AllApply", list::builtin_all_apply),
        ("And", logical::builtin_and),
        ("Apply", list::builtin_apply),
        ("ArgMax", numericsolve::builtin_argmax),
        ("ArgMin", numericsolve::builtin_argmin),
        ("Array", list::builtin_array),
        ("Attributes", symbolic::builtin_attributes),
        ("BlockMap", list::builtin_block_map),
        ("Cases", pattern::builtin_cases),
        ("Clear", clearing::builtin_clear),
        ("ClearAll", clearing::builtin_clear_all),
        ("ClearAttributes", symbolic::builtin_clear_attributes),
        ("Composition", operators::builtin_composition),
        ("CountsBy", association::builtin_counts_by),
        ("Curry", operators::builtin_curry),
        ("D", symbolic::builtin_d),
        ("DeleteCases", pattern::builtin_delete_cases),
        ("DivisorSum", number_theory::builtin_divisor_sum),
        ("ExternalEvaluate", ffi::builtin_external_evaluate),
        ("FindInstance", numericsolve::builtin_find_instance),
        ("FindMaximum", numericsolve::builtin_find_maximum),
        ("FindMinimum", numericsolve::builtin_find_minimum),
        ("FindRoot", numericsolve::builtin_find_root),
        ("FixedPoint", math::builtin_fixed_point),
        ("FixedPointList", list::builtin_fixed_point_list),
        ("Fold", list::builtin_fold),
        ("FoldList", list::builtin_fold_list),
        ("FreeQ", pattern::builtin_free_q),
        ("GatherBy", list::builtin_gather_by),
        ("GroupBy", association::builtin_group_by),
        ("If", integration::builtin_if),
        ("Implies", logical::builtin_implies),
        ("Infix", builtin_infix),
        ("Inner", list::builtin_inner),
        ("Integrate", symbolic::builtin_integrate),
        ("KeyMap", association::builtin_key_map),
        ("KeySelect", association::builtin_key_select),
        ("KeySortBy", association::builtin_key_sort_by),
        ("KeyValueMap", association::builtin_key_value_map),
        ("LibraryFunction", ffi::builtin_library_function),
        ("LoadExtension", ffi::builtin_load_extension),
        ("LoadLibrary", ffi::builtin_load_library),
        ("Map", list::builtin_map),
        ("MapAll", operators::builtin_map_all),
        ("MapApply", list::builtin_map_apply),
        ("MapAt", list::builtin_map_at),
        ("MapIndexed", list::builtin_map_indexed),
        ("MapThread", list::builtin_map_thread),
        ("MatchQ", pattern::builtin_match_q),
        ("Merge", association::builtin_merge),
        ("Module", integration::builtin_module),
        ("NMaximize", numericsolve::builtin_nmaximize),
        ("NMinimize", numericsolve::builtin_nminimize),
        ("NSolve", numericsolve::builtin_nsolve),
        ("Names", names::builtin_names),
        ("Needs", builtin_needs),
        ("Nest", list::builtin_nest),
        ("NestList", list::builtin_nest_list),
        ("NestWhile", list::builtin_nest_while),
        ("NestWhileList", list::builtin_nest_while_list),
        ("OperatorApply", operators::builtin_operator_apply),
        ("Or", logical::builtin_or),
        ("Outer", list::builtin_outer),
        ("ParallelCombine", parallel::builtin_parallel_combine),
        ("ParallelMap", parallel::builtin_parallel_map),
        ("PositionFirst", operators::builtin_position_first),
        ("PositionLast", operators::builtin_position_last),
        ("Postfix", builtin_postfix),
        ("Prefix", builtin_prefix),
        ("RSolve", discrete::builtin_rsolve),
        ("Remove", clearing::builtin_remove),
        ("Replace", operators::builtin_replace),
        ("ReplaceAll", crate::eval::rules::builtin_replace_all),
        ("Scan", list::builtin_scan),
        ("Select", list::builtin_select),
        ("SelectFirst", operators::builtin_select_first),
        ("SelectLast", operators::builtin_select_last),
        ("Series", symbolic::builtin_series),
        ("SetAttributes", symbolic::builtin_set_attributes),
        ("SortBy", dataset::builtin_sort_by),
        ("SplitBy", list::builtin_split_by),
        ("Through", operators::builtin_through),
        ("UnCurry", operators::builtin_uncurry),
        ("Unset", clearing::builtin_unset),
        ("With", integration::builtin_with),
    ];

    for (name, func) in ENV_BUILTINS {
        register_builtin_env(env, name, *func);
    }

    // ── Non-data-driven registration ──
    domains::register(env);

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

    // ── Lazy package auto-loading ──
    register_lazy_package(
        env,
        linalg::SYMBOLS,
        linalg::SYMBOLS,
        "LinearAlgebra",
        linalg::register,
    );
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
    register_lazy_package(
        env,
        graphics::SYMBOLS,
        graphics::SYMBOLS,
        "Graphics",
        graphics::register,
    );
    register_lazy_package(
        env,
        charting::SYMBOLS,
        charting::SYMBOLS,
        "Charting",
        charting::register,
    );

    // -- Eagerly-registered packages --
    developer::register(env);
    systeminfo::register(env);
    algebraic::register(env);
    specialfunctions::register_sfs(env);

    // ── Add SYMA_HOME/Packages and SystemFiles to module search path ──
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

/// Helper: register a package's builtins and its module in the environment.
/// `register_fn` is called first (if provided), then a module is created from
/// `symbol_names` by looking up each symbol in the environment.
fn register_module_package<S: AsRef<str>>(
    env: &Env,
    name: &str,
    symbol_names: &[S],
    register_fn: Option<fn(&Env)>,
) {
    if let Some(register) = register_fn {
        register(env);
    }
    let exports: HashMap<String, Value> = symbol_names
        .iter()
        .filter_map(|sym| env.get(sym.as_ref()).map(|v| (sym.as_ref().to_string(), v)))
        .collect();
    let module = Value::Module {
        name: name.to_string(),
        exports,
        locals: HashMap::new(),
    };
    env.register_module(name.to_string(), module);
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
    match pkg_name.as_str() {
        "LinearAlgebra" => {
            register_module_package(
                env,
                "LinearAlgebra",
                &crate::builtins::linalg::SYMBOLS,
                Some(crate::builtins::linalg::register),
            );
        }
        "Statistics" => {
            register_module_package(
                env,
                "Statistics",
                &crate::builtins::statistics::SYMBOLS,
                Some(crate::builtins::statistics::register),
            );
        }
        "Graphics" => {
            register_module_package(
                env,
                "Graphics",
                &crate::builtins::graphics::SYMBOLS,
                Some(crate::builtins::graphics::register),
            );
        }
        "Charting" => {
            register_module_package(
                env,
                "Charting",
                &crate::builtins::charting::SYMBOLS,
                Some(crate::builtins::charting::register),
            );
        }
        "Developer" => {
            register_module_package(env, "Developer", &crate::builtins::developer::SYMBOLS, None);
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
