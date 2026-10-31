// Rust에서만 생기는 경우(UTF-16 자르기, 숫자 글자 변환, Number() 해석)를 고정하는 테스트.
use super::*;

#[test]
fn js_len_counts_utf16_units() {
    assert_eq!(js_len("삼성전자"), 4);
    assert_eq!(js_len("abc"), 3);
    assert_eq!(js_len("😀"), 2);
}

#[test]
fn js_slice_cuts_by_utf16_units() {
    assert_eq!(js_slice("삼성전자", 0, 2), "삼성");
    assert_eq!(js_slice("abcdef", 2, 4), "cd");
    assert_eq!(js_slice("abc", 0, 10), "abc");
    // 이모지(2단위) 가운데를 자르게 되면 그 앞에서 자른다.
    assert_eq!(js_slice("a😀b", 0, 2), "a");
    assert_eq!(js_slice("a😀b", 0, 3), "a😀");
}

#[test]
fn js_trim_treats_bom_as_space_and_nel_as_text() {
    assert_eq!(js_trim("\u{FEFF} a \u{FEFF}"), "a");
    assert_eq!(js_trim("\u{85}a\u{85}"), "\u{85}a\u{85}");
    assert_eq!(js_trim(" \t\n a b \r\n"), "a b");
}

#[test]
fn js_number_writes_numbers_like_javascript() {
    assert_eq!(js_number(5.0), "5");
    assert_eq!(js_number(-5.0), "-5");
    assert_eq!(js_number(0.0), "0");
    assert_eq!(js_number(-0.0), "0");
    assert_eq!(js_number(0.5), "0.5");
    assert_eq!(js_number(0.1 + 0.2), "0.30000000000000004");
    assert_eq!(js_number(1_792_000_000_000.0), "1792000000000");
    assert_eq!(js_number(0.042), "0.042");
    assert_eq!(js_number(1e21), "1e+21");
    assert_eq!(js_number(1.5e-7), "1.5e-7");
    assert_eq!(js_number(f64::NAN), "NaN");
    assert_eq!(js_number(f64::INFINITY), "Infinity");
}

#[test]
fn js_parse_number_follows_string_to_number() {
    assert_eq!(js_parse_number("4100"), 4100.0);
    assert_eq!(js_parse_number(" 40.5 "), 40.5);
    assert_eq!(js_parse_number(""), 0.0);
    assert_eq!(js_parse_number("   "), 0.0);
    assert_eq!(js_parse_number("0x10"), 16.0);
    assert_eq!(js_parse_number("0b11"), 3.0);
    assert_eq!(js_parse_number("0o17"), 15.0);
    assert_eq!(js_parse_number("1e3"), 1000.0);
    assert_eq!(js_parse_number("+80"), 80.0);
    assert_eq!(js_parse_number("-.5"), -0.5);
    assert_eq!(js_parse_number("5."), 5.0);
    assert_eq!(js_parse_number("Infinity"), f64::INFINITY);
    assert!(js_parse_number("abc").is_nan());
    assert!(js_parse_number("12abc").is_nan());
    assert!(js_parse_number("inf").is_nan());
    assert!(js_parse_number("nan").is_nan());
    assert!(js_parse_number("1_000").is_nan());
    assert!(js_parse_number(".").is_nan());
    assert!(js_parse_number("e5").is_nan());
    assert!(js_parse_number("0x").is_nan());
    assert!(js_parse_number("-0x10").is_nan());
}
