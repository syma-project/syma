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
use crate::ast::Expr;
use crate::env::{Env, SubValue};
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
        name: "Celsius",
        dims: &[("TemperatureUnit", 1)],
        factor: 1.0,
    },
    UnitDef {
        name: "Fahrenheit",
        dims: &[("TemperatureUnit", 1)],
        factor: 5.0 / 9.0,
    },
    UnitDef {
        name: "Rankine",
        dims: &[("TemperatureUnit", 1)],
        factor: 5.0 / 9.0,
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
    ("centigrade", "Celsius"),
    ("celsius", "Celsius"),
    ("fahrenheit", "Fahrenheit"),
    ("degreescelsius", "Celsius"),
    ("degreesfahrenheit", "Fahrenheit"),
    ("rankine", "Rankine"),
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

/// Parts of a `QuantityDistribution[dist, unit]` expression.
fn quantity_distribution_parts(v: &Value) -> Option<(&Value, &Value)> {
    if let Value::Call { head, args } = v
        && head == "QuantityDistribution"
        && args.len() == 2
    {
        return Some((&args[0], &args[1]));
    }
    None
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

// ── Affine temperature scales ────────────────────────────────────────────────

/// `(scale_to_kelvin, offset_to_kelvin)` so that `kelvin = magnitude * scale + offset`.
/// Returns `None` for non-temperature unit names.
fn temperature_affine(name: &str) -> Option<(f64, f64)> {
    match name {
        "Kelvin" => Some((1.0, 0.0)),
        "Celsius" => Some((1.0, 273.15)),
        "Fahrenheit" => Some((5.0 / 9.0, 459.67 * 5.0 / 9.0)),
        "Rankine" => Some((5.0 / 9.0, 0.0)),
        _ => None,
    }
}

/// The bare temperature-unit name when `ue` is exactly one such unit.
fn single_temperature(ue: &UnitExpr) -> Option<&str> {
    if ue.factors.len() != 1 {
        return None;
    }
    let (name, exp) = ue.factors.iter().next()?;
    if *exp == 1 && temperature_affine(name).is_some() {
        Some(name)
    } else {
        None
    }
}

/// True when a plain multiplicative conversion between `src` and `dst` would be
/// wrong because one of them is an affine (offset) temperature scale.
fn needs_affine_temp(src: &UnitExpr, dst: &UnitExpr) -> bool {
    let affine = |ue: &UnitExpr| {
        single_temperature(ue).is_some_and(|n| temperature_affine(n).is_some_and(|(_, o)| o != 0.0))
    };
    affine(src) || affine(dst)
}

/// Convert a magnitude between two temperature scales, honouring offsets.
fn convert_temperature(mag: &Value, src: &UnitExpr, dst: &UnitExpr) -> Option<Value> {
    let src_name = single_temperature(src)?;
    let dst_name = single_temperature(dst)?;
    let value = value_as_f64(mag)?;
    let (src_scale, src_offset) = temperature_affine(src_name)?;
    let (dst_scale, dst_offset) = temperature_affine(dst_name)?;
    let kelvin = value * src_scale + src_offset;
    Some(rf((kelvin - dst_offset) / dst_scale))
}

/// Convert `mag` from `src` to `dst`, applying affine temperature handling.
fn convert_units(mag: &Value, src: &UnitExpr, dst: &UnitExpr) -> Option<Value> {
    if src == dst {
        return Some(mag.clone());
    }
    if needs_affine_temp(src, dst) {
        return convert_temperature(mag, src, dst);
    }
    to_unit(mag, src, dst)
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
        1 => {
            if let Some((mag, _)) = quantity_parts(&args[0]) {
                return Ok(mag.clone());
            }
            if let Some((dist, _)) = quantity_distribution_parts(&args[0]) {
                return Ok(dist.clone());
            }
            Ok(unevaluated("QuantityMagnitude", args))
        }
        2 => {
            let Some((mag, unit)) = quantity_parts(&args[0]) else {
                return Ok(unevaluated("QuantityMagnitude", args));
            };
            let (Some(src), Some(dst)) = (parse_unit(unit), parse_unit(&args[1])) else {
                return Ok(unevaluated("QuantityMagnitude", args));
            };
            match convert_units(mag, &src, &dst) {
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
        1 => {
            if let Some((_, unit)) = quantity_parts(&args[0]) {
                return Ok(unit.clone());
            }
            if let Some((_, unit)) = quantity_distribution_parts(&args[0]) {
                return Ok(unit.clone());
            }
            Ok(unevaluated("QuantityUnit", args))
        }
        2 => {
            let Some((mag, unit)) = quantity_parts(&args[0]) else {
                return Ok(unevaluated("QuantityUnit", args));
            };
            let (Some(src), Some(dst)) = (parse_unit(unit), parse_unit(&args[1])) else {
                return Ok(unevaluated("QuantityUnit", args));
            };
            match convert_units(mag, &src, &dst) {
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
            // Affine temperature units convert to Kelvin base units.
            if let Some(name) = single_temperature(&src)
                && name != "Kelvin"
            {
                let kelvin = UnitExpr::single("Kelvin".to_string());
                let Some(converted) = convert_temperature(mag, &src, &kelvin) else {
                    return Ok(unevaluated("UnitConvert", args));
                };
                return Ok(make_quantity(converted, &kelvin));
            }
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
            match convert_units(mag, &src, &dst) {
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
    block: &'static str,
    density: f64,
    melting_point: f64,
    boiling_point: f64,
    electronegativity: f64,
    state: &'static str,
}

const ELEMENTS: &[Element] = &[
    Element {
        name: "Hydrogen",
        atomic_number: 1,
        atomic_mass: 1.008,
        symbol: "H",
        group: 1,
        period: 1,
        block: "s",
        density: 0.000_089_88,
        melting_point: 13.99,
        boiling_point: 20.271,
        electronegativity: 2.20,
        state: "Gas",
    },
    Element {
        name: "Helium",
        atomic_number: 2,
        atomic_mass: 4.0026,
        symbol: "He",
        group: 18,
        period: 1,
        block: "s",
        density: 0.000_178_5,
        melting_point: 0.95,
        boiling_point: 4.222,
        electronegativity: 0.0,
        state: "Gas",
    },
    Element {
        name: "Lithium",
        atomic_number: 3,
        atomic_mass: 6.94,
        symbol: "Li",
        group: 1,
        period: 2,
        block: "s",
        density: 0.534,
        melting_point: 453.65,
        boiling_point: 1603.0,
        electronegativity: 0.98,
        state: "Solid",
    },
    Element {
        name: "Beryllium",
        atomic_number: 4,
        atomic_mass: 9.0122,
        symbol: "Be",
        group: 2,
        period: 2,
        block: "s",
        density: 1.85,
        melting_point: 1560.0,
        boiling_point: 2742.0,
        electronegativity: 1.57,
        state: "Solid",
    },
    Element {
        name: "Boron",
        atomic_number: 5,
        atomic_mass: 10.81,
        symbol: "B",
        group: 13,
        period: 2,
        block: "p",
        density: 2.34,
        melting_point: 2349.0,
        boiling_point: 4200.0,
        electronegativity: 2.04,
        state: "Solid",
    },
    Element {
        name: "Carbon",
        atomic_number: 6,
        atomic_mass: 12.011,
        symbol: "C",
        group: 14,
        period: 2,
        block: "p",
        density: 2.267,
        melting_point: 3823.0,
        boiling_point: 4098.0,
        electronegativity: 2.55,
        state: "Solid",
    },
    Element {
        name: "Nitrogen",
        atomic_number: 7,
        atomic_mass: 14.007,
        symbol: "N",
        group: 15,
        period: 2,
        block: "p",
        density: 0.001_250_6,
        melting_point: 63.15,
        boiling_point: 77.355,
        electronegativity: 3.04,
        state: "Gas",
    },
    Element {
        name: "Oxygen",
        atomic_number: 8,
        atomic_mass: 15.999,
        symbol: "O",
        group: 16,
        period: 2,
        block: "p",
        density: 0.001_429,
        melting_point: 54.36,
        boiling_point: 90.188,
        electronegativity: 3.44,
        state: "Gas",
    },
    Element {
        name: "Fluorine",
        atomic_number: 9,
        atomic_mass: 18.998,
        symbol: "F",
        group: 17,
        period: 2,
        block: "p",
        density: 0.001_696,
        melting_point: 53.48,
        boiling_point: 85.03,
        electronegativity: 3.98,
        state: "Gas",
    },
    Element {
        name: "Neon",
        atomic_number: 10,
        atomic_mass: 20.180,
        symbol: "Ne",
        group: 18,
        period: 2,
        block: "p",
        density: 0.000_899_9,
        melting_point: 24.56,
        boiling_point: 27.104,
        electronegativity: 0.0,
        state: "Gas",
    },
    Element {
        name: "Sodium",
        atomic_number: 11,
        atomic_mass: 22.99,
        symbol: "Na",
        group: 1,
        period: 3,
        block: "s",
        density: 0.968,
        melting_point: 370.944,
        boiling_point: 1156.09,
        electronegativity: 0.93,
        state: "Solid",
    },
    Element {
        name: "Magnesium",
        atomic_number: 12,
        atomic_mass: 24.305,
        symbol: "Mg",
        group: 2,
        period: 3,
        block: "s",
        density: 1.738,
        melting_point: 923.0,
        boiling_point: 1363.0,
        electronegativity: 1.31,
        state: "Solid",
    },
    Element {
        name: "Aluminium",
        atomic_number: 13,
        atomic_mass: 26.982,
        symbol: "Al",
        group: 13,
        period: 3,
        block: "p",
        density: 2.70,
        melting_point: 933.47,
        boiling_point: 2743.0,
        electronegativity: 1.61,
        state: "Solid",
    },
    Element {
        name: "Silicon",
        atomic_number: 14,
        atomic_mass: 28.085,
        symbol: "Si",
        group: 14,
        period: 3,
        block: "p",
        density: 2.329,
        melting_point: 1687.0,
        boiling_point: 3538.0,
        electronegativity: 1.90,
        state: "Solid",
    },
    Element {
        name: "Phosphorus",
        atomic_number: 15,
        atomic_mass: 30.974,
        symbol: "P",
        group: 15,
        period: 3,
        block: "p",
        density: 1.823,
        melting_point: 317.3,
        boiling_point: 553.7,
        electronegativity: 2.19,
        state: "Solid",
    },
    Element {
        name: "Sulfur",
        atomic_number: 16,
        atomic_mass: 32.06,
        symbol: "S",
        group: 16,
        period: 3,
        block: "p",
        density: 2.07,
        melting_point: 388.36,
        boiling_point: 717.8,
        electronegativity: 2.58,
        state: "Solid",
    },
    Element {
        name: "Chlorine",
        atomic_number: 17,
        atomic_mass: 35.45,
        symbol: "Cl",
        group: 17,
        period: 3,
        block: "p",
        density: 0.003_214,
        melting_point: 171.6,
        boiling_point: 239.11,
        electronegativity: 3.16,
        state: "Gas",
    },
    Element {
        name: "Argon",
        atomic_number: 18,
        atomic_mass: 39.948,
        symbol: "Ar",
        group: 18,
        period: 3,
        block: "p",
        density: 0.001_784,
        melting_point: 83.81,
        boiling_point: 87.302,
        electronegativity: 0.0,
        state: "Gas",
    },
    Element {
        name: "Potassium",
        atomic_number: 19,
        atomic_mass: 39.098,
        symbol: "K",
        group: 1,
        period: 4,
        block: "s",
        density: 0.862,
        melting_point: 336.7,
        boiling_point: 1032.0,
        electronegativity: 0.82,
        state: "Solid",
    },
    Element {
        name: "Calcium",
        atomic_number: 20,
        atomic_mass: 40.078,
        symbol: "Ca",
        group: 2,
        period: 4,
        block: "s",
        density: 1.55,
        melting_point: 1115.0,
        boiling_point: 1757.0,
        electronegativity: 1.00,
        state: "Solid",
    },
    Element {
        name: "Titanium",
        atomic_number: 22,
        atomic_mass: 47.867,
        symbol: "Ti",
        group: 4,
        period: 4,
        block: "d",
        density: 4.506,
        melting_point: 1941.0,
        boiling_point: 3560.0,
        electronegativity: 1.54,
        state: "Solid",
    },
    Element {
        name: "Chromium",
        atomic_number: 24,
        atomic_mass: 51.996,
        symbol: "Cr",
        group: 6,
        period: 4,
        block: "d",
        density: 7.15,
        melting_point: 2180.0,
        boiling_point: 2944.0,
        electronegativity: 1.66,
        state: "Solid",
    },
    Element {
        name: "Manganese",
        atomic_number: 25,
        atomic_mass: 54.938,
        symbol: "Mn",
        group: 7,
        period: 4,
        block: "d",
        density: 7.21,
        melting_point: 1519.0,
        boiling_point: 2334.0,
        electronegativity: 1.55,
        state: "Solid",
    },
    Element {
        name: "Iron",
        atomic_number: 26,
        atomic_mass: 55.845,
        symbol: "Fe",
        group: 8,
        period: 4,
        block: "d",
        density: 7.874,
        melting_point: 1811.0,
        boiling_point: 3134.0,
        electronegativity: 1.83,
        state: "Solid",
    },
    Element {
        name: "Nickel",
        atomic_number: 28,
        atomic_mass: 58.693,
        symbol: "Ni",
        group: 10,
        period: 4,
        block: "d",
        density: 8.908,
        melting_point: 1728.0,
        boiling_point: 3186.0,
        electronegativity: 1.91,
        state: "Solid",
    },
    Element {
        name: "Copper",
        atomic_number: 29,
        atomic_mass: 63.546,
        symbol: "Cu",
        group: 11,
        period: 4,
        block: "d",
        density: 8.96,
        melting_point: 1357.77,
        boiling_point: 2835.0,
        electronegativity: 1.90,
        state: "Solid",
    },
    Element {
        name: "Zinc",
        atomic_number: 30,
        atomic_mass: 65.38,
        symbol: "Zn",
        group: 12,
        period: 4,
        block: "d",
        density: 7.14,
        melting_point: 692.68,
        boiling_point: 1180.0,
        electronegativity: 1.65,
        state: "Solid",
    },
    Element {
        name: "Bromine",
        atomic_number: 35,
        atomic_mass: 79.904,
        symbol: "Br",
        group: 17,
        period: 4,
        block: "p",
        density: 3.1028,
        melting_point: 265.8,
        boiling_point: 332.0,
        electronegativity: 2.96,
        state: "Liquid",
    },
    Element {
        name: "Silver",
        atomic_number: 47,
        atomic_mass: 107.868,
        symbol: "Ag",
        group: 11,
        period: 5,
        block: "d",
        density: 10.49,
        melting_point: 1234.93,
        boiling_point: 2435.0,
        electronegativity: 1.93,
        state: "Solid",
    },
    Element {
        name: "Tungsten",
        atomic_number: 74,
        atomic_mass: 183.84,
        symbol: "W",
        group: 6,
        period: 6,
        block: "d",
        density: 19.25,
        melting_point: 3695.0,
        boiling_point: 6203.0,
        electronegativity: 2.36,
        state: "Solid",
    },
    Element {
        name: "Platinum",
        atomic_number: 78,
        atomic_mass: 195.08,
        symbol: "Pt",
        group: 10,
        period: 6,
        block: "d",
        density: 21.45,
        melting_point: 2041.4,
        boiling_point: 4098.0,
        electronegativity: 2.28,
        state: "Solid",
    },
    Element {
        name: "Gold",
        atomic_number: 79,
        atomic_mass: 196.967,
        symbol: "Au",
        group: 11,
        period: 6,
        block: "d",
        density: 19.3,
        melting_point: 1337.33,
        boiling_point: 3243.0,
        electronegativity: 2.54,
        state: "Solid",
    },
    Element {
        name: "Mercury",
        atomic_number: 80,
        atomic_mass: 200.592,
        symbol: "Hg",
        group: 12,
        period: 6,
        block: "d",
        density: 13.534,
        melting_point: 234.321,
        boiling_point: 629.88,
        electronegativity: 2.00,
        state: "Liquid",
    },
    Element {
        name: "Lead",
        atomic_number: 82,
        atomic_mass: 207.2,
        symbol: "Pb",
        group: 14,
        period: 6,
        block: "p",
        density: 11.34,
        melting_point: 600.61,
        boiling_point: 2022.0,
        electronegativity: 2.33,
        state: "Solid",
    },
    Element {
        name: "Radon",
        atomic_number: 86,
        atomic_mass: 222.0,
        symbol: "Rn",
        group: 18,
        period: 6,
        block: "p",
        density: 0.009_73,
        melting_point: 202.0,
        boiling_point: 211.5,
        electronegativity: 0.0,
        state: "Gas",
    },
    Element {
        name: "Uranium",
        atomic_number: 92,
        atomic_mass: 238.029,
        symbol: "U",
        group: 3,
        period: 7,
        block: "f",
        density: 19.1,
        melting_point: 1405.3,
        boiling_point: 4404.0,
        electronegativity: 1.38,
        state: "Solid",
    },
];

struct Country {
    name: &'static str,
    capital: &'static str,
    population: i64,
    area: f64,
    currency: &'static str,
    language: &'static str,
    continent: &'static str,
    calling_code: i64,
    internet_code: &'static str,
    government_type: &'static str,
}

const COUNTRIES: &[Country] = &[
    Country {
        name: "France",
        capital: "Paris",
        population: 68_000_000,
        area: 551_695.0,
        currency: "Euro",
        language: "French",
        continent: "Europe",
        calling_code: 33,
        internet_code: "FR",
        government_type: "Republic",
    },
    Country {
        name: "Germany",
        capital: "Berlin",
        population: 83_000_000,
        area: 357_022.0,
        currency: "Euro",
        language: "German",
        continent: "Europe",
        calling_code: 49,
        internet_code: "DE",
        government_type: "Republic",
    },
    Country {
        name: "Japan",
        capital: "Tokyo",
        population: 125_000_000,
        area: 377_975.0,
        currency: "Yen",
        language: "Japanese",
        continent: "Asia",
        calling_code: 81,
        internet_code: "JP",
        government_type: "ConstitutionalMonarchy",
    },
    Country {
        name: "Brazil",
        capital: "Brasilia",
        population: 214_000_000,
        area: 8_515_767.0,
        currency: "Real",
        language: "Portuguese",
        continent: "SouthAmerica",
        calling_code: 55,
        internet_code: "BR",
        government_type: "Republic",
    },
    Country {
        name: "Canada",
        capital: "Ottawa",
        population: 38_000_000,
        area: 9_984_670.0,
        currency: "CanadianDollar",
        language: "English",
        continent: "NorthAmerica",
        calling_code: 1,
        internet_code: "CA",
        government_type: "ConstitutionalMonarchy",
    },
    Country {
        name: "Australia",
        capital: "Canberra",
        population: 26_000_000,
        area: 7_692_024.0,
        currency: "AustralianDollar",
        language: "English",
        continent: "Oceania",
        calling_code: 61,
        internet_code: "AU",
        government_type: "ConstitutionalMonarchy",
    },
    Country {
        name: "India",
        capital: "NewDelhi",
        population: 1_400_000_000,
        area: 3_287_263.0,
        currency: "Rupee",
        language: "Hindi",
        continent: "Asia",
        calling_code: 91,
        internet_code: "IN",
        government_type: "Republic",
    },
    Country {
        name: "UnitedStates",
        capital: "WashingtonDC",
        population: 332_000_000,
        area: 9_833_517.0,
        currency: "USDollar",
        language: "English",
        continent: "NorthAmerica",
        calling_code: 1,
        internet_code: "US",
        government_type: "Republic",
    },
    Country {
        name: "Italy",
        capital: "Rome",
        population: 59_000_000,
        area: 301_340.0,
        currency: "Euro",
        language: "Italian",
        continent: "Europe",
        calling_code: 39,
        internet_code: "IT",
        government_type: "Republic",
    },
    Country {
        name: "Egypt",
        capital: "Cairo",
        population: 104_000_000,
        area: 1_002_450.0,
        currency: "EgyptianPound",
        language: "Arabic",
        continent: "Africa",
        calling_code: 20,
        internet_code: "EG",
        government_type: "Republic",
    },
    Country {
        name: "China",
        capital: "Beijing",
        population: 1_412_000_000,
        area: 9_596_961.0,
        currency: "Yuan",
        language: "Mandarin",
        continent: "Asia",
        calling_code: 86,
        internet_code: "CN",
        government_type: "Republic",
    },
    Country {
        name: "UnitedKingdom",
        capital: "London",
        population: 67_000_000,
        area: 242_495.0,
        currency: "PoundSterling",
        language: "English",
        continent: "Europe",
        calling_code: 44,
        internet_code: "GB",
        government_type: "ConstitutionalMonarchy",
    },
    Country {
        name: "Spain",
        capital: "Madrid",
        population: 47_000_000,
        area: 505_990.0,
        currency: "Euro",
        language: "Spanish",
        continent: "Europe",
        calling_code: 34,
        internet_code: "ES",
        government_type: "ConstitutionalMonarchy",
    },
    Country {
        name: "Russia",
        capital: "Moscow",
        population: 144_000_000,
        area: 17_098_242.0,
        currency: "Ruble",
        language: "Russian",
        continent: "Europe",
        calling_code: 7,
        internet_code: "RU",
        government_type: "Republic",
    },
    Country {
        name: "Mexico",
        capital: "MexicoCity",
        population: 129_000_000,
        area: 1_964_375.0,
        currency: "Peso",
        language: "Spanish",
        continent: "NorthAmerica",
        calling_code: 52,
        internet_code: "MX",
        government_type: "Republic",
    },
    Country {
        name: "SouthKorea",
        capital: "Seoul",
        population: 52_000_000,
        area: 100_210.0,
        currency: "Won",
        language: "Korean",
        continent: "Asia",
        calling_code: 82,
        internet_code: "KR",
        government_type: "Republic",
    },
    Country {
        name: "Nigeria",
        capital: "Abuja",
        population: 218_000_000,
        area: 923_768.0,
        currency: "Naira",
        language: "English",
        continent: "Africa",
        calling_code: 234,
        internet_code: "NG",
        government_type: "Republic",
    },
    Country {
        name: "SouthAfrica",
        capital: "Pretoria",
        population: 60_000_000,
        area: 1_221_037.0,
        currency: "Rand",
        language: "English",
        continent: "Africa",
        calling_code: 27,
        internet_code: "ZA",
        government_type: "Republic",
    },
    Country {
        name: "Argentina",
        capital: "BuenosAires",
        population: 46_000_000,
        area: 2_780_400.0,
        currency: "Peso",
        language: "Spanish",
        continent: "SouthAmerica",
        calling_code: 54,
        internet_code: "AR",
        government_type: "Republic",
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

/// Canonical entity type for a (possibly aliased) type name.
fn canonical_entity_type(ty: &str) -> Option<&'static str> {
    match ty {
        "Element" | "ChemicalElement" => Some("Element"),
        "Country" | "CountryEntity" => Some("Country"),
        _ => None,
    }
}

/// Known property names for an entity type.
fn properties_for(ty: &str) -> &'static [&'static str] {
    match ty {
        "Element" => &[
            "AtomicNumber",
            "AtomicMass",
            "AtomicWeight",
            "Symbol",
            "Abbreviation",
            "Name",
            "Group",
            "Period",
            "Block",
            "Density",
            "MeltingPoint",
            "MeltingTemperature",
            "BoilingPoint",
            "BoilingTemperature",
            "Electronegativity",
            "State",
            "Phase",
        ],
        "Country" => &[
            "Name",
            "Capital",
            "Population",
            "PopulationDensity",
            "Area",
            "Currency",
            "Language",
            "Languages",
            "Continent",
            "CallingCode",
            "InternetCode",
            "InternetCountryCode",
            "GovernmentType",
        ],
        _ => &[],
    }
}

/// Named entity classes and their member entity names.
const ENTITY_CLASSES: &[(&str, &str, &[&str])] = &[
    ("Element", "NobleGas", &["Helium", "Neon", "Argon", "Radon"]),
    (
        "Element",
        "AlkaliMetal",
        &["Lithium", "Sodium", "Potassium"],
    ),
    ("Element", "Halogen", &["Fluorine", "Chlorine", "Bromine"]),
    (
        "Element",
        "TransitionMetal",
        &[
            "Titanium",
            "Chromium",
            "Manganese",
            "Iron",
            "Nickel",
            "Copper",
            "Zinc",
            "Silver",
            "Tungsten",
            "Platinum",
            "Gold",
            "Mercury",
        ],
    ),
    (
        "Country",
        "EuropeanUnion",
        &["France", "Germany", "Italy", "Spain"],
    ),
    (
        "Country",
        "G7",
        &[
            "France",
            "Germany",
            "Italy",
            "Japan",
            "Canada",
            "UnitedStates",
            "UnitedKingdom",
        ],
    ),
];

fn entity_class_members(ty: &str, class: &str) -> Option<&'static [&'static str]> {
    ENTITY_CLASSES
        .iter()
        .find(|(t, c, _)| *t == ty && c.eq_ignore_ascii_case(class))
        .map(|(_, _, members)| *members)
}

fn element_property(el: &Element, prop: &str) -> Option<Value> {
    match prop {
        "AtomicNumber" => Some(int(el.atomic_number)),
        "AtomicMass" | "AtomicWeight" => Some(rf(el.atomic_mass)),
        "Symbol" | "Abbreviation" => Some(Value::Str(el.symbol.to_string())),
        "Name" => Some(Value::Str(el.name.to_string())),
        "Group" => Some(int(el.group)),
        "Period" => Some(int(el.period)),
        "Block" => Some(Value::Str(el.block.to_string())),
        "Density" => Some(rf(el.density)),
        "MeltingPoint" | "MeltingTemperature" => Some(rf(el.melting_point)),
        "BoilingPoint" | "BoilingTemperature" => Some(rf(el.boiling_point)),
        "Electronegativity" => Some(rf(el.electronegativity)),
        "State" | "Phase" => Some(Value::Str(el.state.to_string())),
        _ => None,
    }
}

fn country_property(c: &Country, prop: &str) -> Option<Value> {
    match prop {
        "Capital" => Some(Value::Str(c.capital.to_string())),
        "Population" => Some(int(c.population)),
        "PopulationDensity" => Some(rf(c.population as f64 / c.area)),
        "Area" => Some(rf(c.area)),
        "Currency" => Some(Value::Str(c.currency.to_string())),
        "Language" | "Languages" => Some(Value::Str(c.language.to_string())),
        "Name" => Some(Value::Str(c.name.to_string())),
        "Continent" => Some(Value::Str(c.continent.to_string())),
        "CallingCode" => Some(int(c.calling_code)),
        "InternetCode" | "InternetCountryCode" => Some(Value::Str(c.internet_code.to_string())),
        "GovernmentType" => Some(Value::Str(c.government_type.to_string())),
        _ => None,
    }
}

fn entity_property(ty: &str, name: &str, prop: &str) -> Option<Value> {
    match canonical_entity_type(ty)? {
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

fn entity_class_parts(v: &Value) -> Option<(&'static str, &str)> {
    if let Value::Call { head, args } = v
        && head == "EntityClass"
        && args.len() == 2
    {
        let ty = canonical_entity_type(str_value(&args[0])?)?;
        return Some((ty, str_value(&args[1])?));
    }
    None
}

fn entity_list_for(ty: &str, names: &[&str]) -> Value {
    Value::List(
        names
            .iter()
            .map(|n| Value::Call {
                head: "Entity".to_string(),
                args: vec![Value::Str(ty.to_string()), Value::Str((*n).to_string())],
            })
            .collect(),
    )
}

pub fn builtin_entity_property(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 && args.len() != 3 {
        return Err(EvalError::Error(
            "EntityProperty requires 2 or 3 arguments".to_string(),
        ));
    }
    let (Some(a), Some(b)) = (str_value(&args[0]), str_value(&args[1])) else {
        return Ok(unevaluated("EntityProperty", args));
    };
    let (ty, prop) = if canonical_entity_type(a).is_some() {
        (a, b)
    } else if canonical_entity_type(b).is_some() {
        (b, a)
    } else {
        return Ok(unevaluated("EntityProperty", args));
    };
    let Some(ty) = canonical_entity_type(ty) else {
        return Ok(unevaluated("EntityProperty", args));
    };
    if !properties_for(ty)
        .iter()
        .any(|p| p.eq_ignore_ascii_case(prop))
    {
        return Ok(unevaluated("EntityProperty", args));
    }
    let mut out = vec![Value::Str(ty.to_string()), Value::Str(prop.to_string())];
    if args.len() == 3 {
        out.push(args[2].clone());
    }
    Ok(Value::Call {
        head: "EntityProperty".to_string(),
        args: out,
    })
}

pub fn builtin_entity_list(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() > 1 {
        return Err(EvalError::Error(
            "EntityList requires 0 or 1 arguments".to_string(),
        ));
    }
    if args.is_empty() {
        return Ok(Value::List(vec![
            Value::Str("Element".to_string()),
            Value::Str("Country".to_string()),
        ]));
    }
    if let Some((ty, class)) = entity_class_parts(&args[0]) {
        return match entity_class_members(ty, class) {
            Some(members) => Ok(entity_list_for(ty, members)),
            None => Ok(unevaluated("EntityList", args)),
        };
    }
    let Some(ty) = str_value(&args[0]).and_then(canonical_entity_type) else {
        return Ok(unevaluated("EntityList", args));
    };
    let members: Vec<&'static str> = match ty {
        "Element" => ELEMENTS.iter().map(|e| e.name).collect(),
        "Country" => COUNTRIES.iter().map(|c| c.name).collect(),
        _ => Vec::new(),
    };
    Ok(entity_list_for(ty, &members))
}

pub fn builtin_entity_class(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() || args.len() > 2 {
        return Err(EvalError::Error(
            "EntityClass requires 1 or 2 arguments".to_string(),
        ));
    }
    if args.len() == 1 {
        let Some(class) = str_value(&args[0]) else {
            return Ok(unevaluated("EntityClass", args));
        };
        let found = ENTITY_CLASSES
            .iter()
            .find(|(_, c, _)| c.eq_ignore_ascii_case(class));
        return match found {
            Some((ty, _, _)) => Ok(Value::Call {
                head: "EntityClass".to_string(),
                args: vec![Value::Str((*ty).to_string()), Value::Str(class.to_string())],
            }),
            None => Ok(unevaluated("EntityClass", args)),
        };
    }
    let (Some(a), Some(b)) = (str_value(&args[0]), str_value(&args[1])) else {
        return Ok(unevaluated("EntityClass", args));
    };
    let (ty, class) = if canonical_entity_type(a).is_some() {
        (a, b)
    } else if canonical_entity_type(b).is_some() {
        (b, a)
    } else {
        return Ok(unevaluated("EntityClass", args));
    };
    let Some(ty) = canonical_entity_type(ty) else {
        return Ok(unevaluated("EntityClass", args));
    };
    if entity_class_members(ty, class).is_none() {
        return Ok(unevaluated("EntityClass", args));
    }
    Ok(Value::Call {
        head: "EntityClass".to_string(),
        args: vec![Value::Str(ty.to_string()), Value::Str(class.to_string())],
    })
}

// ── QuantityDistribution ─────────────────────────────────────────────────────

pub fn builtin_quantity_distribution(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => Ok(Value::Call {
            head: "QuantityDistribution".to_string(),
            args: vec![args[0].clone(), int(1)],
        }),
        2 => {
            let Some(ue) = parse_unit(&args[1]) else {
                return Ok(unevaluated("QuantityDistribution", args));
            };
            Ok(Value::Call {
                head: "QuantityDistribution".to_string(),
                args: vec![args[0].clone(), ue.to_value()],
            })
        }
        _ => Err(EvalError::Error(
            "QuantityDistribution requires 1 or 2 arguments".to_string(),
        )),
    }
}

// ── Interpreter (basic offline coercion) ─────────────────────────────────────

const INTERPRETER_TYPES: &[&str] = &["Integer", "Real", "Number", "String", "Boolean"];

/// Resolve an interpreter spec to a canonical type name or unit name.
fn interpreter_spec(spec: &str) -> Option<String> {
    if let Some(t) = INTERPRETER_TYPES
        .iter()
        .find(|t| t.eq_ignore_ascii_case(spec))
    {
        return Some((*t).to_string());
    }
    resolve_name(spec).map(|r| r.name)
}

fn parse_number(s: &str) -> Option<Value> {
    let t = s.trim();
    if let Ok(n) = t.parse::<i64>() {
        return Some(int(n));
    }
    t.parse::<f64>().ok().map(rf)
}

fn interpret_unit(unit_name: &str, input: &Value) -> Option<Value> {
    let ue = UnitExpr::single(unit_name.to_string());
    if let Some((mag, u)) = quantity_parts(input) {
        let src = parse_unit(u)?;
        return convert_units(mag, &src, &ue).map(|c| make_quantity(c, &ue));
    }
    if value_as_f64(input).is_some() {
        return Some(make_quantity(input.clone(), &ue));
    }
    if let Value::Str(text) = input {
        return parse_number(text).map(|n| make_quantity(n, &ue));
    }
    None
}

fn interpret_input(spec: &str, input: &Value) -> Option<Value> {
    match spec {
        "Integer" => match input {
            Value::Integer(_) => Some(input.clone()),
            Value::Real(r) if r.is_integer() => r.to_integer().map(Value::Integer),
            Value::Str(text) => text
                .trim()
                .parse::<i64>()
                .ok()
                .map(|n| Value::Integer(Integer::from(n))),
            _ => None,
        },
        "Real" => match input {
            Value::Real(_) => Some(input.clone()),
            Value::Integer(n) => Some(rf(n.to_f64())),
            Value::Rational(r) => Some(rf(r.as_ref().to_f64())),
            Value::Str(text) => text.trim().parse::<f64>().ok().map(rf),
            _ => None,
        },
        "Number" => match input {
            Value::Integer(_) | Value::Real(_) | Value::Rational(_) => Some(input.clone()),
            Value::Str(text) => parse_number(text),
            _ => None,
        },
        "String" => match input {
            Value::Str(_) => Some(input.clone()),
            Value::Integer(n) => Some(Value::Str(n.to_string())),
            Value::Real(r) => Some(Value::Str(r.to_string())),
            Value::Bool(b) => Some(Value::Str(b.to_string())),
            _ => None,
        },
        "Boolean" => match input {
            Value::Bool(_) => Some(input.clone()),
            Value::Str(text) => match text.trim().to_ascii_lowercase().as_str() {
                "true" => Some(Value::Bool(true)),
                "false" => Some(Value::Bool(false)),
                _ => None,
            },
            _ => None,
        },
        unit => interpret_unit(unit, input),
    }
}

pub fn builtin_interpreter(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        1 => {
            let Some(spec) = str_value(&args[0]).and_then(interpreter_spec) else {
                return Ok(unevaluated("Interpreter", args));
            };
            Ok(Value::Call {
                head: "Interpreter".to_string(),
                args: vec![Value::Str(spec)],
            })
        }
        2 => {
            let Some(spec) = str_value(&args[0]).and_then(interpreter_spec) else {
                return Ok(unevaluated("Interpreter", args));
            };
            match interpret_input(&spec, &args[1]) {
                Some(v) => Ok(v),
                None => Ok(unevaluated("Interpreter", args)),
            }
        }
        _ => Err(EvalError::Error(
            "Interpreter requires 1 or 2 arguments".to_string(),
        )),
    }
}

fn blank(name: &str) -> Expr {
    Expr::NamedBlank {
        name: name.to_string(),
        type_constraint: None,
    }
}

fn apply_call(head: &str, names: &[&str]) -> Expr {
    Expr::Call {
        head: Box::new(Expr::Symbol(head.to_string())),
        args: names
            .iter()
            .map(|n| Expr::Symbol((*n).to_string()))
            .collect(),
    }
}

// ── Registration ─────────────────────────────────────────────────────────────

/// Register the builtins implemented in this module.
pub fn register(env: &Env) {
    use super::register_builtin;
    register_builtin(env, "Quantity", builtin_quantity);
    register_builtin(env, "QuantityQ", builtin_quantity_q);
    register_builtin(env, "QuantityMagnitude", builtin_quantity_magnitude);
    register_builtin(env, "QuantityUnit", builtin_quantity_unit);
    register_builtin(env, "QuantityDistribution", builtin_quantity_distribution);
    register_builtin(env, "UnitConvert", builtin_unit_convert);
    register_builtin(env, "UnitSimplify", builtin_unit_simplify);
    register_builtin(env, "CompatibleUnitQ", builtin_compatible_unit_q);
    register_builtin(env, "UnitDimensions", builtin_unit_dimensions);
    register_builtin(env, "KnownUnitQ", builtin_known_unit_q);
    register_builtin(env, "Entity", builtin_entity);
    register_builtin(env, "EntityValue", builtin_entity_value);
    register_builtin(env, "EntityQ", builtin_entity_q);
    register_builtin(env, "EntityProperty", builtin_entity_property);
    register_builtin(env, "EntityList", builtin_entity_list);
    register_builtin(env, "EntityClass", builtin_entity_class);
    register_builtin(env, "Interpreter", builtin_interpreter);

    // `Interpreter["type"][input]` routes through a curried subvalue to the
    // two-argument interpreter form.
    env.add_subvalue(
        "Interpreter",
        SubValue {
            outer: vec![blank("spec")],
            inner: vec![blank("x")],
            rhs: apply_call("Interpreter", &["spec", "x"]),
            delayed: true,
            guard: None,
        },
    );

    // Quantity-aware arithmetic (delegates to `arithmetic` for plain values).
    register_builtin(env, "Plus", q_plus);
    register_builtin(env, "Times", q_times);
    register_builtin(env, "Divide", q_divide);
    register_builtin(env, "Minus", q_minus);
    register_builtin(env, "Power", q_power);
    register_builtin(env, "Abs", q_abs);
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
        let out = builtin_entity_value(&[e.clone(), s("CrystalStructure")]).unwrap();
        assert_eq!(
            out,
            Value::Call {
                head: "EntityValue".to_string(),
                args: vec![e, s("CrystalStructure")]
            }
        );
    }

    fn approx(v: &Value, expected: f64) -> bool {
        value_as_f64(v).is_some_and(|x| (x - expected).abs() < 1e-6)
    }

    #[test]
    fn test_expanded_element_table_and_properties() {
        // Newly added element resolves.
        let e = builtin_entity(&[s("Element"), s("Neon")]).unwrap();
        assert_eq!(
            builtin_entity_value(&[e.clone(), s("AtomicNumber")]).unwrap(),
            int(10)
        );
        assert_eq!(
            builtin_entity_value(&[e.clone(), s("Symbol")]).unwrap(),
            s("Ne")
        );
        assert_eq!(
            builtin_entity_value(&[e.clone(), s("Block")]).unwrap(),
            s("p")
        );
        assert_eq!(builtin_entity_value(&[e, s("State")]).unwrap(), s("Gas"));

        // Element count grew well beyond the original dozen.
        assert!(ELEMENTS.len() >= 30);
        assert!(COUNTRIES.len() >= 15);
    }

    #[test]
    fn test_entity_property_builtin() {
        let p = builtin_entity_property(&[s("Element"), s("BoilingPoint")]).unwrap();
        assert_eq!(
            p,
            Value::Call {
                head: "EntityProperty".to_string(),
                args: vec![s("Element"), s("BoilingPoint")]
            }
        );
        // Reversed order is normalised.
        let p2 = builtin_entity_property(&[s("Population"), s("Country")]).unwrap();
        assert_eq!(
            p2,
            Value::Call {
                head: "EntityProperty".to_string(),
                args: vec![s("Country"), s("Population")]
            }
        );
        // Unknown property stays unevaluated.
        let bad = builtin_entity_property(&[s("Element"), s("Colour")]).unwrap();
        assert_eq!(
            bad,
            Value::Call {
                head: "EntityProperty".to_string(),
                args: vec![s("Element"), s("Colour")]
            }
        );
    }

    #[test]
    fn test_entity_list() {
        let out = builtin_entity_list(&[s("Element")]).unwrap();
        match out {
            Value::List(items) => {
                assert_eq!(items.len(), ELEMENTS.len());
                assert_eq!(
                    items[0],
                    Value::Call {
                        head: "Entity".to_string(),
                        args: vec![s("Element"), s("Hydrogen")]
                    }
                );
            }
            other => panic!("expected a list, got {other:?}"),
        }
        let types = builtin_entity_list(&[]).unwrap();
        assert_eq!(types, Value::List(vec![s("Element"), s("Country")]));
    }

    #[test]
    fn test_entity_class_basic() {
        let c = builtin_entity_class(&[s("Element"), s("NobleGas")]).unwrap();
        assert_eq!(
            c,
            Value::Call {
                head: "EntityClass".to_string(),
                args: vec![s("Element"), s("NobleGas")]
            }
        );
        // Unknown class stays unevaluated.
        let bad = builtin_entity_class(&[s("Element"), s("Metalloid")]).unwrap();
        assert_eq!(
            bad,
            Value::Call {
                head: "EntityClass".to_string(),
                args: vec![s("Element"), s("Metalloid")]
            }
        );
        // EntityList expands a class to its member entities.
        let members = builtin_entity_list(&[c]).unwrap();
        match members {
            Value::List(items) => {
                assert_eq!(items.len(), 4);
                assert!(items.iter().all(|i| entity_parts(i).is_some()));
            }
            other => panic!("expected a list, got {other:?}"),
        }
    }

    #[test]
    fn test_temperature_convert_celsius_fahrenheit() {
        // 100 °C → 373.15 K
        let out = builtin_unit_convert(&[q(100, "Celsius"), s("Kelvin")]).unwrap();
        assert!(approx(&mag_of(&out), 373.15));
        assert_eq!(unit_of(&out), s("Kelvin"));

        // 32 °F → 0 °C
        let out = builtin_unit_convert(&[q(32, "Fahrenheit"), s("Celsius")]).unwrap();
        assert!(approx(&mag_of(&out), 0.0));
        assert_eq!(unit_of(&out), s("Celsius"));

        // 0 °C → 32 °F
        let out = builtin_unit_convert(&[q(0, "Celsius"), s("Fahrenheit")]).unwrap();
        assert!(approx(&mag_of(&out), 32.0));

        // One-argument form converts to Kelvin base units.
        let base = builtin_unit_convert(&[q(0, "Celsius")]).unwrap();
        assert!(approx(&mag_of(&base), 273.15));
        assert_eq!(unit_of(&base), s("Kelvin"));
    }

    #[test]
    fn test_temperature_aliases_and_dimensions() {
        assert_eq!(unit_of(&q(1, "DegreesCelsius")), s("Celsius"));
        assert_eq!(unit_of(&q(1, "Fahrenheit")), s("Fahrenheit"));
        let dims = builtin_unit_dimensions(&[s("Celsius")]).unwrap();
        assert_eq!(
            dims,
            Value::List(vec![Value::List(vec![s("TemperatureUnit"), int(1)])])
        );
        assert_eq!(
            builtin_compatible_unit_q(&[s("Celsius"), s("Fahrenheit")]).unwrap(),
            Value::Bool(true)
        );
    }

    #[test]
    fn test_quantity_distribution_basic() {
        let dist = Value::Call {
            head: "NormalDistribution".to_string(),
            args: vec![int(0), int(1)],
        };
        let qd = builtin_quantity_distribution(&[dist.clone(), s("Meter")]).unwrap();
        assert_eq!(
            qd,
            Value::Call {
                head: "QuantityDistribution".to_string(),
                args: vec![dist.clone(), s("Meter")]
            }
        );
        assert_eq!(mag_of(&qd), dist);
        assert_eq!(unit_of(&qd), s("Meter"));

        // Dimensionless form defaults to unit 1.
        let plain = builtin_quantity_distribution(&[dist]).unwrap();
        assert_eq!(
            plain,
            Value::Call {
                head: "QuantityDistribution".to_string(),
                args: vec![
                    Value::Call {
                        head: "NormalDistribution".to_string(),
                        args: vec![int(0), int(1)]
                    },
                    int(1)
                ]
            }
        );
    }

    #[test]
    fn test_interpreter_basic_types() {
        assert_eq!(
            builtin_interpreter(&[s("Integer"), int(42)]).unwrap(),
            int(42)
        );
        assert_eq!(
            builtin_interpreter(&[s("Integer"), s("123")]).unwrap(),
            int(123)
        );
        assert!(approx(
            &builtin_interpreter(&[s("Real"), int(3)]).unwrap(),
            3.0
        ));
        assert_eq!(
            builtin_interpreter(&[s("String"), int(42)]).unwrap(),
            s("42")
        );
        assert_eq!(
            builtin_interpreter(&[s("Boolean"), s("True")]).unwrap(),
            Value::Bool(true)
        );
        assert_eq!(
            builtin_interpreter(&[s("Number"), s("2.5")]).unwrap(),
            rf(2.5)
        );
        // Failed interpretation stays unevaluated.
        let bad = builtin_interpreter(&[s("Integer"), s("abc")]).unwrap();
        assert_eq!(
            bad,
            Value::Call {
                head: "Interpreter".to_string(),
                args: vec![s("Integer"), s("abc")]
            }
        );
    }

    #[test]
    fn test_interpreter_spec_and_unit_coercion() {
        // One-argument form yields the interpreter object.
        let obj = builtin_interpreter(&[s("Integer")]).unwrap();
        assert_eq!(
            obj,
            Value::Call {
                head: "Interpreter".to_string(),
                args: vec![s("Integer")]
            }
        );

        // Unit spec coerces numbers into quantities.
        let qty = builtin_interpreter(&[s("Meter"), int(5)]).unwrap();
        assert_eq!(qty, q(5, "Meter"));

        // A compatible quantity is converted to the requested unit.
        let converted = builtin_interpreter(&[s("Meter"), q(1, "Kilometer")]).unwrap();
        assert_eq!(mag_of(&converted), int(1000));
        assert_eq!(unit_of(&converted), s("Meter"));

        // Unknown spec stays unevaluated.
        let bad = builtin_interpreter(&[s("Furlong"), int(1)]).unwrap();
        assert_eq!(
            bad,
            Value::Call {
                head: "Interpreter".to_string(),
                args: vec![s("Furlong"), int(1)]
            }
        );
    }
}
