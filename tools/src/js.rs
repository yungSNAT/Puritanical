// SPDX-License-Identifier: MIT

//! JavaScript semantics the generated files depend on: number printing,
//! `toFixed`, `Math.round`, UTF-16 lengths, `trim`, `JSON.stringify` and
//! `localeCompare`. The outputs were first written by Node, so matching its
//! rules keeps every committed byte the same.

use serde_json::Value;
use std::cmp::Ordering;
use std::fmt::Write as _;
use std::path::Path;

/// `String(x)` for a number.
#[must_use]
pub fn num(x: f64) -> String {
    if x.is_nan() {
        return "NaN".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.into();
    }
    if x == 0.0 {
        return "0".into();
    }
    let a = x.abs();
    if (1e-6..1e21).contains(&a) {
        return format!("{x}");
    }
    // Both print the shortest round-trip digits; JavaScript signs the exponent.
    let s = format!("{x:e}");
    match s.split_once('e') {
        Some((m, e)) if !e.starts_with('-') => format!("{m}e+{e}"),
        _ => s,
    }
}

/// `x.toFixed(digits)`: the exact binary value, ties rounded away from zero.
///
/// # Panics
/// Never: fixed notation always has a point and ASCII digits.
#[must_use]
pub fn to_fixed(x: f64, digits: usize) -> String {
    if !x.is_finite() || x.abs() >= 1e21 {
        return num(x);
    }
    let negative = x < 0.0;
    // 1100 places hold every finite double's exact expansion.
    let exact = format!("{:.1100}", x.abs());
    let (int, frac) = exact.split_once('.').expect("fixed notation has a point");
    let mut kept: Vec<u8> = int.bytes().chain(frac.bytes().take(digits)).collect();
    if frac.as_bytes()[digits] >= b'5' {
        let mut i = kept.len();
        loop {
            if i == 0 {
                kept.insert(0, b'1');
                break;
            }
            i -= 1;
            if kept[i] == b'9' {
                kept[i] = b'0';
            } else {
                kept[i] += 1;
                break;
            }
        }
    }
    let split = kept.len() - digits;
    let mut out = String::from(if negative { "-" } else { "" });
    out.push_str(std::str::from_utf8(&kept[..split]).expect("ASCII digits"));
    if digits > 0 {
        out.push('.');
        out.push_str(std::str::from_utf8(&kept[split..]).expect("ASCII digits"));
    }
    out
}

/// `Math.round`: halves round up, towards positive infinity.
#[must_use]
pub fn round(x: f64) -> f64 {
    let f = x.floor();
    if x - f >= 0.5 { f + 1.0 } else { f }
}

/// `s.length`: UTF-16 code units.
#[must_use]
pub fn len(s: &str) -> usize {
    units(s).count()
}

/// The UTF-16 code units JavaScript holds for `s`: a stand-in is its one lone surrogate.
fn units(s: &str) -> impl Iterator<Item = u16> + '_ {
    s.chars().flat_map(|c| {
        let mut buf = [0; 2];
        let n = match lone(c) {
            Some(u) => {
                buf[0] = u as u16;
                1
            }
            None => c.encode_utf16(&mut buf).len(),
        };
        buf.into_iter().take(n)
    })
}

/// The stand-in for lone surrogate `0xD800 + k` is U+10F800 + k: the last
/// 2,048 code points of plane 16, which are private use.
const LONE: u32 = 0x10_F800;

/// The lone surrogate a stand-in holds.
fn lone(c: char) -> Option<u32> {
    u32::from(c).checked_sub(LONE).map(|k| 0xD800 + k)
}

/// `s.slice(0, n)` by UTF-16 code units. A cut through a surrogate pair
/// leaves a lone surrogate, which a Rust string cannot hold, so a stand-in
/// keeps it: `stringify` writes it as `\udXXX`, as `JSON.stringify` does,
/// and [`lossy`] prints it as U+FFFD, as Node's console does.
///
/// # Panics
/// Never: every surrogate's stand-in is a valid code point.
#[must_use]
pub fn slice(s: &str, n: usize) -> String {
    char::decode_utf16(units(s).take(n))
        .map(|r| {
            r.unwrap_or_else(|e| {
                char::from_u32(LONE + u32::from(e.unpaired_surrogate()) - 0xD800).expect("plane 16")
            })
        })
        .collect()
}

/// A [`slice`] as Node prints it: a lone surrogate becomes U+FFFD.
#[must_use]
pub fn lossy(s: &str) -> String {
    s.chars()
        .map(|c| if lone(c).is_some() { '\u{fffd}' } else { c })
        .collect()
}

/// JavaScript's whitespace and line terminators (`\s` and `trim`).
#[must_use]
pub fn is_space(c: char) -> bool {
    matches!(
        c,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// `s.trim()`.
#[must_use]
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_space)
}

/// `s.padStart(width)` / `s.padEnd(width)` with spaces.
#[must_use]
pub fn pad(s: &str, width: usize, start: bool) -> String {
    let fill = " ".repeat(width.saturating_sub(len(s)));
    if start {
        fill + s
    } else {
        format!("{s}{fill}")
    }
}

