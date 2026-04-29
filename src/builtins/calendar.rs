use crate::value::{DEFAULT_PRECISION, EvalError, Value};
use rug::Float;
use rug::Integer;
use std::collections::HashMap;

// ── Helpers ──

fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
}

fn days_in_month(year: i64, month: i64) -> i64 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if is_leap_year(year) { 29 } else { 28 },
        _ => 0,
    }
}

/// Convert {year, month, day, hour, minute, second} to Unix timestamp (seconds since 1970-01-01).
fn date_to_unix_seconds(year: i64, month: i64, day: i64, hour: i64, minute: i64, second: f64) -> f64 {
    let mut total_days: i64 = 0;

    // Count days from year 1970 to `year`
    if year >= 1970 {
        for y in 1970..year {
            total_days += 365 + if is_leap_year(y) { 1 } else { 0 };
        }
    } else {
        for y in year..1970 {
            total_days -= 365 + if is_leap_year(y) { 1 } else { 0 };
        }
    }

    // Count days from January to `month` in the given year
    for m in 1..month {
        total_days += days_in_month(year, m);
    }

    // Add days of the current month
    total_days += day - 1;

    total_days as f64 * 86400.0 + hour as f64 * 3600.0 + minute as f64 * 60.0 + second
}

/// For negative timestamps, the above approach is fragile. Let's rewrite with a cleaner strategy.
/// This version works bidirectionally by counting days from epoch.
fn unix_seconds_to_datetime_v2(seconds: f64) -> (i64, i64, i64, i64, i64, f64) {
    let has_fraction = seconds.fract().abs() > 1e-10;
    let total_secs = if seconds >= 0.0 {
        seconds.floor() as i64
    } else {
        // For negative: -3.5 → floor = -4.0, we want -3 whole seconds + -0.5 fraction
        // Actually let's be careful: frac already has a different sign for negative.
        // e.g. -3.7: seconds.floor() = -4.0, seconds.fract() = -0.3 → -4 + -0.3 = -4.3 (wrong)
        // Better: for negative, use ceil on the integer part
        seconds.ceil() as i64
    };
    // Recompute fraction relative to the chosen integer boundary
    let remaining_frac = seconds - total_secs as f64;

    let sign = if total_secs < 0 { -1i64 } else { 1i64 };
    let abs_total = total_secs.abs();

    let total_days = abs_total / 86400;
    let day_remainder = abs_total % 86400;

    let hour = day_remainder / 3600;
    let minute = (day_remainder % 3600) / 60;
    let sec = day_remainder % 60;

    // Now compute year/month/day from total_days since epoch (can be negative)
    let (year, month, day) = days_since_epoch_to_ymd(total_days * sign, sign);

    // Build a clean second value: integer seconds + fraction
    let sec_val = sec as f64 + if has_fraction { remaining_frac } else { 0.0 };

    (
        year,
        month,
        day,
        hour * sign,
        minute * sign,
        sec_val * sign as f64,
    )
}

/// Convert a day count relative to Unix epoch (positive = after, negative = before) 
/// to (year, month, day).
fn days_since_epoch_to_ymd(day_offset: i64, _sign: i64) -> (i64, i64, i64) {
    if day_offset >= 0 {
        // Forward from 1970-01-01 (day 0 = 1970-01-01)
        let mut y = 1970i64;
        let mut remaining = day_offset;
        loop {
            let diy = 365i64 + if is_leap_year(y) { 1 } else { 0 };
            if remaining < diy {
                break;
            }
            remaining -= diy;
            y += 1;
        }
        let mut m = 1i64;
        while m <= 12 {
            let dim = days_in_month(y, m);
            if remaining < dim {
                break;
            }
            remaining -= dim;
            m += 1;
        }
        let d = remaining + 1;
        (y, m, d)
    } else {
        // Backward from 1970-01-01
        // day_offset is negative. Let's count backward.
        let abs_offset = day_offset.abs();
        // abs_offset = 1 means 1969-12-31, abs_offset = 0 means 1970-01-01
        if abs_offset == 0 {
            return (1970, 1, 1);
        }
        // We're `abs_offset` days before 1970-01-01.
        // Start at 1969 and go backward.
        let mut y = 1969i64;
        let mut remaining = abs_offset;
        loop {
            let diy = 365i64 + if is_leap_year(y) { 1 } else { 0 };
            if remaining <= diy {
                break;
            }
            remaining -= diy;
            y -= 1;
        }
        // `remaining` days from end of year `y`
        let diy = 365i64 + if is_leap_year(y) { 1 } else { 0 };
        let day_of_year = diy - remaining + 1;
        // Convert day_of_year (1-based) in year y to y/m/d
        let mut m = 1i64;
        let mut doy = 1i64;
        while m <= 12 {
            let dim = days_in_month(y, m);
            if doy + dim > day_of_year {
                break;
            }
            doy += dim;
            m += 1;
        }
        let d = day_of_year - doy + 1;
        (y, m, d)
    }
}

