//! quantities and units: Quantity, UnitConvert, UnitSimplify, QuantityMagnitude, QuantityUnit; basic Entity.
//!
//! Quantities are represented as `Value::Call { head: "Quantity", args: [magnitude, unit] }`.
//! The unit is a symbolic expression built from unit-name strings, `Times`, `Divide`
//! and `Power` (for example `"Meter"/"Second"`). Units are resolved through a small
//! offline table of SI base units, common imperial units and prefixed forms.
//!
//! Arithmetic on quantities is provided by thin wrappers around the numeric
//! implementations: adding compatible quantities converts to a common unit,
//! multiplying/dividing quantities combines their units symbolically (without
//! converting — matching Wolfram Language behaviour), and `Power`/`Minus`/`Abs`
//! act on the magnitude while scaling the unit.
//!
//! Functions return the unevaluated `Value::Call { head, args }` when a unit or
//! entity is unknown.

use super::arithmetic;
use crate::env::Env;
use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;
use rug::Rational;
use std::collections::BTreeMap;

// ── Small value helpers ──────────────────────────────────────────────────────

fn int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

fn rf(f: f64) -> Value {
    Value::Real(Float::with_val(DEFAULT_PRECISION, f))
}

fn is_zero(v: &Value) -> bool {
    match v {
        Value::Integer(n) => n.is_zero(),
        Value::Rational(r) => r.as_ref().is_zero(),
        Value::Real(r) => r.is_zero(),
        _ => false,
    }
}

fn value_as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Integer(n) => Some(n.to_f64()),
        Value::Real(r) => Some(r.to_f64()),
        Value::Rational(r) => Some(r.as_ref().to_f64()),
        _ => None,
    }
}

fn value_as_i64(v: &Value) -> Option<i64> {
    match v {
        Value::Integer(n) => i64::try_from(n).ok(),
        _ => None,
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().collect::<String>() + chars.as_str(),
        None => String::new(),
    }
}

/// A unit definition: canonical name, base-dimension exponents and the
/// multiplicative factor to SI base units.
struct UnitDef {
    name: &'static str,
    dims: &'static [(&'static str, i32)],
    factor: f64,
}