/// `n.toLocaleString('en-US')` for a whole number.
#[must_use]
pub fn grouped(n: f64) -> String {
    if !n.is_finite() || n.fract() != 0.0 || n.abs() >= 1e21 {
        return num(n);
    }
    let digits = num(n.abs());
    let mut out = String::from(if n < 0.0 { "-" } else { "" });
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// Truthiness of a parsed JSON value; a missing value is `undefined`.
#[must_use]
pub fn truthy(v: Option<&Value>) -> bool {
    match v {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|x| x != 0.0 && !x.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// `String(v)`, as a template literal prints a value.
#[must_use]
pub fn string(v: Option<&Value>) -> String {
    match v {
        None => "undefined".into(),
        Some(Value::Null) => "null".into(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(Value::Number(n)) => num(as_f64(n)),
        Some(Value::String(s)) => s.clone(),
        Some(Value::Array(a)) => a
            .iter()
            .map(|x| match x {
                Value::Null => String::new(),
                x => string(Some(x)),
            })
            .collect::<Vec<_>>()
            .join(","),
        Some(Value::Object(_)) => "[object Object]".into(),
    }
}

/// A JSON number as JavaScript holds it: always a double.
///
/// # Panics
/// Never: every `serde_json` number converts to f64.
#[must_use]
pub fn as_f64(n: &serde_json::Number) -> f64 {
    n.as_f64().expect("serde_json numbers convert to f64")
}

/// A numeric field: the number, or NaN for anything else, as arithmetic on it would give.
#[must_use]
pub fn number(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Number(n)) => as_f64(n),
        _ => f64::NAN,
    }
}

/// `v` as an operand of `-`: `null` is 0, `true` is 1. Strings are not
/// parsed (NaN); token counts are numbers.
#[must_use]
pub fn operand(v: Option<&Value>) -> f64 {
    match v {
        Some(Value::Null | Value::Bool(false)) => 0.0,
        Some(Value::Bool(true)) => 1.0,
        v => number(v),
    }
}

/// `v || 0` read as a number.
#[must_use]
pub fn number_or_zero(v: Option<&Value>) -> f64 {
    if truthy(v) { number(v) } else { 0.0 }
}

/// `JSON.stringify(v, null, indent)`.
#[must_use]
pub fn stringify(v: &Value, indent: usize) -> String {
    let mut out = String::new();
    write_json(&mut out, v, indent, 0);
    out
}

fn write_json(out: &mut String, v: &Value, indent: usize, depth: usize) {
    let pretty = (indent, depth);
    match v {
        Value::Array(items) if !items.is_empty() => {
            write_items(out, "[]", items.iter().map(|i| (None, i)), pretty);
        }
        Value::Object(map) if !map.is_empty() => {
            write_items(out, "{}", map.iter().map(|(k, i)| (Some(k), i)), pretty);
        }
        v => out.push_str(&scalar(v)),
    }
}

/// A value with nothing inside it to indent.
fn scalar(v: &Value) -> String {
    match v {
        Value::Null => "null".into(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => num(as_f64(n)),
        Value::String(s) => quote(s),
        Value::Array(_) => "[]".into(),
        Value::Object(_) => "{}".into(),
    }
}

/// A JSON string literal; a [`slice`]'s stand-in becomes its `\udXXX` escape.
fn quote(s: &str) -> String {
    Value::String(s.to_owned())
        .to_string()
        .chars()
        .map(|c| lone(c).map_or_else(|| c.to_string(), |u| format!("\\u{u:04x}")))
        .collect()
}

/// An array or object's members between `brackets`, one per line when indenting.
fn write_items<'a>(
    out: &mut String,
    brackets: &str,
    items: impl Iterator<Item = (Option<&'a String>, &'a Value)>,
    (indent, depth): (usize, usize),
) {
    let (open, close) = brackets.split_at(1);
    out.push_str(open);
    for (i, (key, item)) in items.enumerate() {
        out.push_str(if i == 0 { "" } else { "," });
        newline(out, indent, depth + 1);
        if let Some(k) = key {
            let _ = write!(out, "{}:{}", quote(k), if indent > 0 { " " } else { "" });
        }
        write_json(out, item, indent, depth + 1);
    }
    newline(out, indent, depth);
    out.push_str(close);
}

fn newline(out: &mut String, indent: usize, depth: usize) {
    if indent > 0 {
        out.push('\n');
        out.push_str(&" ".repeat(indent * depth));
    }
}

/// A JSON number from a double, keeping NaN as `null` as `JSON.stringify` does.
#[must_use]
pub fn json_num(x: f64) -> Value {
    serde_json::Number::from_f64(x).map_or(Value::Null, Value::Number)
}

/// `a.localeCompare(b)` under the default (root) collation, for ASCII
/// identifiers: space, then punctuation, then digits, then letters; letters
/// compared without case first, then lower case before upper case.
///
/// # Panics
/// Never: the collation table is short.
#[must_use]
pub fn locale_compare(a: &str, b: &str) -> Ordering {
    const ORDER: &str = " _-,;:!?.'\"()[]{}@*/\\&#%`^+<=>|~$0123456789abcdefghijklmnopqrstuvwxyz";
    let primary = |c: char| {
        ORDER
            .find(c.to_ascii_lowercase())
            .map_or(u32::from(c) + 128, |i| {
                u32::try_from(i).expect("short table")
            })
    };
    let key = |s: &str| s.chars().map(primary).collect::<Vec<_>>();
    let case = |s: &str| {
        s.chars()
            .map(|c| c.is_ascii_uppercase())
            .collect::<Vec<_>>()
    };
    key(a).cmp(&key(b)).then_with(|| case(a).cmp(&case(b)))
}

/// `fs.readFileSync(p, 'utf8')`: invalid UTF-8 becomes U+FFFD.
///
/// # Errors
/// The file cannot be read, or holds a code point [`RESERVED`] for stand-ins.
pub fn read(p: &Path) -> std::io::Result<String> {
    let text = String::from_utf8_lossy(&std::fs::read(p)?).into_owned();
    if text.chars().any(|c| lone(c).is_some()) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            RESERVED,
        ));
    }
    Ok(text)
}

