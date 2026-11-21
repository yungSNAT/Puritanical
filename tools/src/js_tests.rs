// SPDX-License-Identifier: MIT

use super::*;
use serde_json::json;

#[test]
fn numbers_print_as_javascript_does() {
    let cases = [
        (f64::NAN, "NaN"),
        (f64::INFINITY, "Infinity"),
        (f64::NEG_INFINITY, "-Infinity"),
        (-0.0, "0"),
        (9.0, "9"),
        (0.1 + 0.2, "0.30000000000000004"),
        (0.000_001, "0.000001"),
        (1e-7, "1e-7"),
        (-1.5e-7, "-1.5e-7"),
        (1e21, "1e+21"),
        (123_456_789_012_345_680_000.0, "123456789012345680000"),
    ];
    for (x, want) in cases {
        assert_eq!(num(x), want, "String({x:e})");
    }
}

#[test]
fn to_fixed_rounds_the_exact_value_half_away_from_zero() {
    let cases = [
        (0.125, 2, "0.13"),
        (1.005, 2, "1.00"),
        (-0.125, 2, "-0.13"),
        (9.995, 0, "10"),
        (99.96, 1, "100.0"),
        (0.5, 0, "1"),
        (-0.0001, 2, "-0.00"),
        (12.0, 3, "12.000"),
        (f64::NAN, 2, "NaN"),
        (2e21, 2, "2e+21"),
    ];
    for (x, digits, want) in cases {
        assert_eq!(to_fixed(x, digits), want, "({x}).toFixed({digits})");
    }
}

#[test]
fn round_takes_halves_up() {
    let cases = [
        (2.5, "3"),
        (-2.5, "-2"),
        (0.499_999_999_999_999_94, "0"),
        (-0.4, "0"),
        (f64::NAN, "NaN"),
    ];
    for (x, want) in cases {
        assert_eq!(num(round(x)), want, "Math.round({x})");
    }
}

#[test]
fn strings_measure_in_utf16_units() {
    assert_eq!(len("a😀"), 3);
    assert_eq!(lossy(&slice("a😀b", 2)), "a\u{fffd}", "console.log");
    assert_eq!(
        stringify(&json!({"t": slice("a😀b", 2)}), 0),
        r#"{"t":"a\ud83d"}"#,
        "JSON.stringify keeps the lone surrogate"
    );
    assert_eq!(slice("abc", 10), "abc");
    assert_eq!(lossy("a😀"), "a😀");
    assert_eq!(trim("\u{feff}\u{a0} x \n"), "x");
    assert_eq!(
        trim("\u{85}x"),
        "\u{85}x",
        "NEL is not JavaScript whitespace"
    );
    assert_eq!(pad("ab", 4, true), "  ab");
    assert_eq!(pad("ab", 4, false), "ab  ");
    assert_eq!(pad("abcde", 4, true), "abcde");
}

#[test]
fn grouped_adds_thousands_separators() {
    assert_eq!(grouped(1_234_567.0), "1,234,567");
    assert_eq!(grouped(-1234.0), "-1,234");
    assert_eq!(grouped(999.0), "999");
    assert_eq!(grouped(12.5), "12.5");
}

#[test]
fn values_convert_as_javascript_does() {
    let falsy = [json!(null), json!(false), json!(0), json!(""), json!(0.0)];
    assert!(falsy.iter().all(|v| !truthy(Some(v))));
    assert!(!truthy(None));
    let truthy_values = [json!(true), json!(1), json!("0"), json!([]), json!({})];
    assert!(truthy_values.iter().all(|v| truthy(Some(v))));
    let strings = [
        (json!(null), "null"),
        (json!(true), "true"),
        (json!(1.5), "1.5"),
        (json!("s"), "s"),
        (json!([1, null, "a", [2, 3]]), "1,,a,2,3"),
        (json!({"a": 1}), "[object Object]"),
    ];
    for (v, want) in strings {
        assert_eq!(string(Some(&v)), want);
    }
    assert_eq!(string(None), "undefined");
    assert_eq!(num(number(Some(&json!(4)))), "4");
    assert!(number(Some(&json!("4"))).is_nan());
    assert!(number(None).is_nan());
    assert_eq!(num(number_or_zero(None)), "0");
    assert_eq!(num(number_or_zero(Some(&json!(7)))), "7");
}

#[test]
fn stringify_matches_json_stringify() {
    let v = json!({"a": [], "b": {}, "c": [1, {"d": "x\u{1}\"\n"}], "e": 2.0, "f": 1e21});
    assert_eq!(
        stringify(&v, 2),
        "{\n  \"a\": [],\n  \"b\": {},\n  \"c\": [\n    1,\n    {\n      \"d\": \"x\\u0001\\\"\\n\"\n    }\n  ],\n  \"e\": 2,\n  \"f\": 1e+21\n}"
    );
    assert_eq!(stringify(&json!({"a": [1, true]}), 0), "{\"a\":[1,true]}");
    assert_eq!(stringify(&json_num(f64::NAN), 2), "null");
    assert_eq!(stringify(&json!(u64::MAX), 0), "18446744073709552000");
}

#[test]
fn locale_compare_follows_the_root_collation() {
    assert_eq!(locale_compare("chisle", "chisle-trim"), Ordering::Less);
    assert_eq!(
        locale_compare("a_b", "a-b"),
        Ordering::Less,
        "low line sorts before hyphen"
    );
    assert_eq!(locale_compare("a-b", "a1"), Ordering::Less);
    assert_eq!(
        locale_compare("B", "a"),
        Ordering::Greater,
        "letters compare without case first"
    );
    assert_eq!(
        locale_compare("a", "A"),
        Ordering::Less,
        "then lower case first"
    );
    assert_eq!(
        locale_compare("é", "ü"),
        Ordering::Less,
        "outside ASCII: code point order"
    );
    assert_eq!(locale_compare("x", "x"), Ordering::Equal);
    assert_eq!(
        locale_compare("a b", "a_b"),
        Ordering::Less,
        "space sorts first"
    );
}

#[test]
fn split_cell_matches_the_greedy_pattern() {
    let arm = |c: char| is_word(c) || c == '.' || c == '-';
    assert_eq!(
        split_cell("cache__chisle-3.13.json", arm),
        Some(("cache", "chisle-3.13"))
    );
    assert_eq!(split_cell("a___b.json", is_word), Some(("a_", "b")));
    assert_eq!(split_cell("a__b__.json", is_word), Some(("a", "b__")));
    assert_eq!(split_cell("a__b-c.json", is_word), None);
    assert_eq!(split_cell("__b.json", is_word), None);
    assert_eq!(split_cell("a__b.txt", is_word), None);
    assert_eq!(split_cell("a\n__b.json", is_word), None);
    assert_eq!(split_cell("é__b.json", is_word), Some(("é", "b")));
}

#[test]
fn files_read_as_node_reads_them() {
    let dir = tempfile::tempdir().expect("temp dir");
    std::fs::write(dir.path().join("b.json"), b"{\"x\": \"\xff\"}").expect("write");
    std::fs::write(dir.path().join("a.json"), "not json").expect("write");
    assert_eq!(read_dir(dir.path()).expect("list"), ["a.json", "b.json"]);
    assert_eq!(
        read_json(&dir.path().join("b.json")),
        Some(json!({"x": "\u{fffd}"}))
    );
    assert_eq!(read_json(&dir.path().join("a.json")), None);
    assert_eq!(read_json(&dir.path().join("missing.json")), None);
    assert!(read_dir(&dir.path().join("missing")).is_err());
}