/// Day of week: returns 0=Monday, 1=Tuesday, ..., 6=Sunday.
/// Uses Tomohiko Sakamoto's algorithm (which gives 0=Sunday, so we adjust).
fn day_of_week(year: i64, month: i64, day: i64) -> u32 {
    // Sakamoto's algorithm: 0=Sunday, 1=Monday, ..., 6=Saturday
    let t: [i32; 12] = [0, 3, 2, 5, 0, 3, 5, 1, 4, 6, 2, 4];
    let y = if month < 3 { year - 1 } else { year };
    let dow_sunday = (y + y / 4 - y / 100 + y / 400 + t[(month - 1) as usize] as i64 + day) % 7;
    let dow_sunday = dow_sunday as u32;
    // Convert: Sunday=0 → we want Sunday=6
    if dow_sunday == 0 { 6 } else { dow_sunday - 1 }
}

/// Current datetime components {year, month, day, hour, minute, second} as f64 timestamp then decomposed.
fn get_current_datetime() -> (i64, i64, i64, i64, i64, f64) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs_f64();
    unix_seconds_to_datetime_v2(now)
}

/// Extract {year, month, day, hour?, minute?, second?} from a Value::List.
/// Returns (y, m, d, h, mi, s, remaining_len) where h, mi, s default to 0.
fn extract_datetime(args: &[Value]) -> Result<(i64, i64, i64, i64, i64, f64), EvalError> {
    if args.len() < 3 || args.len() > 6 {
        return Err(EvalError::Error(
            "Date specification must have 3 to 6 elements: {year, month, day, hour?, minute?, second?}".to_string(),
        ));
    }
    let to_i64 = |i: usize, what: &str| -> Result<i64, EvalError> {
        match &args[i] {
            Value::Integer(n) => i64::try_from(n).map_err(|_| EvalError::Error(format!("{} out of range", what))),
            Value::Real(r) => Ok(r.to_f64() as i64),
            _ => Err(EvalError::TypeError {
                expected: "Integer".to_string(),
                got: args[i].type_name().to_string(),
            }),
        }
    };
    let to_f64 = |i: usize, _what: &str| -> Result<f64, EvalError> {
        match &args[i] {
            Value::Integer(n) => Ok(n.to_f64()),
            Value::Real(r) => Ok(r.to_f64()),
            _ => Err(EvalError::TypeError {
                expected: "Number".to_string(),
                got: args[i].type_name().to_string(),
            }),
        }
    };
    let y = to_i64(0, "Year")?;
    let m = to_i64(1, "Month")?;
    let d = to_i64(2, "Day")?;
    let h = if args.len() > 3 { to_i64(3, "Hour")? } else { 0i64 };
    let mi = if args.len() > 4 { to_i64(4, "Minute")? } else { 0i64 };
    let s = if args.len() > 5 { to_f64(5, "Second")? } else { 0.0f64 };
    Ok((y, m, d, h, mi, s))
}

/// Parse an ISO 8601 date string like "2024-01-15T10:30:00" or "2024-01-15".
fn parse_iso_date(s: &str) -> Result<(i64, i64, i64, i64, i64, f64), EvalError> {
    // Try full ISO 8601: "YYYY-MM-DDTHH:MM:SS" or "YYYY-MM-DD HH:MM:SS"
    let core = if s.contains('T') {
        s.split('T').collect::<Vec<&str>>()[0]
    } else {
        s.split_whitespace().collect::<Vec<&str>>().first().copied().unwrap_or(s)
    };

    let parts: Vec<&str> = core.split('-').collect();
    if parts.len() < 3 {
        return Err(EvalError::Error(format!("Unable to parse date string: {}", s)));
    }

    let year: i64 = parts[0].parse().map_err(|_| EvalError::Error("Invalid year".to_string()))?;
    let month: i64 = parts[1].parse().map_err(|_| EvalError::Error("Invalid month".to_string()))?;
    let day_part = parts[2];
    // Day might contain time like "15T10:30:00" if no T in date
    let day_str = day_part.split('T').next().unwrap_or(day_part);
    let day: i64 = day_str.parse().map_err(|_| EvalError::Error("Invalid day".to_string()))?;

    // Parse time part if present
    let time_str = if s.contains('T') {
        s.split('T').nth(1)
    } else if s.contains(' ') {
        s.split_whitespace().nth(1)
    } else {
        None
    };

    let (hour, minute, second) = if let Some(t) = time_str {
        let time_parts: Vec<&str> = t.split(':').collect();
        let h: i64 = time_parts.first().and_then(|s| s.parse().ok()).unwrap_or(0);
        let m: i64 = time_parts.get(1).and_then(|s| s.parse().ok()).unwrap_or(0);
        let s: f64 = time_parts.get(2).and_then(|s| {
            // Strip fractional seconds and timezone info
            let num_str = s.split(['.', '+', '-']).next().unwrap_or(s);
            num_str.parse().ok()
        }).unwrap_or(0.0);
        (h, m, s)
    } else {
        (0, 0, 0.0)
    };

    Ok((year, month, day, hour, minute, second))
}

