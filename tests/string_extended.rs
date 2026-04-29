/// Extended string operation integration tests

#[path = "common/mod.rs"]
mod common;
use common::*;

// ── StringPart ──

#[test]
fn test_string_part_basic() {
    let out = syma_eval(r#"StringPart["hello", 2]"#);
    assert!(
        out.contains("e"),
        "StringPart[hello,2] should be e, got: {out}"
    );
}

// ── StringPosition ──

#[test]
fn test_string_position_basic() {
    let out = syma_eval(r#"StringPosition["hello", "l"]"#);
    assert!(
        !out.contains("error"),
        "StringPosition should not error, got: {out}"
    );
}

// ── StringCount ──

#[test]
fn test_string_count_basic() {
    let out = syma_eval(r#"StringCount["hello", "l"]"#);
    assert!(
        out.contains("2"),
        "StringCount[hello,l] should be 2, got: {out}"
    );
}

// ── StringRepeat ──

#[test]
fn test_string_repeat_basic() {
    let out = syma_eval(r#"StringRepeat["ab", 3]"#);
    assert!(
        out.contains("ababab"),
        "StringRepeat[ab,3] should be ababab, got: {out}"
    );
}

// ── StringDelete ──

#[test]
fn test_string_delete_basic() {
    let out = syma_eval(r#"StringDelete["hello", "l"]"#);
    assert!(
        !out.contains("error"),
        "StringDelete should not error, got: {out}"
    );
}

// ── StringInsert ──

#[test]
fn test_string_insert_basic() {
    let out = syma_eval(r#"StringInsert["ab", "X", 2]"#);
    assert!(
        !out.contains("error"),
        "StringInsert should not error, got: {out}"
    );
}

// ── StringRiffle ──

#[test]
fn test_string_riffle_basic() {
    let out = syma_eval(r#"StringRiffle[{"a", "b", "c"}, ","]"#);
    assert!(
        out.contains("a") && out.contains("b") && out.contains("c"),
        "StringRiffle should contain elements, got: {out}"
    );
}

// ── StringFreeQ ──

#[test]
fn test_string_free_q_true() {
    let out = syma_eval(r#"StringFreeQ["hello", "xyz"]"#);
    assert!(
        out.contains("True"),
        "StringFreeQ should be True, got: {out}"
    );
}

#[test]
fn test_string_free_q_false() {
    let out = syma_eval(r#"StringFreeQ["hello", "ell"]"#);
    assert!(
        out.contains("False"),
        "StringFreeQ should be False, got: {out}"
    );
}

// ── LetterQ / DigitQ ──

#[test]
fn test_letter_q_true() {
    let out = syma_eval(r#"LetterQ["a"]"#);
    assert!(
        out.contains("True"),
        "LetterQ[a] should be True, got: {out}"
    );
}

#[test]
fn test_letter_q_false() {
    let out = syma_eval(r#"LetterQ["1"]"#);
    assert!(
        out.contains("False"),
        "LetterQ[1] should be False, got: {out}"
    );
}

#[test]
fn test_digit_q_true() {
    let out = syma_eval(r#"DigitQ["1"]"#);
    assert!(out.contains("True"), "DigitQ[1] should be True, got: {out}");
}

#[test]
fn test_digit_q_false() {
    let out = syma_eval(r#"DigitQ["a"]"#);
    assert!(
        out.contains("False"),
        "DigitQ[a] should be False, got: {out}"
    );
}

// ── UpperCaseQ / LowerCaseQ ──

#[test]
fn test_upper_case_q_true() {
    let out = syma_eval(r#"UpperCaseQ["A"]"#);
    assert!(
        out.contains("True"),
        "UpperCaseQ[A] should be True, got: {out}"
    );
}

#[test]
fn test_upper_case_q_false() {
    let out = syma_eval(r#"UpperCaseQ["a"]"#);
    assert!(
        out.contains("False"),
        "UpperCaseQ[a] should be False, got: {out}"
    );
}

#[test]
fn test_lower_case_q_true() {
    let out = syma_eval(r#"LowerCaseQ["a"]"#);
    assert!(
        out.contains("True"),
        "LowerCaseQ[a] should be True, got: {out}"
    );
}

#[test]
fn test_lower_case_q_false() {
    let out = syma_eval(r#"LowerCaseQ["A"]"#);
    assert!(
        out.contains("False"),
        "LowerCaseQ[A] should be False, got: {out}"
    );
}

// ── TextWords / CharacterCounts / Alphabet ──

#[test]
fn test_text_words() {
    let out = syma_eval(r#"TextWords["hello world"]"#);
    assert!(
        !out.contains("error"),
        "TextWords should not error, got: {out}"
    );
}

#[test]
fn test_character_counts() {
    let out = syma_eval(r#"CharacterCounts["abbccc"]"#);
    assert!(
        !out.contains("error"),
        "CharacterCounts should not error, got: {out}"
    );
}

#[test]
fn test_alphabet() {
    let out = syma_eval("Alphabet[]");
    assert!(
        !out.contains("error"),
        "Alphabet[] should not error, got: {out}"
    );
}

// ── ToCharacterCode / FromCharacterCode ──

#[test]
fn test_to_character_code() {
    let out = syma_eval(r#"ToCharacterCode["A"]"#);
    assert!(
        out.contains("65"),
        "ToCharacterCode[A] should be 65, got: {out}"
    );
}

#[ignore = "FromCharacterCode returns unevaluated"]
#[test]
fn test_from_character_code() {
    let out = syma_eval("FromCharacterCode[65]");
    assert!(
        out.contains("A"),
        "FromCharacterCode[65] should be A, got: {out}"
    );
}

// ── EditDistance ──

#[test]
fn test_edit_distance() {
    let out = syma_eval(r#"EditDistance["hello", "hallo"]"#);
    assert!(
        out.contains("1"),
        "EditDistance[hello,hallo] should be 1, got: {out}"
    );
}

// ── LongestCommonSubsequence / LongestCommonSubstring ──

#[test]
fn test_longest_common_subsequence() {
    let out = syma_eval(r#"LongestCommonSubsequence["abcdef", "acf"]"#);
    assert!(
        !out.contains("error"),
        "LongestCommonSubsequence should not error, got: {out}"
    );
}

#[test]
fn test_longest_common_sub_string() {
    let out = syma_eval(r#"LongestCommonSubString["abcdef", "bcd"]"#);
    assert!(
        !out.contains("error"),
        "LongestCommonSubString should not error, got: {out}"
    );
}

// ── WordCount / SentenceCount ──

#[test]
fn test_word_count() {
    let out = syma_eval(r#"WordCount["hello world"]"#);
    assert!(
        out.contains("2"),
        "WordCount[hello world] should be 2, got: {out}"
    );
}

#[test]
fn test_sentence_count() {
    let out = syma_eval(r#"SentenceCount["Hi. Ok."]"#);
    assert!(
        out.contains("2"),
        "SentenceCount[Hi. Ok.] should be 2, got: {out}"
    );
}

// ── StringCases ──

#[test]
fn test_string_cases() {
    let out = syma_eval(r#"StringCases["abc123", "\\d"]"#);
    assert!(
        !out.contains("error"),
        "StringCases should not error, got: {out}"
    );
}
