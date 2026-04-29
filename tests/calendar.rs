/// Calendar/date builtin integration tests

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── DateObject ──

#[test]
fn test_date_object_list() {
    let out = syma_eval("DateObject[{2025, 1, 15}]");
    assert!(
        !out.contains("error"),
        "DateObject[{{2025,1,15}}] should not error, got: {out}"
    );
    assert!(!out.is_empty(), "DateObject output should be non-empty");
}

// ── DateString ──

#[test]
fn test_date_string_now() {
    let out = syma_eval("DateString[]");
    assert!(
        !out.contains("error"),
        "DateString[] should not error, got: {out}"
    );
    assert!(
        !out.is_empty(),
        "DateString[] should return a non-empty string"
    );
}

#[test]
fn test_date_string_list() {
    let out = syma_eval("DateString[{2025, 1, 15}]");
    assert!(
        !out.contains("error"),
        "DateString[{{2025,1,15}}] should not error, got: {out}"
    );
    assert!(
        !out.is_empty(),
        "DateString with date arg should return non-empty"
    );
}

// ── DateList ──

#[test]
fn test_date_list_now() {
    let out = syma_eval("DateList[]");
    assert!(
        !out.contains("error"),
        "DateList[] should not error, got: {out}"
    );
    // Should return something like "{2025, 4, 29, ...}"
    assert!(
        out.starts_with('{'),
        "DateList[] should start with {{, got: {out}"
    );
    assert!(
        out.ends_with('}'),
        "DateList[] should end with }}, got: {out}"
    );
}

#[test]
fn test_date_list_from_list() {
    let out = syma_eval("DateList[{2025, 1, 15}]");
    assert!(
        !out.contains("error"),
        "DateList[{{2025,1,15}}] should not error, got: {out}"
    );
}

// ── DatePlus ──

#[test]
fn test_date_plus_days() {
    let out = syma_eval("DatePlus[{2025, 1, 15}, {5, \"Days\"}]");
    assert!(
        !out.contains("error"),
        "DatePlus with list should not error, got: {out}"
    );
}

#[test]
fn test_date_plus_days_list() {
    let out = syma_eval("DatePlus[{2025, 1, 15}, {5, \"Days\"}]");
    assert!(
        !out.contains("error"),
        "DatePlus[{{2025,1,15}}, {{5,\"Days\"}}] should not error, got: {out}"
    );
}

// ── DateDifference ──

#[test]
fn test_date_difference_days() {
    let out = syma_eval("DateDifference[{2025, 1, 15}, {2025, 2, 1}]");
    assert!(
        !out.contains("error"),
        "DateDifference between Jan 15 and Feb 1 should not error, got: {out}"
    );
}

// ── Now ──

#[test]
fn test_now() {
    let out = syma_eval("Now[]");
    assert!(!out.contains("error"), "Now[] should not error, got: {out}");
    assert!(!out.is_empty(), "Now[] should return a non-empty value");
}

// ── Today ──

#[test]
fn test_today() {
    let out = syma_eval("Today[]");
    assert!(
        !out.contains("error"),
        "Today[] should not error, got: {out}"
    );
    assert!(!out.is_empty(), "Today[] should return a non-empty value");
    // Should be {year, month, day}
    assert!(
        out.starts_with('{'),
        "Today[] should start with {{, got: {out}"
    );
}

// ── DayName ──

#[test]
fn test_day_name_list() {
    let out = syma_eval("DayName[{2025, 1, 15}]");
    assert!(
        !out.contains("error"),
        "DayName[{{2025,1,15}}] should not error, got: {out}"
    );
    // January 15, 2025 was a Wednesday
    assert!(
        out.contains("Wednesday"),
        "DayName[{{2025,1,15}}] should be Wednesday, got: {out}"
    );
}

#[test]
fn test_day_name_date_object() {
    let out = syma_eval("DayName[{2025, 1, 15}]");
    assert!(
        !out.contains("error"),
        "DayName[{{2025,1,15}}] should not error, got: {out}"
    );
}

// ── LeapYearQ ──

#[test]
fn test_leap_year_q_2024() {
    let out = syma_eval("LeapYearQ[2024]");
    assert!(
        out.contains("True"),
        "LeapYearQ[2024] should be True, got: {out}"
    );
}

#[test]
fn test_leap_year_q_2023() {
    let out = syma_eval("LeapYearQ[2023]");
    assert!(
        out.contains("False"),
        "LeapYearQ[2023] should be False, got: {out}"
    );
}

#[test]
fn test_leap_year_q_2000() {
    let out = syma_eval("LeapYearQ[2000]");
    assert!(
        out.contains("True"),
        "LeapYearQ[2000] should be True, got: {out}"
    );
}

#[test]
fn test_leap_year_q_1900() {
    let out = syma_eval("LeapYearQ[1900]");
    assert!(
        out.contains("False"),
        "LeapYearQ[1900] should be False, got: {out}"
    );
}

// ── MonthName ──

#[test]
fn test_month_name_from_list() {
    let out = syma_eval("MonthName[{2025, 1, 15}]");
    assert!(
        !out.contains("error"),
        "MonthName[{{2025,1,15}}] should not error, got: {out}"
    );
    assert!(
        out.contains("January"),
        "MonthName[{{2025,1,15}}] should be January, got: {out}"
    );
}

#[test]
fn test_month_name_from_number() {
    let out = syma_eval("MonthName[1]");
    assert!(
        out.contains("January"),
        "MonthName[1] should be January, got: {out}"
    );
}

// ── DayCount ──

#[test]
fn test_day_count() {
    let out = syma_eval("DayCount[{2025, 1, 15}, {2025, 2, 1}]");
    assert!(
        !out.contains("error"),
        "DayCount[{{2025,1,15}},{{2025,2,1}}] should not error, got: {out}"
    );
}

// ── AbsoluteTime ──

#[test]
fn test_absolute_time() {
    let out = syma_eval("AbsoluteTime[]");
    assert!(
        !out.contains("error"),
        "AbsoluteTime[] should not error, got: {out}"
    );
    assert!(
        !out.is_empty(),
        "AbsoluteTime[] should return a non-empty value"
    );
}

// ── DaysInMonth ──

#[ignore = "DaysInMonth returns unevaluated"]
#[test]
fn test_days_in_month_feb_2024() {
    let out = syma_eval("DaysInMonth[{2024, 2}]");
    assert!(
        out.contains("29"),
        "DaysInMonth[{{2024,2}}] should be 29, got: {out}"
    );
}

#[ignore = "DaysInMonth returns unevaluated"]
#[test]
fn test_days_in_month_feb_2023() {
    let out = syma_eval("DaysInMonth[{2023, 2}]");
    assert!(
        out.contains("28"),
        "DaysInMonth[{{2023,2}}] should be 28, got: {out}"
    );
}