fn val_int(n: i64) -> Value {
    Value::Integer(Integer::from(n))
}

fn val_real(f: f64) -> Value {
    Value::Real(Float::with_val(DEFAULT_PRECISION, f))
}

/// Parse a DurationSpec value: {n, "Unit"} → (n, unit_string).
fn parse_duration_spec(val: &Value) -> Result<(f64, String), EvalError> {
    match val {
        Value::List(elems) if elems.len() == 2 => {
            let n = match &elems[0] {
                Value::Integer(i) => i.to_f64(),
                Value::Real(r) => r.to_f64(),
                _ => return Err(EvalError::TypeError {
                    expected: "Number".to_string(),
                    got: elems[0].type_name().to_string(),
                }),
            };
            let unit = match &elems[1] {
                Value::Str(s) => s.clone(),
                Value::Symbol(s) => s.clone(),
                _ => return Err(EvalError::TypeError {
                    expected: "String".to_string(),
                    got: elems[1].type_name().to_string(),
                }),
            };
            Ok((n, unit))
        }
        _ => Err(EvalError::Error(
            "Duration specification must be {number, \"Unit\"}".to_string(),
        )),
    }
}

/// Convert a unit string to seconds multiplier.
fn unit_to_seconds(unit: &str) -> Result<f64, EvalError> {
    match unit {
        "Seconds" | "Second" => Ok(1.0),
        "Minutes" | "Minute" => Ok(60.0),
        "Hours" | "Hour" => Ok(3600.0),
        "Days" | "Day" => Ok(86400.0),
        "Weeks" | "Week" => Ok(604800.0),
        "Months" | "Month" => Ok(2592000.0),  // 30 days
        "Years" | "Year" => Ok(31536000.0),   // 365 days
        _ => Err(EvalError::Error(format!("Unknown time unit: {}", unit))),
    }
}

/// Format a datetime into components matching the input length.
fn format_datetime_components(secs: f64, len: usize) -> Value {
    let (y, m, d, h, mi, s) = unix_seconds_to_datetime_v2(secs);
    match len {
        3 => Value::List(vec![val_int(y), val_int(m), val_int(d)]),
        6 => Value::List(vec![val_int(y), val_int(m), val_int(d), val_int(h), val_int(mi), val_real(s)]),
        _ => Value::List(vec![val_int(y), val_int(m), val_int(d), val_int(h), val_int(mi), val_real(s)]),
    }
}

const DAY_NAMES: [&str; 7] = [
    "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday",
];

const MONTH_NAMES: [&str; 12] = [
    "January", "February", "March", "April", "May", "June",
    "July", "August", "September", "October", "November", "December",
];

// ── Builtin implementations ──

/// `AbsoluteTime[]` or `AbsoluteTime[date]`
/// Returns Unix timestamp (seconds since 1970-01-01).
pub fn builtin_absolute_time(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        0 => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs_f64();
            Ok(val_real(now))
        }
        1 => {
            let (y, m, d, h, mi, s) = match &args[0] {
                Value::List(elems) => extract_datetime(elems)?,
                Value::Str(s) => parse_iso_date(s)?,
                _ => return Err(EvalError::TypeError {
                    expected: "List or String".to_string(),
                    got: args[0].type_name().to_string(),
                }),
            };
            let secs = date_to_unix_seconds(y, m, d, h, mi, s);
            Ok(val_real(secs))
        }
        _ => Err(EvalError::Error(
            "AbsoluteTime takes 0 or 1 argument".to_string(),
        )),
    }
}

