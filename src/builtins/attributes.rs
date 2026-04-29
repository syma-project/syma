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
        "NonCommutativeMultiply" => vec!["Flat", "Locked", "OneIdentity", "ReadProtected"],
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
        "D" | "Integrate" => vec!["HoldAll", "Locked", "ReadProtected"],
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
        | "PrimeNu" | "PositiveQ" | "NegativeQ" | "NonNegativeQ" | "ZeroQ" => llr(),
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