/// Why input holding a [`slice`] stand-in code point is refused.
pub const RESERVED: &str = "holds a code point from U+10F800..U+10FFFF, which puritanical-tools reserves for lone surrogates";

/// `JSON.parse(fs.readFileSync(p, 'utf8'))`, or `None` where either throws.
///
/// # Panics
/// The file holds a code point from U+10F800..U+10FFFF, which the port
/// reserves for lone surrogates; refusing it beats corrupting it.
#[must_use]
pub fn read_json(p: &Path) -> Option<Value> {
    let text = match read(p) {
        Err(e) if e.kind() == std::io::ErrorKind::InvalidData => panic!("{}: {e}", p.display()),
        r => r.ok()?,
    };
    let text = keep_lone(&text).unwrap_or_else(|| panic!("{}: {RESERVED}", p.display()));
    serde_json::from_str(&text).ok()
}

/// JSON text with each escaped lone surrogate (`"\ud800"`, which `JSON.parse`
/// accepts and `serde_json` refuses) replaced by its [`slice`] stand-in, so
/// the record parses and prints back as it was. `None` when the text already
/// holds a stand-in code point, raw or escaped.
pub(crate) fn keep_lone(text: &str) -> Option<String> {
    if text.chars().any(|c| lone(c).is_some()) {
        return None;
    }
    let hex = |i: usize| {
        text.get(i..i + 6)
            .and_then(|e| e.strip_prefix("\\u"))
            .filter(|h| h.bytes().all(|b| b.is_ascii_hexdigit()))
            .and_then(|h| u32::from_str_radix(h, 16).ok())
    };
    let b = text.as_bytes();
    let (mut out, mut from, mut i) = (String::with_capacity(text.len()), 0, 0);
    while i < b.len() {
        if b[i] != b'\\' {
            i += 1;
            continue;
        }
        let Some(u) = hex(i) else {
            i += 2;
            continue;
        };
        let low = hex(i + 6).filter(|l| (0xDC00..0xE000).contains(l));
        if (0xD800..0xDC00).contains(&u) && low.is_some() {
            if u >= 0xDBFE {
                return None;
            }
            i += 12;
            continue;
        }
        if (0xD800..0xE000).contains(&u) {
            out.push_str(&text[from..i]);
            out.push(char::from_u32(LONE + u - 0xD800)?);
            from = i + 6;
        }
        i += 6;
    }
    out.push_str(&text[from..]);
    Some(out)
}

/// `fs.readdirSync(p)`: names in byte order, as libuv sorts them.
///
/// # Errors
/// The directory cannot be listed.
pub fn read_dir(p: &Path) -> std::io::Result<Vec<String>> {
    std::fs::read_dir(p)?
        .map(|e| e.map(|e| e.file_name().to_string_lossy().into_owned()))
        .collect::<std::io::Result<Vec<_>>>()
        .map(|mut names| {
            names.sort();
            names
        })
}

/// Splits `<task>__<arm>.json` as `/^(.+)__(<arm chars>+)\.json$/` does: the
/// greedy task takes the rightmost `__` that leaves a valid arm.
#[must_use]
pub fn split_cell(name: &str, arm_char: fn(char) -> bool) -> Option<(&str, &str)> {
    let stem = name.strip_suffix(".json")?;
    // Overlapping matches: `a___b` may split after `a` or after `a_`.
    (1..stem.len())
        .rev()
        .filter(|&i| {
            stem.is_char_boundary(i) && stem[i..].starts_with("__") && !stem[..i].contains('\n')
        })
        .map(|i| (&stem[..i], &stem[i + 2..]))
        .find(|(_, arm)| !arm.is_empty() && arm.chars().all(arm_char))
}

/// `\w` without the `u` flag.
#[must_use]
pub fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
#[path = "js_tests.rs"]
mod tests;