/// `Now` — current date/time as {year, month, day, hour, minute, second}.
pub fn builtin_now(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return Err(EvalError::Error("Now takes no arguments".to_string()));
    }
    let (y, m, d, h, mi, s) = get_current_datetime();
    Ok(Value::List(vec![
        val_int(y),
        val_int(m),
        val_int(d),
        val_int(h),
        val_int(mi),
        val_real(s),
    ]))
}

/// `DateString[]` or `DateString[spec]` or `DateString[date]` or `DateString[spec, date]`
pub fn builtin_date_string(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        0 => {
            let (y, m, d, h, mi, s) = get_current_datetime();
            Ok(Value::Str(format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, h, mi, s as i64)))
        }
        1 => {
            // Could be a spec string or a date
            match &args[0] {
                Value::Str(spec) => {
                    let (y, m, d, h, mi, s) = get_current_datetime();
                    format_date_string(spec, y, m, d, h, mi, s)
                }
                Value::List(elems) => {
                    let (y, m, d, h, mi, s) = extract_datetime(elems)?;
                    Ok(Value::Str(format!("{:04}-{:02}-{:02} {:02}:{:02}:{:02}", y, m, d, h, mi, s as i64)))
                }
                _ => Err(EvalError::TypeError {
                    expected: "String or List".to_string(),
                    got: args[0].type_name().to_string(),
                }),
            }
        }
        2 => {
            // DateString[spec, date]
            let spec = match &args[0] {
                Value::Str(s) => s.clone(),
                _ => return Err(EvalError::TypeError {
                    expected: "String".to_string(),
                    got: args[0].type_name().to_string(),
                }),
            };
            let (y, m, d, h, mi, s) = match &args[1] {
                Value::List(elems) => extract_datetime(elems)?,
                Value::Str(s) => parse_iso_date(s)?,
                _ => return Err(EvalError::TypeError {
                    expected: "List or String".to_string(),
                    got: args[1].type_name().to_string(),
                }),
            };
            format_date_string(&spec, y, m, d, h, mi, s)
        }
        _ => Err(EvalError::Error(
            "DateString takes 0, 1, or 2 arguments".to_string(),
        )),
    }
}

fn format_date_string(spec: &str, y: i64, m: i64, d: i64, h: i64, mi: i64, s: f64) -> Result<Value, EvalError> {
    let result = match spec {
        "ISO8601" => format!("{:04}-{:02}-{:02}T{:02}:{:02}:{:02}", y, m, d, h, mi, s as i64),
        "ShortDate" => format!("{:02}/{:02}/{:04}", m, d, y),
        "LongDate" => format!("{}, {:02}, {:04}", MONTH_NAMES[(m - 1) as usize], d, y),
        "Year" => format!("{:04}", y),
        "Month" => format!("{:02}", m),
        "Day" => format!("{:02}", d),
        "Hour" => format!("{:02}", h),
        "Minute" => format!("{:02}", mi),
        "Second" => format!("{:02}", s as i64),
        "Time" => format!("{:02}:{:02}:{:02}", h, mi, s as i64),
        _ => return Err(EvalError::Error(format!("Unknown date format: {}", spec))),
    };
    Ok(Value::Str(result))
}