// Base dimensions (Wolfram-style names) and common derived units. Factors are
// relative to SI base units.
const UNITS: &[UnitDef] = &[
    UnitDef {
        name: "Meter",
        dims: &[("LengthUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Angstrom",
        dims: &[("LengthUnit", 1)],
        factor: 1e-10,
    },
    UnitDef {
        name: "Inch",
        dims: &[("LengthUnit", 1)],
        factor: 0.0254,
    },
    UnitDef {
        name: "Foot",
        dims: &[("LengthUnit", 1)],
        factor: 0.3048,
    },
    UnitDef {
        name: "Yard",
        dims: &[("LengthUnit", 1)],
        factor: 0.9144,
    },
    UnitDef {
        name: "Mile",
        dims: &[("LengthUnit", 1)],
        factor: 1609.344,
    },
    UnitDef {
        name: "NauticalMile",
        dims: &[("LengthUnit", 1)],
        factor: 1852.0,
    },
    UnitDef {
        name: "AstronomicalUnit",
        dims: &[("LengthUnit", 1)],
        factor: 1.495_978_707e11,
    },
    UnitDef {
        name: "LightYear",
        dims: &[("LengthUnit", 1)],
        factor: 9.460_730_472_580_8e15,
    },
    UnitDef {
        name: "Parsec",
        dims: &[("LengthUnit", 1)],
        factor: 3.085_677_581_491_367e16,
    },
    UnitDef {
        name: "Second",
        dims: &[("TimeUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Minute",
        dims: &[("TimeUnit", 1)],
        factor: 60.0,
    },
    UnitDef {
        name: "Hour",
        dims: &[("TimeUnit", 1)],
        factor: 3600.0,
    },
    UnitDef {
        name: "Day",
        dims: &[("TimeUnit", 1)],
        factor: 86400.0,
    },
    UnitDef {
        name: "Week",
        dims: &[("TimeUnit", 1)],
        factor: 604800.0,
    },
    UnitDef {
        name: "Year",
        dims: &[("TimeUnit", 1)],
        factor: 31_536_000.0,
    },
    UnitDef {
        name: "Kilogram",
        dims: &[("MassUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Gram",
        dims: &[("MassUnit", 1)],
        factor: 1e-3,
    },
    UnitDef {
        name: "Tonne",
        dims: &[("MassUnit", 1)],
        factor: 1000.0,
    },
    UnitDef {
        name: "Pound",
        dims: &[("MassUnit", 1)],
        factor: 0.453_592_37,
    },
    UnitDef {
        name: "Ounce",
        dims: &[("MassUnit", 1)],
        factor: 0.028_349_523_125,
    },
    UnitDef {
        name: "Stone",
        dims: &[("MassUnit", 1)],
        factor: 6.350_293_18,
    },
    UnitDef {
        name: "Ampere",
        dims: &[("ElectricCurrentUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Kelvin",
        dims: &[("TemperatureUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Mole",
        dims: &[("AmountUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Candela",
        dims: &[("LuminousIntensityUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Radian",
        dims: &[("AngleUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Degree",
        dims: &[("AngleUnit", 1)],
        factor: 0.017_453_292_519_943_295,
    },
    UnitDef {
        name: "Steradian",
        dims: &[("SolidAngleUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Bit",
        dims: &[("InformationUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Byte",
        dims: &[("InformationUnit", 1)],
        factor: 8.0,
    },
    UnitDef {
        name: "USDollar",
        dims: &[("MoneyUnit", 1)],
        factor: 1.0,
    },
    // Dimensionless units.
    UnitDef {
        name: "Percent",
        dims: &[],
        factor: 0.01,
    },
    UnitDef {
        name: "PartsPerMillion",
        dims: &[],
        factor: 1e-6,
    },
    UnitDef {
        name: "Dozen",
        dims: &[],
        factor: 12.0,
    },
    UnitDef {
        name: "Score",
        dims: &[],
        factor: 20.0,
    },
    // Derived SI units.
    UnitDef {
        name: "Hertz",
        dims: &[("TimeUnit", -1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Newton",
        dims: &[("LengthUnit", 1), ("MassUnit", 1), ("TimeUnit", -2)],
        factor: 1.0,
    },
    UnitDef {
        name: "Pascal",
        dims: &[("LengthUnit", -1), ("MassUnit", 1), ("TimeUnit", -2)],
        factor: 1.0,
    },
    UnitDef {
        name: "Joule",
        dims: &[("LengthUnit", 2), ("MassUnit", 1), ("TimeUnit", -2)],
        factor: 1.0,
    },
    UnitDef {
        name: "Watt",
        dims: &[("LengthUnit", 2), ("MassUnit", 1), ("TimeUnit", -3)],
        factor: 1.0,
    },
    UnitDef {
        name: "Coulomb",
        dims: &[("ElectricCurrentUnit", 1), ("TimeUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Volt",
        dims: &[
            ("ElectricCurrentUnit", -1),
            ("LengthUnit", 2),
            ("MassUnit", 1),
            ("TimeUnit", -3),
        ],
        factor: 1.0,
    },
    UnitDef {
        name: "Ohm",
        dims: &[
            ("ElectricCurrentUnit", -2),
            ("LengthUnit", 2),
            ("MassUnit", 1),
            ("TimeUnit", -3),
        ],
        factor: 1.0,
    },
    UnitDef {
        name: "Siemens",
        dims: &[
            ("ElectricCurrentUnit", 2),
            ("LengthUnit", -2),
            ("MassUnit", -1),
            ("TimeUnit", 3),
        ],
        factor: 1.0,
    },
    UnitDef {
        name: "Farad",
        dims: &[
            ("ElectricCurrentUnit", 2),
            ("LengthUnit", -2),
            ("MassUnit", -1),
            ("TimeUnit", 4),
        ],
        factor: 1.0,
    },
    UnitDef {
        name: "Weber",
        dims: &[
            ("ElectricCurrentUnit", -1),
            ("LengthUnit", 2),
            ("MassUnit", 1),
            ("TimeUnit", -2),
        ],
        factor: 1.0,
    },
    UnitDef {
        name: "Tesla",
        dims: &[
            ("ElectricCurrentUnit", -1),
            ("MassUnit", 1),
            ("TimeUnit", -2),
        ],
        factor: 1.0,
    },
    UnitDef {
        name: "Henry",
        dims: &[
            ("ElectricCurrentUnit", -2),
            ("LengthUnit", 2),
            ("MassUnit", 1),
            ("TimeUnit", -2),
        ],
        factor: 1.0,
    },
    UnitDef {
        name: "Lumen",
        dims: &[("LuminousIntensityUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Lux",
        dims: &[("LengthUnit", -2), ("LuminousIntensityUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Becquerel",
        dims: &[("TimeUnit", -1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Gray",
        dims: &[("LengthUnit", 2), ("TimeUnit", -2)],
        factor: 1.0,
    },
    UnitDef {
        name: "Sievert",
        dims: &[("LengthUnit", 2), ("TimeUnit", -2)],
        factor: 1.0,
    },
    UnitDef {
        name: "Katal",
        dims: &[("AmountUnit", 1), ("TimeUnit", -1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Liter",
        dims: &[("LengthUnit", 3)],
        factor: 1e-3,
    },
];

/// Preferred derived units used by `UnitSimplify`.
const DERIVED_UNITS: &[&str] = &[
    "Hertz",
    "Newton",
    "Pascal",
    "Joule",
    "Watt",
    "Coulomb",
    "Volt",
    "Farad",
    "Ohm",
    "Siemens",
    "Weber",
    "Tesla",
    "Henry",
    "Lumen",
    "Lux",
    "Becquerel",
    "Gray",
    "Sievert",
    "Katal",
    "Liter",
];

/// Alternate spellings (lower-cased) mapped to a canonical unit name.
const ALIASES: &[(&str, &str)] = &[
    ("metre", "Meter"),
    ("meters", "Meter"),
    ("kilometre", "Kilometer"),
    ("litre", "Liter"),
    ("liters", "Liter"),
    ("feet", "Foot"),
    ("inches", "Inch"),
    ("pounds", "Pound"),
    ("ounces", "Ounce"),
    ("miles", "Mile"),
    ("yards", "Yard"),
    ("seconds", "Second"),
    ("minutes", "Minute"),
    ("hours", "Hour"),
    ("days", "Day"),
    ("weeks", "Week"),
    ("years", "Year"),
    ("grams", "Gram"),
    ("kilograms", "Kilogram"),
    ("amperes", "Ampere"),
    ("kelvins", "Kelvin"),
    ("moles", "Mole"),
    ("candelas", "Candela"),
    ("radians", "Radian"),
    ("degrees", "Degree"),
    ("steradians", "Steradian"),
    ("bits", "Bit"),
    ("bytes", "Byte"),
    ("percent", "Percent"),
    ("partspermillion", "PartsPerMillion"),
    ("scores", "Score"),
    ("dozens", "Dozen"),
    ("hertz", "Hertz"),
    ("newtons", "Newton"),
    ("pascals", "Pascal"),
    ("joules", "Joule"),
    ("watts", "Watt"),
    ("coulombs", "Coulomb"),
    ("volts", "Volt"),
    ("ohms", "Ohm"),
    ("farads", "Farad"),
    ("webers", "Weber"),
    ("teslas", "Tesla"),
    ("henries", "Henry"),
];

/// SI-style prefixes (prefix spelling, factor).
const PREFIXES: &[(&str, f64)] = &[
    ("Yotta", 1e24),
    ("Zetta", 1e21),
    ("Exa", 1e18),
    ("Peta", 1e15),
    ("Tera", 1e12),
    ("Giga", 1e9),
    ("Mega", 1e6),
    ("Kilo", 1e3),
    ("Hecto", 1e2),
    ("Deca", 10.0),
    ("Deci", 1e-1),
    ("Centi", 1e-2),
    ("Milli", 1e-3),
    ("Micro", 1e-6),
    ("Nano", 1e-9),
    ("Pico", 1e-12),
    ("Femto", 1e-15),
];

struct Resolved {
    name: String,
    dims: Vec<(String, i64)>,
    factor: f64,
}

fn resolve_from_table(lname: &str) -> Option<Resolved> {
    for def in UNITS {
        if def.name.to_lowercase() == lname {
            return Some(Resolved {
                name: def.name.to_string(),
                dims: def
                    .dims
                    .iter()
                    .map(|(d, e)| (d.to_string(), *e as i64))
                    .collect(),
                factor: def.factor,
            });
        }
    }
    for (alias, canonical) in ALIASES {
        if *alias == lname {
            return resolve_base(&canonical.to_lowercase());
        }
    }
    None
}

/// Resolve a bare (prefix-free) unit name.
fn resolve_base(lname: &str) -> Option<Resolved> {
    if let Some(r) = resolve_from_table(lname) {
        return Some(r);
    }
    if let Some(singular) = lname.strip_suffix('s')
        && !singular.is_empty()
    {
        return resolve_base(singular);
    }
    None
}

/// Resolve a unit name, applying an SI prefix if present.
fn resolve_name(name: &str) -> Option<Resolved> {
    let lname = name.to_lowercase();
    if let Some(r) = resolve_base(&lname) {
        return Some(r);
    }
    for (prefix, factor) in PREFIXES {
        if let Some(rest) = lname.strip_prefix(&prefix.to_lowercase())
            && !rest.is_empty()
            && let Some(base) = resolve_base(rest)
        {
            return Some(Resolved {
                name: format!("{}{}", capitalize(prefix), base.name),
                dims: base.dims,
                factor: base.factor * factor,
            });
        }
    }
    None
}

const DIM_TO_BASE: &[(&str, &str)] = &[
    ("LengthUnit", "Meter"),
    ("MassUnit", "Kilogram"),
    ("TimeUnit", "Second"),
    ("ElectricCurrentUnit", "Ampere"),
    ("TemperatureUnit", "Kelvin"),
    ("AmountUnit", "Mole"),
    ("LuminousIntensityUnit", "Candela"),
    ("AngleUnit", "Radian"),
    ("SolidAngleUnit", "Steradian"),
    ("InformationUnit", "Bit"),
    ("MoneyUnit", "USDollar"),
];

/// A symbolic unit expression: unit-name → integer exponent.
#[derive(Clone, Default, PartialEq, Eq)]
struct UnitExpr {
    factors: BTreeMap<String, i64>,
}

impl UnitExpr {
    fn single(name: String) -> Self {
        let mut factors = BTreeMap::new();
        factors.insert(name, 1);
        UnitExpr { factors }
    }

    fn mul(mut self, other: &UnitExpr) -> Self {
        for (name, exp) in &other.factors {
            *self.factors.entry(name.clone()).or_insert(0) += *exp;
        }
        self.factors.retain(|_, e| *e != 0);
        self
    }

    fn div(mut self, other: &UnitExpr) -> Self {
        for (name, exp) in &other.factors {
            *self.factors.entry(name.clone()).or_insert(0) -= *exp;
        }
        self.factors.retain(|_, e| *e != 0);
        self
    }

    fn pow(mut self, exp: i64) -> Self {
        for v in self.factors.values_mut() {
            *v *= exp;
        }
        self.factors.retain(|_, e| *e != 0);
        self
    }

    fn is_empty(&self) -> bool {
        self.factors.is_empty()
    }

    /// Render back into a symbolic `Value` (`"Meter"`, `"Meter"/"Second"`, …).
    fn to_value(&self) -> Value {
        let mut pos = Vec::new();
        let mut neg = Vec::new();
        for (name, exp) in &self.factors {
            let base = Value::Str(name.clone());
            let term = if exp.unsigned_abs() == 1 {
                base
            } else {
                Value::Call {
                    head: "Power".to_string(),
                    args: vec![base, int(exp.abs())],
                }
            };
            if *exp > 0 {
                pos.push(term);
            } else {
                neg.push(term);
            }
        }
        match (product(pos), product(neg)) {
            (Some(p), None) => p,
            (None, Some(n)) => Value::Call {
                head: "Divide".to_string(),
                args: vec![int(1), n],
            },
            (Some(p), Some(n)) => Value::Call {
                head: "Divide".to_string(),
                args: vec![p, n],
            },
            (None, None) => int(1),
        }
    }
}

fn product(mut terms: Vec<Value>) -> Option<Value> {
    match terms.len() {
        0 => None,
        1 => terms.pop(),
        _ => Some(Value::Call {
            head: "Times".to_string(),
            args: terms,
        }),
    }
}

fn unit_from_dims(dims: &[(String, i64)]) -> UnitExpr {
    let mut factors: BTreeMap<String, i64> = BTreeMap::new();
    for (dim, exp) in dims {
        if let Some((_, base)) = DIM_TO_BASE.iter().find(|(d, _)| d == dim) {
            *factors.entry((*base).to_string()).or_insert(0) += *exp;
        }
    }
    UnitExpr { factors }
}

fn dims_factor(ue: &UnitExpr) -> (Vec<(String, i64)>, f64) {
    let mut dims: BTreeMap<String, i64> = BTreeMap::new();
    let mut factor = 1.0f64;
    for (name, exp) in &ue.factors {
        match resolve_name(name) {
            Some(r) => {
                for (d, e) in r.dims {
                    *dims.entry(d).or_insert(0) += e * exp;
                }
                factor *= r.factor.powi(*exp as i32);
            }
            None => {
                *dims.entry(format!("{name}Unit")).or_insert(0) += *exp;
            }
        }
    }
    let mut out: Vec<(String, i64)> = dims.into_iter().filter(|(_, e)| *e != 0).collect();
    out.sort();
    (out, factor)
}

/// Parse a unit expression value into a `UnitExpr`.
fn parse_unit(v: &Value) -> Option<UnitExpr> {
    match v {
        Value::Str(s) | Value::Symbol(s) => {
            if let Some(r) = resolve_name(s) {
                Some(UnitExpr::single(r.name))
            } else if s.is_empty() {
                None
            } else {
                Some(UnitExpr::single(s.clone()))
            }
        }
        Value::Integer(n) if *n == 1 => Some(UnitExpr::default()),
        Value::Call { head, args } if head == "Divide" && args.len() == 2 => {
            Some(parse_unit(&args[0])?.div(&parse_unit(&args[1])?))
        }
        Value::Call { head, args } if head == "Times" => {
            let mut acc = UnitExpr::default();
            for a in args {
                acc = acc.mul(&parse_unit(a)?);
            }
            Some(acc)
        }
        Value::Call { head, args } if head == "Power" && args.len() == 2 => {
            let exp = value_as_i64(&args[1])?;
            Some(parse_unit(&args[0])?.pow(exp))
        }
        _ => None,
    }
}

fn quantity_parts(v: &Value) -> Option<(&Value, &Value)> {
    if let Value::Call { head, args } = v
        && head == "Quantity"
        && args.len() == 2
    {
        return Some((&args[0], &args[1]));
    }
    None
}

fn is_quantity(v: &Value) -> bool {
    quantity_parts(v).is_some()
}

fn make_quantity(mag: Value, unit: &UnitExpr) -> Value {
    if unit.is_empty() {
        mag
    } else {
        Value::Call {
            head: "Quantity".to_string(),
            args: vec![mag, unit.to_value()],
        }
    }
}

fn unevaluated(head: &str, args: &[Value]) -> Value {
    Value::Call {
        head: head.to_string(),
        args: args.to_vec(),
    }
}

/// Multiply a magnitude by a scale factor, keeping exact integers when the
/// factor is an exact whole number.
fn scale_mag(mag: &Value, ratio: f64) -> Result<Value, EvalError> {
    if ratio == 1.0 {
        return Ok(mag.clone());
    }
    if ratio.fract() == 0.0 && ratio.abs() < 9.0e15 {
        let k = ratio as i64;
        match mag {
            Value::Integer(n) => return Ok(Value::Integer(n.clone() * Integer::from(k))),
            Value::Rational(r) => {
                let prod: Rational = r.as_ref() * Rational::from(k);
                let (num, den) = prod.into_numer_denom();
                return Ok(crate::value::rational_value(num, den));
            }
            _ => {}
        }
    }
    arithmetic::mul_values(mag, &rf(ratio))
}

/// Convert `mag` from unit `src` to unit `dst`; `None` if dimensions differ.
fn to_unit(mag: &Value, src: &UnitExpr, dst: &UnitExpr) -> Option<Value> {
    let (src_dims, src_factor) = dims_factor(src);
    let (dst_dims, dst_factor) = dims_factor(dst);
    if src_dims != dst_dims {
        return None;
    }
    if src == dst {
        return Some(mag.clone());
    }
    scale_mag(mag, src_factor / dst_factor).ok()
}

// ── Quantity constructors / accessors ────────────────────────────────────────

pub fn builtin_quantity(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => match parse_unit(&args[0]) {
            Some(ue) => Ok(make_quantity(int(1), &ue)),
            None => Ok(unevaluated("Quantity", args)),
        },
        2 => match parse_unit(&args[1]) {
            Some(ue) => Ok(make_quantity(args[0].clone(), &ue)),
            None => Ok(unevaluated("Quantity", args)),
        },
        _ => Err(EvalError::Error(
            "Quantity requires 1 or 2 arguments".to_string(),
        )),
    }
}

pub fn builtin_quantity_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "QuantityQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(is_quantity(&args[0])))
}

pub fn builtin_quantity_magnitude(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => match quantity_parts(&args[0]) {
            Some((mag, _)) => Ok(mag.clone()),
            None => Ok(unevaluated("QuantityMagnitude", args)),
        },
        2 => {
            let Some((mag, unit)) = quantity_parts(&args[0]) else {
                return Ok(unevaluated("QuantityMagnitude", args));
            };
            let (Some(src), Some(dst)) = (parse_unit(unit), parse_unit(&args[1])) else {
                return Ok(unevaluated("QuantityMagnitude", args));
            };
            match to_unit(mag, &src, &dst) {
                Some(converted) => Ok(converted),
                None => Ok(unevaluated("QuantityMagnitude", args)),
            }
        }
        _ => Err(EvalError::Error(
            "QuantityMagnitude requires 1 or 2 arguments".to_string(),
        )),
    }
}

pub fn builtin_quantity_unit(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => match quantity_parts(&args[0]) {
            Some((_, unit)) => Ok(unit.clone()),
            None => Ok(unevaluated("QuantityUnit", args)),
        },
        2 => {
            let Some((mag, unit)) = quantity_parts(&args[0]) else {
                return Ok(unevaluated("QuantityUnit", args));
            };
            let (Some(src), Some(dst)) = (parse_unit(unit), parse_unit(&args[1])) else {
                return Ok(unevaluated("QuantityUnit", args));
            };
            match to_unit(mag, &src, &dst) {
                Some(converted) => Ok(make_quantity(converted, &dst)),
                None => Ok(unevaluated("QuantityUnit", args)),
            }
        }
        _ => Err(EvalError::Error(
            "QuantityUnit requires 1 or 2 arguments".to_string(),
        )),
    }
}

// ── Conversion / inspection ──────────────────────────────────────────────────

pub fn builtin_unit_convert(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => {
            let Some((mag, unit)) = quantity_parts(&args[0]) else {
                return Ok(unevaluated("UnitConvert", args));
            };
            let Some(src) = parse_unit(unit) else {
                return Ok(unevaluated("UnitConvert", args));
            };
            let (dims, factor) = dims_factor(&src);
            let converted = scale_mag(mag, factor)?;
            Ok(make_quantity(converted, &unit_from_dims(&dims)))
        }
        2 => {
            let Some((mag, unit)) = quantity_parts(&args[0]) else {
                return Ok(unevaluated("UnitConvert", args));
            };
            let (Some(src), Some(dst)) = (parse_unit(unit), parse_unit(&args[1])) else {
                return Ok(unevaluated("UnitConvert", args));
            };
            match to_unit(mag, &src, &dst) {
                Some(converted) => Ok(make_quantity(converted, &dst)),
                None => Ok(unevaluated("UnitConvert", args)),
            }
        }
        _ => Err(EvalError::Error(
            "UnitConvert requires 1 or 2 arguments".to_string(),
        )),
    }
}

pub fn builtin_unit_simplify(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "UnitSimplify requires exactly 1 argument".to_string(),
        ));
    }
    if let Value::List(items) = &args[0] {
        let mapped: Result<Vec<_>, _> = items
            .iter()
            .map(|i| builtin_unit_simplify(std::slice::from_ref(i)))
            .collect();
        return Ok(Value::List(mapped?));
    }
    let Some((mag, unit)) = quantity_parts(&args[0]) else {
        return Ok(unevaluated("UnitSimplify", args));
    };
    let Some(src) = parse_unit(unit) else {
        return Ok(unevaluated("UnitSimplify", args));
    };
    if src.factors.len() <= 1 {
        return Ok(args[0].clone());
    }
    let (dims, _) = dims_factor(&src);
    for name in DERIVED_UNITS {
        if let Some(r) = resolve_name(name)
            && r.dims == dims
            && let Some(converted) = to_unit(mag, &src, &UnitExpr::single(r.name.clone()))
        {
            let dst = UnitExpr::single(r.name);
            return Ok(make_quantity(converted, &dst));
        }
    }
    Ok(args[0].clone())
}

pub fn builtin_compatible_unit_q(args: &[Value]) -> Result<Value, EvalError> {
    let mut items: Vec<&Value> = Vec::new();
    for a in args {
        match a {
            Value::List(inner) => items.extend(inner.iter()),
            other => items.push(other),
        }
    }
    if items.is_empty() {
        return Ok(Value::Bool(false));
    }
    let mut reference: Option<Vec<(String, i64)>> = None;
    for item in items {
        let ue = if let Some((_, unit)) = quantity_parts(item) {
            parse_unit(unit)
        } else if value_as_f64(item).is_some() {
            Some(UnitExpr::default())
        } else {
            parse_unit(item)
        };
        let Some(ue) = ue else {
            return Ok(Value::Bool(false));
        };
        let (dims, _) = dims_factor(&ue);
        match &reference {
            None => reference = Some(dims),
            Some(r) if *r == dims => {}
            Some(_) => return Ok(Value::Bool(false)),
        }
    }
    Ok(Value::Bool(true))
}

pub fn builtin_unit_dimensions(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "UnitDimensions requires exactly 1 argument".to_string(),
        ));
    }
    if let Value::List(items) = &args[0] {
        let mapped: Result<Vec<_>, _> = items
            .iter()
            .map(|i| builtin_unit_dimensions(std::slice::from_ref(i)))
            .collect();
        return Ok(Value::List(mapped?));
    }
    let ue = if let Some((_, unit)) = quantity_parts(&args[0]) {
        parse_unit(unit)
    } else if value_as_f64(&args[0]).is_some() {
        Some(UnitExpr::default())
    } else {
        parse_unit(&args[0])
    };
    let Some(ue) = ue else {
        return Ok(unevaluated("UnitDimensions", args));
    };
    let (dims, _) = dims_factor(&ue);
    let pairs: Vec<Value> = dims
        .into_iter()
        .map(|(d, e)| Value::List(vec![Value::Str(d), int(e)]))
        .collect();
    Ok(Value::List(pairs))
}

pub fn builtin_known_unit_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "KnownUnitQ requires exactly 1 argument".to_string(),
        ));
    }
    let known = if let Some((_, unit)) = quantity_parts(&args[0]) {
        parse_unit(unit)
    } else {
        parse_unit(&args[0])
    };
    let known = known.is_some_and(|ue| ue.factors.keys().all(|k| resolve_name(k).is_some()));
    Ok(Value::Bool(known))
}

// ── Arithmetic wrappers ──────────────────────────────────────────────────────

/// Combine a group of quantities into a single quantity when their units are
/// compatible; otherwise return the original terms unchanged.
fn combine_quantities(qs: &[Value]) -> Result<Vec<Value>, EvalError> {
    if qs.len() < 2 {
        return Ok(qs.to_vec());
    }
    let Some((_, first_unit)) = quantity_parts(&qs[0]) else {
        return Ok(qs.to_vec());
    };
    let Some(base_ue) = parse_unit(first_unit) else {
        return Ok(qs.to_vec());
    };
    let (base_dims, _) = dims_factor(&base_ue);
    let mut terms = Vec::with_capacity(qs.len());
    for q in qs {
        let (mag, unit) = quantity_parts(q).expect("is_quantity checked");
        let Some(ue) = parse_unit(unit) else {
            return Ok(qs.to_vec());
        };
        let (dims, _) = dims_factor(&ue);
        if dims != base_dims {
            return Ok(qs.to_vec());
        }
        match to_unit(mag, &ue, &base_ue) {
            Some(t) => terms.push(t),
            None => return Ok(qs.to_vec()),
        }
    }
    let sum = arithmetic::builtin_plus(&terms)?;
    Ok(vec![make_quantity(sum, &base_ue)])
}

pub fn q_plus(args: &[Value]) -> Result<Value, EvalError> {
    let qs: Vec<Value> = args.iter().filter(|a| is_quantity(a)).cloned().collect();
    let others: Vec<Value> = args.iter().filter(|a| !is_quantity(a)).cloned().collect();
    if qs.is_empty() {
        return arithmetic::builtin_plus(args);
    }
    let other_sum = arithmetic::builtin_plus(&others)?;
    let mut terms = combine_quantities(&qs)?;
    if !is_zero(&other_sum) {
        terms.push(other_sum);
    }
    // Delegate to the core combiner so like terms are folded consistently.
    arithmetic::builtin_plus(&terms)
}

pub fn q_times(args: &[Value]) -> Result<Value, EvalError> {
    if !args.iter().any(is_quantity) {
        return arithmetic::builtin_times(args);
    }
    let mut mag = int(1);
    let mut unit = UnitExpr::default();
    let mut scalars = Vec::new();
    for a in args {
        if let Some((m, u)) = quantity_parts(a) {
            let Some(ue) = parse_unit(u) else {
                return arithmetic::builtin_times(args);
            };
            mag = arithmetic::mul_values(&mag, m)?;
            unit = unit.mul(&ue);
        } else {
            scalars.push(a.clone());
        }
    }
    let scalar = arithmetic::builtin_times(&scalars)?;
    let mag = arithmetic::mul_values(&mag, &scalar)?;
    Ok(make_quantity(mag, &unit))
}

pub fn q_divide(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 || !(is_quantity(&args[0]) || is_quantity(&args[1])) {
        return arithmetic::builtin_divide(args);
    }
    let (mag0, unit0) = match quantity_parts(&args[0]) {
        Some((m, u)) => match parse_unit(u) {
            Some(ue) => (m.clone(), ue),
            None => return arithmetic::builtin_divide(args),
        },
        None => (args[0].clone(), UnitExpr::default()),
    };
    let (mag1, unit1) = match quantity_parts(&args[1]) {
        Some((m, u)) => match parse_unit(u) {
            Some(ue) => (m.clone(), ue),
            None => return arithmetic::builtin_divide(args),
        },
        None => (args[1].clone(), UnitExpr::default()),
    };
    let mag = arithmetic::builtin_divide(&[mag0, mag1])?;
    Ok(make_quantity(mag, &unit0.div(&unit1)))
}

pub fn q_power(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 || !is_quantity(&args[0]) {
        return arithmetic::builtin_power(args);
    }
    let Some((mag, unit)) = quantity_parts(&args[0]) else {
        return arithmetic::builtin_power(args);
    };
    let Some(exp) = value_as_i64(&args[1]) else {
        return Ok(unevaluated("Power", args));
    };
    let Some(ue) = parse_unit(unit) else {
        return arithmetic::builtin_power(args);
    };
    let new_mag = arithmetic::builtin_power(&[mag.clone(), args[1].clone()])?;
    Ok(make_quantity(new_mag, &ue.pow(exp)))
}

pub fn q_minus(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 || !is_quantity(&args[0]) {
        return arithmetic::builtin_minus(args);
    }
    let Some((mag, unit)) = quantity_parts(&args[0]) else {
        return arithmetic::builtin_minus(args);
    };
    let Some(ue) = parse_unit(unit) else {
        return arithmetic::builtin_minus(args);
    };
    let new_mag = arithmetic::builtin_minus(&[mag.clone()])?;
    Ok(make_quantity(new_mag, &ue))
}

pub fn q_abs(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 || !is_quantity(&args[0]) {
        return arithmetic::builtin_abs(args);
    }
    let Some((mag, unit)) = quantity_parts(&args[0]) else {
        return arithmetic::builtin_abs(args);
    };
    let Some(ue) = parse_unit(unit) else {
        return arithmetic::builtin_abs(args);
    };
    let new_mag = arithmetic::builtin_abs(&[mag.clone()])?;
    Ok(make_quantity(new_mag, &ue))
}

// ── Entity (small offline table) ─────────────────────────────────────────────

struct Element {
    name: &'static str,
    atomic_number: i64,
    atomic_mass: f64,
    symbol: &'static str,
    group: i64,
    period: i64,
}

const ELEMENTS: &[Element] = &[
    Element {
        name: "Hydrogen",
        atomic_number: 1,
        atomic_mass: 1.008,
        symbol: "H",
        group: 1,
        period: 1,
    },
    Element {
        name: "Helium",
        atomic_number: 2,
        atomic_mass: 4.0026,
        symbol: "He",
        group: 18,
        period: 1,
    },
    Element {
        name: "Carbon",
        atomic_number: 6,
        atomic_mass: 12.011,
        symbol: "C",
        group: 14,
        period: 2,
    },
    Element {
        name: "Nitrogen",
        atomic_number: 7,
        atomic_mass: 14.007,
        symbol: "N",
        group: 15,
        period: 2,
    },
    Element {
        name: "Oxygen",
        atomic_number: 8,
        atomic_mass: 15.999,
        symbol: "O",
        group: 16,
        period: 2,
    },
    Element {
        name: "Sodium",
        atomic_number: 11,
        atomic_mass: 22.99,
        symbol: "Na",
        group: 1,
        period: 3,
    },
    Element {
        name: "Aluminium",
        atomic_number: 13,
        atomic_mass: 26.982,
        symbol: "Al",
        group: 13,
        period: 3,
    },
    Element {
        name: "Iron",
        atomic_number: 26,
        atomic_mass: 55.845,
        symbol: "Fe",
        group: 8,
        period: 4,
    },
    Element {
        name: "Copper",
        atomic_number: 29,
        atomic_mass: 63.546,
        symbol: "Cu",
        group: 11,
        period: 4,
    },
    Element {
        name: "Silver",
        atomic_number: 47,
        atomic_mass: 107.868,
        symbol: "Ag",
        group: 11,
        period: 5,
    },
    Element {
        name: "Gold",
        atomic_number: 79,
        atomic_mass: 196.967,
        symbol: "Au",
        group: 11,
        period: 6,
    },
    Element {
        name: "Uranium",
        atomic_number: 92,
        atomic_mass: 238.029,
        symbol: "U",
        group: 3,
        period: 7,
    },
];

struct Country {
    name: &'static str,
    capital: &'static str,
    population: i64,
    area: f64,
    currency: &'static str,
    language: &'static str,
}

const COUNTRIES: &[Country] = &[
    Country {
        name: "France",
        capital: "Paris",
        population: 68_000_000,
        area: 551_695.0,
        currency: "Euro",
        language: "French",
    },
    Country {
        name: "Germany",
        capital: "Berlin",
        population: 83_000_000,
        area: 357_022.0,
        currency: "Euro",
        language: "German",
    },
    Country {
        name: "Japan",
        capital: "Tokyo",
        population: 125_000_000,
        area: 377_975.0,
        currency: "Yen",
        language: "Japanese",
    },
    Country {
        name: "Brazil",
        capital: "Brasilia",
        population: 214_000_000,
        area: 8_515_767.0,
        currency: "Real",
        language: "Portuguese",
    },
    Country {
        name: "Canada",
        capital: "Ottawa",
        population: 38_000_000,
        area: 9_984_670.0,
        currency: "CanadianDollar",
        language: "English",
    },
    Country {
        name: "Australia",
        capital: "Canberra",
        population: 26_000_000,
        area: 7_692_024.0,
        currency: "AustralianDollar",
        language: "English",
    },
    Country {
        name: "India",
        capital: "NewDelhi",
        population: 1_400_000_000,
        area: 3_287_263.0,
        currency: "Rupee",
        language: "Hindi",
    },
    Country {
        name: "UnitedStates",
        capital: "WashingtonDC",
        population: 332_000_000,
        area: 9_833_517.0,
        currency: "USDollar",
        language: "English",
    },
    Country {
        name: "Italy",
        capital: "Rome",
        population: 59_000_000,
        area: 301_340.0,
        currency: "Euro",
        language: "Italian",
    },
    Country {
        name: "Egypt",
        capital: "Cairo",
        population: 104_000_000,
        area: 1_002_450.0,
        currency: "EgyptianPound",
        language: "Arabic",
    },
];

fn str_value(v: &Value) -> Option<&str> {
    match v {
        Value::Str(s) | Value::Symbol(s) => Some(s),
        _ => None,
    }
}

fn is_entity_type(s: &str) -> bool {
    matches!(
        s,
        "Element" | "ChemicalElement" | "Country" | "CountryEntity"
    )
}

pub fn builtin_entity(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "Entity requires exactly 2 arguments".to_string(),
        ));
    }
    let (Some(a), Some(b)) = (str_value(&args[0]), str_value(&args[1])) else {
        return Ok(unevaluated("Entity", args));
    };
    // Accept both Entity["type", "name"] (Wolfram order) and Entity["name", "type"].
    let (ty, name) = if is_entity_type(a) {
        (a, b)
    } else if is_entity_type(b) {
        (b, a)
    } else {
        return Ok(unevaluated("Entity", args));
    };
    let ty = match ty {
        "ChemicalElement" => "Element",
        "CountryEntity" => "Country",
        other => other,
    };
    Ok(unevaluated(
        "Entity",
        &[Value::Str(ty.to_string()), Value::Str(name.to_string())],
    ))
}

fn entity_parts(v: &Value) -> Option<(&str, &str)> {
    if let Value::Call { head, args } = v
        && head == "Entity"
        && args.len() == 2
    {
        return Some((str_value(&args[0])?, str_value(&args[1])?));
    }
    None
}

fn element_property(el: &Element, prop: &str) -> Option<Value> {
    match prop {
        "AtomicNumber" => Some(int(el.atomic_number)),
        "AtomicMass" | "AtomicWeight" => Some(rf(el.atomic_mass)),
        "Symbol" | "Abbreviation" => Some(Value::Str(el.symbol.to_string())),
        "Name" => Some(Value::Str(el.name.to_string())),
        "Group" => Some(int(el.group)),
        "Period" => Some(int(el.period)),
        _ => None,
    }
}

fn country_property(c: &Country, prop: &str) -> Option<Value> {
    match prop {
        "Capital" => Some(Value::Str(c.capital.to_string())),
        "Population" => Some(int(c.population)),
        "Area" => Some(rf(c.area)),
        "Currency" => Some(Value::Str(c.currency.to_string())),
        "Language" | "Languages" => Some(Value::Str(c.language.to_string())),
        "Name" => Some(Value::Str(c.name.to_string())),
        _ => None,
    }
}

fn entity_property(ty: &str, name: &str, prop: &str) -> Option<Value> {
    match ty {
        "Element" => ELEMENTS
            .iter()
            .find(|e| e.name.eq_ignore_ascii_case(name))
            .and_then(|e| element_property(e, prop)),
        "Country" => COUNTRIES
            .iter()
            .find(|c| c.name.eq_ignore_ascii_case(name))
            .and_then(|c| country_property(c, prop)),
        _ => None,
    }
}

pub fn builtin_entity_value(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "EntityValue requires 1 or 2 arguments".to_string(),
        ));
    }
    let Some((ty, name)) = entity_parts(&args[0]) else {
        return Ok(unevaluated("EntityValue", args));
    };
    if args.len() == 1 {
        let props: Vec<String> = match ty {
            "Element" => vec![
                "AtomicNumber".to_string(),
                "AtomicMass".to_string(),
                "Symbol".to_string(),
                "Group".to_string(),
                "Period".to_string(),
            ],
            "Country" => vec![
                "Capital".to_string(),
                "Population".to_string(),
                "Area".to_string(),
                "Currency".to_string(),
                "Language".to_string(),
            ],
            _ => vec![],
        };
        let mut fields = std::collections::HashMap::new();
        for p in props {
            if let Some(v) = entity_property(ty, name, &p) {
                fields.insert(p.to_string(), v);
            }
        }
        return Ok(Value::Assoc(fields));
    }
    if let Value::List(props) = &args[1] {
        let mut out = Vec::with_capacity(props.len());
        for p in props {
            let Some(p) = str_value(p) else {
                return Ok(unevaluated("EntityValue", args));
            };
            match entity_property(ty, name, p) {
                Some(v) => out.push(v),
                None => return Ok(unevaluated("EntityValue", args)),
            }
        }
        return Ok(Value::List(out));
    }
    let Some(prop) = str_value(&args[1]) else {
        return Ok(unevaluated("EntityValue", args));
    };
    match entity_property(ty, name, prop) {
        Some(v) => Ok(v),
        None => Ok(unevaluated("EntityValue", args)),
    }
}

pub fn builtin_entity_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "EntityQ requires exactly 1 argument".to_string(),
        ));
    }
    Ok(Value::Bool(entity_parts(&args[0]).is_some()))
}

// ── Registration ─────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(_env: &Env) {
    use super::register_builtin;
    register_builtin(_env, "Quantity", builtin_quantity);
    register_builtin(_env, "QuantityQ", builtin_quantity_q);
    register_builtin(_env, "QuantityMagnitude", builtin_quantity_magnitude);
    register_builtin(_env, "QuantityUnit", builtin_quantity_unit);
    register_builtin(_env, "UnitConvert", builtin_unit_convert);
    register_builtin(_env, "UnitSimplify", builtin_unit_simplify);
    register_builtin(_env, "CompatibleUnitQ", builtin_compatible_unit_q);
    register_builtin(_env, "UnitDimensions", builtin_unit_dimensions);
    register_builtin(_env, "KnownUnitQ", builtin_known_unit_q);
    register_builtin(_env, "Entity", builtin_entity);
    register_builtin(_env, "EntityValue", builtin_entity_value);
    register_builtin(_env, "EntityQ", builtin_entity_q);

    // Quantity-aware arithmetic (delegates to `arithmetic` for plain values).
    register_builtin(_env, "Plus", q_plus);
    register_builtin(_env, "Times", q_times);
    register_builtin(_env, "Divide", q_divide);
    register_builtin(_env, "Minus", q_minus);
    register_builtin(_env, "Power", q_power);
    register_builtin(_env, "Abs", q_abs);
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn s(name: &str) -> Value {
        Value::Str(name.to_string())
    }

    fn q(mag: i64, unit: &str) -> Value {
        builtin_quantity(&[int(mag), s(unit)]).unwrap()
    }

    fn mag_of(v: &Value) -> Value {
        builtin_quantity_magnitude(std::slice::from_ref(v)).unwrap()
    }

    fn unit_of(v: &Value) -> Value {
        builtin_quantity_unit(std::slice::from_ref(v)).unwrap()
    }

    #[test]
    fn test_quantity_basic_and_plural_normalization() {
        let v = q(5, "Meters");
        assert_eq!(
            v,
            Value::Call {
                head: "Quantity".to_string(),
                args: vec![int(5), s("Meter")],
            }
        );
        // Singular form is preserved.
        assert_eq!(q(5, "Meter"), v);
    }

    #[test]
    fn test_quantity_composite_and_prefixed_units() {
        let v = q(3, "Centimeters");
        assert_eq!(unit_of(&v), s("CentiMeter"));

        let composite = builtin_quantity(&[int(1), s("Meter")]).unwrap();
        assert!(is_quantity(&composite));
        // `"Meter"/"Second"` parses to a Divide unit.
        let composed = parse_unit(&Value::Call {
            head: "Divide".to_string(),
            args: vec![s("Meter"), s("Second")],
        })
        .unwrap();
        assert_eq!(composed.factors.get("Meter"), Some(&1));
        assert_eq!(composed.factors.get("Second"), Some(&-1));
    }

    #[test]
    fn test_quantity_magnitude_and_unit() {
        let v = q(7, "Kilograms");
        assert_eq!(mag_of(&v), int(7));
        assert_eq!(unit_of(&v), s("Kilogram"));
    }

    #[test]
    fn test_quantity_missing_returns_unevaluated() {
        let out = builtin_quantity_magnitude(&[int(3)]).unwrap();
        assert_eq!(
            out,
            Value::Call {
                head: "QuantityMagnitude".to_string(),
                args: vec![int(3)]
            }
        );
    }

    #[test]
    fn test_unit_convert_mile_to_meter() {
        let qv = q(1, "Mile");
        let out = builtin_unit_convert(&[qv, s("Meter")]).unwrap();
        let m = value_as_f64(&mag_of(&out)).unwrap();
        assert!((m - 1609.344).abs() < 1e-6);
        assert_eq!(unit_of(&out), s("Meter"));
    }

    #[test]
    fn test_unit_convert_hour_to_second() {
        let out = builtin_unit_convert(&[q(2, "Hours"), s("Second")]).unwrap();
        assert_eq!(mag_of(&out), int(7200));
        assert_eq!(unit_of(&out), s("Second"));
    }

    #[test]
    fn test_unit_convert_incompatible_unevaluated() {
        let qv = q(1, "Meter");
        let out = builtin_unit_convert(&[qv.clone(), s("Second")]).unwrap();
        assert_eq!(
            out,
            Value::Call {
                head: "UnitConvert".to_string(),
                args: vec![qv, s("Second")]
            }
        );
    }

    #[test]
    fn test_unit_convert_si_base() {
        let out = builtin_unit_convert(&[q(1, "Kilometer")]).unwrap();
        assert_eq!(mag_of(&out), int(1000));
        assert_eq!(unit_of(&out), s("Meter"));
    }

    #[test]
    fn test_compatible_unit_q() {
        assert_eq!(
            builtin_compatible_unit_q(&[q(1, "Meter"), q(1, "Mile")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_compatible_unit_q(&[q(1, "Meter"), q(1, "Second")]).unwrap(),
            Value::Bool(false)
        );
        assert_eq!(
            builtin_compatible_unit_q(&[s("Meter"), s("Feet")]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_unit_dimensions() {
        let out = builtin_unit_dimensions(&[q(1, "Meter")]).unwrap();
        assert_eq!(
            out,
            Value::List(vec![Value::List(vec![s("LengthUnit"), int(1)])])
        );
        let compound = builtin_unit_dimensions(&[Value::Call {
            head: "Divide".to_string(),
            args: vec![s("Meter"), s("Second")],
        }])
        .unwrap();
        assert_eq!(
            compound,
            Value::List(vec![
                Value::List(vec![s("LengthUnit"), int(1)]),
                Value::List(vec![s("TimeUnit"), int(-1)]),
            ])
        );
    }

    #[test]
    fn test_add_same_and_compatible_units() {
        let sum = q_plus(&[q(2, "Meter"), q(3, "Meter")]).unwrap();
        assert_eq!(sum, q(5, "Meter"));

        // Compatible but distinct units convert to the first unit.
        let mixed = q_plus(&[q(1, "Meter"), q(100, "CentiMeter")]).unwrap();
        let m = value_as_f64(&mag_of(&mixed)).unwrap();
        assert!((m - 2.0).abs() < 1e-9);
        assert_eq!(unit_of(&mixed), s("Meter"));
    }

    #[test]
    fn test_add_quantity_and_scalar_stays_unevaluated() {
        let out = q_plus(&[q(2, "Meter"), int(3)]).unwrap();
        assert_eq!(
            out,
            Value::Call {
                head: "Plus".to_string(),
                args: vec![q(2, "Meter"), int(3)]
            }
        );
    }

    #[test]
    fn test_times_combines_units() {
        let out = q_times(&[q(2, "Meter"), q(3, "Second")]).unwrap();
        assert_eq!(mag_of(&out), int(6));
        let (dims, _) = dims_factor(&parse_unit(&unit_of(&out)).unwrap());
        assert_eq!(
            dims,
            vec![("LengthUnit".to_string(), 1), ("TimeUnit".to_string(), 1)]
        );

        let scaled = q_times(&[q(2, "Meter"), int(3)]).unwrap();
        assert_eq!(scaled, q(6, "Meter"));
    }

    #[test]
    fn test_divide_cancels_to_scalar() {
        let out = q_divide(&[q(6, "Meter"), q(2, "Meter")]).unwrap();
        assert_eq!(out, int(3));

        let ratio = q_divide(&[q(2, "Meter"), q(1, "Second")]).unwrap();
        assert_eq!(mag_of(&ratio), int(2));
    }

    #[test]
    fn test_power_minus_abs_values() {
        let squared = q_power(&[q(3, "Meter"), int(2)]).unwrap();
        assert_eq!(mag_of(&squared), int(9));
        assert_eq!(
            unit_of(&squared),
            Value::Call {
                head: "Power".to_string(),
                args: vec![s("Meter"), int(2)]
            }
        );

        let neg = q_minus(&[q(4, "Second")]).unwrap();
        assert_eq!(neg, q(-4, "Second"));

        let a = q_abs(&[q(-4, "Second")]).unwrap();
        assert_eq!(a, q(4, "Second"));
    }

    #[test]
    fn test_unit_simplify_derived_unit() {
        // Joule / Second → Watt
        let unit = Value::Call {
            head: "Divide".to_string(),
            args: vec![s("Joule"), s("Second")],
        };
        let qty = builtin_quantity(&[int(1), unit]).unwrap();
        let out = builtin_unit_simplify(&[qty]).unwrap();
        assert_eq!(unit_of(&out), s("Watt"));
    }

    #[test]
    fn test_unknown_quantity_returns_unevaluated() {
        // A non-unit value falls back to the unevaluated call.
        let out = builtin_quantity(&[int(5), Value::List(vec![])]).unwrap();
        assert_eq!(
            out,
            Value::Call {
                head: "Quantity".to_string(),
                args: vec![int(5), Value::List(vec![])]
            }
        );
    }

    #[test]
    fn test_quantity_q_and_known_unit_q() {
        assert_eq!(
            builtin_quantity_q(&[q(1, "Meter")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(builtin_quantity_q(&[int(1)]).unwrap(), Value::Bool(false));
        assert_eq!(
            builtin_known_unit_q(&[s("Meters")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_known_unit_q(&[s("FurlongPerFortnight")]).unwrap(),
            Value::Bool(false)
        );
    }

    #[test]
    fn test_entity_element() {
        let e = builtin_entity(&[s("Element"), s("Gold")]).unwrap();
        assert_eq!(
            builtin_entity_value(&[e.clone(), s("AtomicNumber")]).unwrap(),
            int(79)
        );
        assert_eq!(builtin_entity_value(&[e, s("Symbol")]).unwrap(), s("Au"));
    }

    #[test]
    fn test_entity_accepts_reversed_order_and_country() {
        let e = builtin_entity(&[s("France"), s("Country")]).unwrap();
        assert_eq!(
            e,
            Value::Call {
                head: "Entity".to_string(),
                args: vec![s("Country"), s("France")]
            }
        );
        assert_eq!(
            builtin_entity_value(&[e, s("Capital")]).unwrap(),
            s("Paris")
        );
    }

    #[test]
    fn test_entity_value_unknown_unevaluated() {
        let e = builtin_entity(&[s("Element"), s("Gold")]).unwrap();
        let out = builtin_entity_value(&[e.clone(), s("BoilingPoint")]).unwrap();
        assert_eq!(
            out,
            Value::Call {
                head: "EntityValue".to_string(),
                args: vec![e, s("BoilingPoint")]
            }
        );
    }
}