/// `DatePlus[datespec, duration]`
pub fn builtin_date_plus(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 2 {
        return Err(EvalError::Error(
            "DatePlus requires exactly 2 arguments".to_string(),
        ));
    }
    let (y, m, d, h, mi, s) = match &args[0] {
        Value::List(elems) => {
            extract_datetime(elems)?
        }
        Value::Str(s) => parse_iso_date(s)?,
        _ => return Err(EvalError::TypeError {
            expected: "List or String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    };

    let input_len = match &args[0] {
        Value::List(elems) => elems.len(),
        _ => 6,
    };

    let (amount, unit) = parse_duration_spec(&args[1])?;
    let seconds_per_unit = unit_to_seconds(&unit)?;
    let delta = amount * seconds_per_unit;

    let base_secs = date_to_unix_seconds(y, m, d, h, mi, s);
    let result_secs = base_secs + delta;
    Ok(format_datetime_components(result_secs, input_len))
}

/// `DateDifference[date1, date2]` or `DateDifference[date1, date2, unit]`
pub fn builtin_date_difference(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() < 2 || args.len() > 3 {
        return Err(EvalError::Error(
            "DateDifference requires 2 or 3 arguments".to_string(),
        ));
    }

    let dt1 = match &args[0] {
        Value::List(elems) => extract_datetime(elems)?,
        Value::Str(s) => parse_iso_date(s)?,
        _ => return Err(EvalError::TypeError {
            expected: "List or String".to_string(),
            got: args[0].type_name().to_string(),
        }),
    };
    let dt2 = match &args[1] {
        Value::List(elems) => extract_datetime(elems)?,
        Value::Str(s) => parse_iso_date(s)?,
        _ => return Err(EvalError::TypeError {
            expected: "List or String".to_string(),
            got: args[1].type_name().to_string(),
        }),
    };

    let secs1 = date_to_unix_seconds(dt1.0, dt1.1, dt1.2, dt1.3, dt1.4, dt1.5);
    let secs2 = date_to_unix_seconds(dt2.0, dt2.1, dt2.2, dt2.3, dt2.4, dt2.5);
    let diff = secs2 - secs1;

    if args.len() == 3 {
        let unit = match &args[2] {
            Value::Str(s) => s.clone(),
            Value::Symbol(s) => s.clone(),
            _ => return Err(EvalError::TypeError {
                expected: "String".to_string(),
                got: args[2].type_name().to_string(),
            }),
        };
        let divisor = unit_to_seconds(&unit)?;
        Ok(val_real(diff / divisor))
    } else {
        Ok(val_real(diff))
    }
}

/// `DateObject[date]` — structured date object
pub fn builtin_date_object(args: &[Value]) -> Result<Value, EvalError> {
    let (y, m, d, h, mi, s) = match args.len() {
        0 => get_current_datetime(),
        1 => match &args[0] {
            Value::List(elems) => extract_datetime(elems)?,
            Value::Str(s) => parse_iso_date(s)?,
            _ => return Err(EvalError::TypeError {
                expected: "List or String".to_string(),
                got: args[0].type_name().to_string(),
            }),
        },
        _ => return Err(EvalError::Error(
            "DateObject takes 0 or 1 argument".to_string(),
        )),
    };

    let mut fields = HashMap::new();
    fields.insert("year".to_string(), val_int(y));
    fields.insert("month".to_string(), val_int(m));
    fields.insert("day".to_string(), val_int(d));
    fields.insert("hour".to_string(), val_int(h));
    fields.insert("minute".to_string(), val_int(mi));
    fields.insert("second".to_string(), val_real(s));
    fields.insert("timezone".to_string(), Value::Str("UTC".to_string()));
    Ok(Value::Object {
        class_name: "DateObject".to_string(),
        fields,
    })
}

/// `DateList[]` or `DateList[date]` or `DateList[datespec]`
/// Returns {year, month, day, hour, minute, second}.
pub fn builtin_date_list(args: &[Value]) -> Result<Value, EvalError> {
    let (y, m, d, h, mi, s) = match args.len() {
        0 => get_current_datetime(),
        1 => match &args[0] {
            Value::List(elems) => extract_datetime(elems)?,
            Value::Str(s) => parse_iso_date(s)?,
            _ => return Err(EvalError::TypeError {
                expected: "List or String".to_string(),
                got: args[0].type_name().to_string(),
            }),
        },
        _ => return Err(EvalError::Error(
            "DateList takes 0 or 1 argument".to_string(),
        )),
    };
    Ok(Value::List(vec![
        val_int(y),
        val_int(m),
        val_int(d),
        val_int(h),
        val_int(mi),
        val_real(s),
    ]))
}

/// `Today` — returns {year, month, day}
pub fn builtin_today(args: &[Value]) -> Result<Value, EvalError> {
    if !args.is_empty() {
        return Err(EvalError::Error("Today takes no arguments".to_string()));
    }
    let (y, m, d, _, _, _) = get_current_datetime();
    Ok(Value::List(vec![val_int(y), val_int(m), val_int(d)]))
}

/// `DayName[date]` — name of the day of the week
pub fn builtin_day_name(args: &[Value]) -> Result<Value, EvalError> {
    let (y, m, d) = match args.len() {
        0 => {
            let (y, m, d, _, _, _) = get_current_datetime();
            (y, m, d)
        }
        1 => {
            let (y, m, d, _, _, _) = match &args[0] {
                Value::List(elems) => extract_datetime(elems)?,
                Value::Str(s) => parse_iso_date(s)?,
                _ => return Err(EvalError::TypeError {
                    expected: "List or String".to_string(),
                    got: args[0].type_name().to_string(),
                }),
            };
            (y, m, d)
        }
        _ => return Err(EvalError::Error(
            "DayName takes 0 or 1 argument".to_string(),
        )),
    };
    let dow = day_of_week(y, m, d);
    Ok(Value::Str(DAY_NAMES[dow as usize].to_string()))
}

/// `DayCount[date1, date2]` or `DayCount[]`
/// Number of days between two dates.
pub fn builtin_day_count(args: &[Value]) -> Result<Value, EvalError> {
    match args.len() {
        0 => {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap();
            Ok(val_int((now.as_secs() / 86400) as i64))
        }
        2 => {
            let dt1 = match &args[0] {
                Value::List(elems) => extract_datetime(elems)?,
                Value::Str(s) => parse_iso_date(s)?,
                _ => return Err(EvalError::TypeError {
                    expected: "List or String".to_string(),
                    got: args[0].type_name().to_string(),
                }),
            };
            let dt2 = match &args[1] {
                Value::List(elems) => extract_datetime(elems)?,
                Value::Str(s) => parse_iso_date(s)?,
                _ => return Err(EvalError::TypeError {
                    expected: "List or String".to_string(),
                    got: args[1].type_name().to_string(),
                }),
            };
            let secs1 = date_to_unix_seconds(dt1.0, dt1.1, dt1.2, dt1.3, dt1.4, dt1.5);
            let secs2 = date_to_unix_seconds(dt2.0, dt2.1, dt2.2, dt2.3, dt2.4, dt2.5);
            let diff_days = ((secs2 - secs1) / 86400.0).round() as i64;
            Ok(val_int(diff_days))
        }
        _ => Err(EvalError::Error(
            "DayCount takes 0 or 2 arguments".to_string(),
        )),
    }
}

/// `MonthName[date]` or `MonthName[month_number]`
pub fn builtin_month_name(args: &[Value]) -> Result<Value, EvalError> {
    if args.is_empty() {
        return Err(EvalError::Error(
            "MonthName requires 1 argument".to_string(),
        ));
    }
    let month_num: i64 = match &args[0] {
        Value::Integer(n) => i64::try_from(n).unwrap_or(0),
        Value::Real(r) => r.to_f64() as i64,
        Value::List(elems) => {
            if elems.is_empty() {
                return Err(EvalError::Error("MonthName requires a month".to_string()));
            }
            // Extract month from {year, month, day} or {month}
            match elems.len() {
                1 => match &elems[0] {
                    Value::Integer(n) => i64::try_from(n).unwrap_or(0),
                    Value::Real(r) => r.to_f64() as i64,
                    _ => return Err(EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: elems[0].type_name().to_string(),
                    }),
                },
                _ => {
                    if elems.len() < 2 {
                        return Err(EvalError::Error("Date list must have at least 2 elements for MonthName".to_string()));
                    }
                    match &elems[1] {
                        Value::Integer(n) => i64::try_from(n).unwrap_or(0),
                        Value::Real(r) => r.to_f64() as i64,
                        _ => return Err(EvalError::TypeError {
                            expected: "Integer".to_string(),
                            got: elems[1].type_name().to_string(),
                        }),
                    }
                }
            }
        }
        _ => return Err(EvalError::TypeError {
            expected: "Integer or List".to_string(),
            got: args[0].type_name().to_string(),
        }),
    };
    if !(1..=12).contains(&month_num) {
        return Err(EvalError::Error(
            "Month must be between 1 and 12".to_string(),
        ));
    }
    Ok(Value::Str(MONTH_NAMES[(month_num - 1) as usize].to_string()))
}

/// `DaysInMonth[{year, month}]`
pub fn builtin_days_in_month(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "DaysInMonth requires exactly 1 argument".to_string(),
        ));
    }
    let (year, month) = match &args[0] {
        Value::List(elems) if elems.len() >= 2 => {
            let to_i64 = |i: usize, what: &str| -> Result<i64, EvalError> {
                match &elems[i] {
                    Value::Integer(n) => i64::try_from(n).map_err(|_| EvalError::Error(format!("{} out of range", what))),
                    Value::Real(r) => Ok(r.to_f64() as i64),
                    _ => Err(EvalError::TypeError {
                        expected: "Integer".to_string(),
                        got: elems[i].type_name().to_string(),
                    }),
                }
            };
            (to_i64(0, "Year")?, to_i64(1, "Month")?)
        }
        _ => return Err(EvalError::TypeError {
            expected: "List {year, month}".to_string(),
            got: args[0].type_name().to_string(),
        }),
    };
    Ok(val_int(days_in_month(year, month)))
}

/// `LeapYearQ[year]`
pub fn builtin_leap_year_q(args: &[Value]) -> Result<Value, EvalError> {
    if args.len() != 1 {
        return Err(EvalError::Error(
            "LeapYearQ requires exactly 1 argument".to_string(),
        ));
    }
    let year = match &args[0] {
        Value::Integer(n) => i64::try_from(n).unwrap_or(0),
        Value::Real(r) => r.to_f64() as i64,
        _ => return Err(EvalError::TypeError {
            expected: "Integer".to_string(),
            got: args[0].type_name().to_string(),
        }),
    };
    Ok(Value::Bool(is_leap_year(year)))
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_leap_year() {
        assert!(is_leap_year(2000));  // divisible by 400
        assert!(is_leap_year(2024));  // divisible by 4, not 100
        assert!(!is_leap_year(1900)); // divisible by 100, not 400
        assert!(!is_leap_year(2023)); // not divisible by 4
    }

    #[test]
    fn test_days_in_month_helper() {
        assert_eq!(days_in_month(2024, 1), 31);
        assert_eq!(days_in_month(2024, 2), 29); // leap year
        assert_eq!(days_in_month(2023, 2), 28); // not leap
        assert_eq!(days_in_month(2024, 4), 30);
        assert_eq!(days_in_month(2024, 12), 31);
    }

    #[test]
    fn test_date_to_unix_seconds_epoch() {
        // 1970-01-01 00:00:00 should be 0
        let secs = date_to_unix_seconds(1970, 1, 1, 0, 0, 0.0);
        assert_eq!(secs, 0.0);
    }

    #[test]
    fn test_date_to_unix_seconds_known() {
        // 2000-01-01 00:00:00 UTC = 946684800
        let secs = date_to_unix_seconds(2000, 1, 1, 0, 0, 0.0);
        assert_eq!(secs, 946684800.0);
    }

    #[test]
    fn test_unix_seconds_to_datetime_roundtrip() {
        let test_dates = vec![
            (1970, 1, 1, 0, 0, 0.0),
            (2000, 1, 1, 0, 0, 0.0),
            (2024, 2, 29, 12, 30, 45.0),
            (2024, 1, 15, 10, 30, 0.5),
            (2025, 12, 31, 23, 59, 59.0),
        ];

        for (y, m, d, h, mi, s) in test_dates {
            let secs = date_to_unix_seconds(y, m, d, h, mi, s);
            let (ry, rm, rd, rh, rmi, rs) = unix_seconds_to_datetime_v2(secs);
            assert_eq!(ry, y, "year mismatch for {:?}", (y, m, d, h, mi, s));
            assert_eq!(rm, m, "month mismatch for {:?}", (y, m, d, h, mi, s));
            assert_eq!(rd, d, "day mismatch for {:?}", (y, m, d, h, mi, s));
            assert_eq!(rh, h, "hour mismatch for {:?}", (y, m, d, h, mi, s));
            assert_eq!(rmi, mi, "minute mismatch for {:?}", (y, m, d, h, mi, s));
            assert!((rs - s).abs() < 0.01, "second mismatch for {:?}: got {}, expected {}", (y, m, d, h, mi, s), rs, s);
        }
    }

    #[test]
    fn test_day_of_week_known() {
        // Jan 1, 2024 was a Monday
        assert_eq!(day_of_week(2024, 1, 1), 0); // Monday
        // Jan 1, 2000 was a Saturday
        assert_eq!(day_of_week(2000, 1, 1), 5); // Saturday
        // Jan 1, 1970 was a Thursday
        assert_eq!(day_of_week(1970, 1, 1), 3); // Thursday
    }

    #[test]
    fn test_parse_iso_date() {
        let (y, m, d, h, mi, s) = parse_iso_date("2024-01-15T10:30:00").unwrap();
        assert_eq!((y, m, d, h, mi, s), (2024, 1, 15, 10, 30, 0.0));

        let (y, m, d, h, mi, s) = parse_iso_date("2024-01-15").unwrap();
        assert_eq!((y, m, d, h, mi, s), (2024, 1, 15, 0, 0, 0.0));
    }

    #[test]
    fn test_absolute_time_no_args() {
        let result = builtin_absolute_time(&[]).unwrap();
        match result {
            Value::Real(r) => {
                let secs = r.to_f64();
                assert!(secs > 0.0, "Expected positive Unix timestamp");
            }
            _ => panic!("Expected Real value"),
        }
    }

    #[test]
    fn test_builtin_days_in_month_leap() {
        let result = builtin_days_in_month(&[Value::List(vec![val_int(2024), val_int(2)])]).unwrap();
        assert_eq!(result, val_int(29));

        let result = builtin_days_in_month(&[Value::List(vec![val_int(2023), val_int(2)])]).unwrap();
        assert_eq!(result, val_int(28));
    }

    #[test]
    fn test_builtin_leap_year_q() {
        assert_eq!(builtin_leap_year_q(&[val_int(2000)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_leap_year_q(&[val_int(1900)]).unwrap(), Value::Bool(false));
        assert_eq!(builtin_leap_year_q(&[val_int(2024)]).unwrap(), Value::Bool(true));
        assert_eq!(builtin_leap_year_q(&[val_int(2023)]).unwrap(), Value::Bool(false));
    }

    #[test]
    fn test_builtin_date_string_format() {
        let result = builtin_date_string(&[Value::Str("ISO8601".to_string())]).unwrap();
        match result {
            Value::Str(s) => assert!(s.contains('T')),
            _ => panic!("Expected String"),
        }

        let result = builtin_date_string(&[Value::Str("Year".to_string())]).unwrap();
        match result {
            Value::Str(s) => assert!(s.chars().all(|c| c.is_ascii_digit()) && s.len() == 4),
            _ => panic!("Expected String"),
        }
    }

    #[test]
    fn test_builtin_date_plus() {
        // Add 10 days to 2024-01-15
        let date = Value::List(vec![val_int(2024), val_int(1), val_int(15)]);
        let duration = Value::List(vec![val_int(10), Value::Str("Days".to_string())]);
        let result = builtin_date_plus(&[date, duration]).unwrap();
        match result {
            Value::List(elems) => {
                assert_eq!(elems[0], val_int(2024));
                assert_eq!(elems[1], val_int(1));
                assert_eq!(elems[2], val_int(25));
            }
            _ => panic!("Expected List"),
        }
    }

    #[test]
    fn test_builtin_date_difference() {
        let d1 = Value::List(vec![val_int(2024), val_int(1), val_int(1)]);
        let d2 = Value::List(vec![val_int(2024), val_int(1), val_int(11)]);
        let result = builtin_date_difference(&[d1, d2]).unwrap();
        match result {
            Value::Real(r) => {
                let diff = r.to_f64();
                // 10 days in seconds = 864000
                assert!((diff - 864000.0).abs() < 1.0);
            }
            _ => panic!("Expected Real"),
        }
    }

    #[test]
    fn test_builtin_date_difference_with_unit() {
        let d1 = Value::List(vec![val_int(2024), val_int(1), val_int(1)]);
        let d2 = Value::List(vec![val_int(2024), val_int(1), val_int(11)]);
        let result = builtin_date_difference(&[d1, d2, Value::Str("Days".to_string())]).unwrap();
        match result {
            Value::Real(r) => {
                let diff = r.to_f64();
                assert!((diff - 10.0).abs() < 0.01);
            }
            _ => panic!("Expected Real"),
        }
    }

    #[test]
    fn test_builtin_day_name() {
        let date = Value::List(vec![val_int(2024), val_int(1), val_int(1)]);
        let result = builtin_day_name(&[date]).unwrap();
        assert_eq!(result, Value::Str("Monday".to_string()));

        let date = Value::List(vec![val_int(2000), val_int(1), val_int(1)]);
        let result = builtin_day_name(&[date]).unwrap();
        assert_eq!(result, Value::Str("Saturday".to_string()));
    }

    #[test]
    fn test_builtin_month_name() {
        assert_eq!(builtin_month_name(&[val_int(1)]).unwrap(), Value::Str("January".to_string()));
        assert_eq!(builtin_month_name(&[val_int(2)]).unwrap(), Value::Str("February".to_string()));
        assert_eq!(builtin_month_name(&[val_int(12)]).unwrap(), Value::Str("December".to_string()));
    }

    #[test]
    fn test_builtin_day_count() {
        let d1 = Value::List(vec![val_int(2024), val_int(1), val_int(1)]);
        let d2 = Value::List(vec![val_int(2024), val_int(1), val_int(11)]);
        let result = builtin_day_count(&[d1, d2]).unwrap();
        assert_eq!(result, val_int(10));
    }

    #[test]
    fn test_days_since_epoch_to_ymd_backward() {
        // 1 day before epoch = 1969-12-31
        let (y, m, d) = days_since_epoch_to_ymd(-1, -1);
        assert_eq!((y, m, d), (1969, 12, 31));

        // 61 days before epoch = 1969-11-01 (Dec has 31, 61-31=30, Nov has 30)
        let (y, m, d) = days_since_epoch_to_ymd(-61, -1);
        assert_eq!((y, m, d), (1969, 11, 1));
    }

    #[test]
    fn test_parse_duration_spec() {
        let spec = Value::List(vec![val_int(5), Value::Str("Days".to_string())]);
        let (n, unit) = parse_duration_spec(&spec).unwrap();
        assert_eq!(n, 5.0);
        assert_eq!(unit, "Days");
    }
}
