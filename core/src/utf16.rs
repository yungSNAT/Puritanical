#![forbid(unsafe_code)]

//! UTF-16LE primitives matching JavaScript string and ECMAScript RegExp semantics.

/// Case-insensitive check for ECMAScript WhiteSpace and LineTerminator code units.
#[must_use]
pub fn is_js_whitespace(u: u16) -> bool {
    matches!(
        u,
        0x0009..=0x000D
            | 0x0020
            | 0x00A0
            | 0x1680
            | 0x2000..=0x200A
            | 0x2028
            | 0x2029
            | 0x202F
            | 0x205F
            | 0x3000
            | 0xFEFF
    )
}

/// ECMAScript RegExp LineTerminator code units (where `.` does not match).
#[must_use]
pub fn is_line_terminator(u: u16) -> bool {
    matches!(u, 0x000A | 0x000D | 0x2028 | 0x2029)
}

/// ECMAScript non-Unicode RegExp `\w` (`[A-Za-z0-9_]`).
#[must_use]
pub fn is_js_word(u: u16) -> bool {
    matches!(u, 0x0030..=0x0039 | 0x0041..=0x005A | 0x005F | 0x0061..=0x007A)
}

/// Returns `true` if `s[index - 1]` is a high surrogate and `s[index]` is a low surrogate.
#[must_use]
pub fn splits_surrogate_pair(s: &[u16], index: usize) -> bool {
    if index == 0 || index >= s.len() {
        return false;
    }
    let before = s[index - 1];
    let after = s[index];
    (0xD800..=0xDBFF).contains(&before) && (0xDC00..=0xDFFF).contains(&after)
}

/// Prefix of at most `max_units` UTF-16 code units without splitting a surrogate pair.
#[must_use]
pub fn safe_prefix(s: &[u16], max_units: usize) -> &[u16] {
    let mut end = s.len().min(max_units);
    if splits_surrogate_pair(s, end) {
        end -= 1;
    }
    &s[..end]
}

/// Suffix of at most `max_units` UTF-16 code units without splitting a surrogate pair.
#[must_use]
pub fn safe_suffix(s: &[u16], max_units: usize) -> &[u16] {
    let mut start = s.len().saturating_sub(max_units);
    if splits_surrogate_pair(s, start) {
        start += 1;
    }
    &s[start..]
}

/// Slice `s[start..end]` clamped and nudged inwards away from split surrogate pairs.
#[must_use]
pub fn safe_slice(s: &[u16], start: usize, end: usize) -> &[u16] {
    let mut safe_start = s.len().min(start);
    let mut safe_end = s.len().min(end).max(safe_start);
    if splits_surrogate_pair(s, safe_start) {
        safe_start += 1;
    }
    if splits_surrogate_pair(s, safe_end) {
        safe_end -= 1;
    }
    let hi = safe_start.max(safe_end);
    &s[safe_start..hi]
}

#[must_use]
pub fn trim(s: &[u16]) -> &[u16] {
    trim_end(trim_start(s))
}

#[must_use]
pub fn trim_start(s: &[u16]) -> &[u16] {
    let mut i = 0;
    while i < s.len() && is_js_whitespace(s[i]) {
        i += 1;
    }
    &s[i..]
}

#[must_use]
pub fn trim_end(s: &[u16]) -> &[u16] {
    let mut i = s.len();
    while i > 0 && is_js_whitespace(s[i - 1]) {
        i -= 1;
    }
    &s[..i]
}

pub fn push_ascii(out: &mut Vec<u16>, s: &str) {
    out.extend(s.encode_utf16());
}

/// Formats `n` with ASCII US thousands commas (`en-US`), matching `n.toLocaleString('en-US')`.
pub fn push_commas(out: &mut Vec<u16>, mut n: usize) {
    let mut buf = [0u8; 32];
    let mut pos = buf.len();
    let mut digits = 0usize;
    loop {
        if digits > 0 && digits.is_multiple_of(3) {
            pos -= 1;
            buf[pos] = b',';
        }
        pos -= 1;
        buf[pos] = b'0' + (n % 10) as u8;
        digits += 1;
        n /= 10;
        if n == 0 {
            break;
        }
    }
    for &b in &buf[pos..] {
        out.push(u16::from(b));
    }
}

pub fn push_usize(out: &mut Vec<u16>, n: usize) {
    let mut buf = [0u8; 24];
    let mut pos = buf.len();
    let mut x = n;
    loop {
        pos -= 1;
        buf[pos] = b'0' + (x % 10) as u8;
        x /= 10;
        if x == 0 {
            break;
        }
    }
    for &b in &buf[pos..] {
        out.push(u16::from(b));
    }
}

#[must_use]
pub fn ascii_lower(s: &[u16]) -> Vec<u16> {
    s.iter()
        .map(|&u| {
            if (b'A' as u16..=b'Z' as u16).contains(&u) {
                u + 32
            } else {
                u
            }
        })
        .collect()
}

#[must_use]
pub fn starts_with_ascii(s: &[u16], pat: &[u8]) -> bool {
    s.len() >= pat.len()
        && s[..pat.len()]
            .iter()
            .zip(pat)
            .all(|(&u, &b)| u == u16::from(b))
}

#[must_use]
pub fn starts_with_ascii_ci(s: &[u16], pat_lower: &[u8]) -> bool {
    s.len() >= pat_lower.len()
        && s[..pat_lower.len()].iter().zip(pat_lower).all(|(&u, &b)| {
            let l = if (b'A' as u16..=b'Z' as u16).contains(&u) {
                u + 32
            } else {
                u
            };
            l == u16::from(b)
        })
}

#[must_use]
pub fn eq_ascii(s: &[u16], pat: &[u8]) -> bool {
    s.len() == pat.len() && starts_with_ascii(s, pat)
}

#[must_use]
pub fn eq_ascii_ci(s: &[u16], pat_lower: &[u8]) -> bool {
    s.len() == pat_lower.len() && starts_with_ascii_ci(s, pat_lower)
}

#[must_use]
pub fn find_subslice(haystack: &[u16], needle: &[u16]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

#[must_use]
pub fn find_ascii(haystack: &[u16], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    if haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .position(|w| w.iter().zip(needle).all(|(&u, &b)| u == u16::from(b)))
}

#[must_use]
pub fn find_ascii_ci(haystack: &[u16], needle_lower: &[u8]) -> Option<usize> {
    if needle_lower.is_empty() {
        return Some(0);
    }
    if haystack.len() < needle_lower.len() {
        return None;
    }
    haystack.windows(needle_lower.len()).position(|w| {
        w.iter().zip(needle_lower).all(|(&u, &b)| {
            let l = if (b'A' as u16..=b'Z' as u16).contains(&u) {
                u + 32
            } else {
                u
            };
            l == u16::from(b)
        })
    })
}

/// Checks a word-boundary match at byte span `[start..end]` in `s`.
#[must_use]
pub fn has_word_bounds(s: &[u16], start: usize, end: usize) -> bool {
    let left_ok = start == 0 || !is_js_word(s[start - 1]);
    let right_ok = end >= s.len() || !is_js_word(s[end]);
    left_ok && right_ok
}

#[must_use]
pub fn contains_ascii_word(haystack: &[u16], word_lower: &[u8]) -> bool {
    let mut i = 0;
    while i + word_lower.len() <= haystack.len() {
        if starts_with_ascii_ci(&haystack[i..], word_lower)
            && has_word_bounds(haystack, i, i + word_lower.len())
        {
            return true;
        }
        i += 1;
    }
    false
}

/// Joins slices with a single UTF-16 separator code unit.
pub fn join_u16(slices: &[&[u16]], sep: u16) -> Vec<u16> {
    let total: usize =
        slices.iter().map(|s| s.len()).sum::<usize>() + slices.len().saturating_sub(1);
    let mut out = Vec::with_capacity(total);
    for (idx, part) in slices.iter().enumerate() {
        if idx > 0 {
            out.push(sep);
        }
        out.extend_from_slice(part);
    }
    out
}

/// Splits on `\n` (`0x000A`), matching JS `String.prototype.split('\n')`.
pub fn split_lf(s: &[u16]) -> Vec<&[u16]> {
    s.split(|&u| u == 0x000A).collect()
}

/// Parses an ASCII unsigned base-10 integer slice if all code units are digits.
#[must_use]
pub fn parse_ascii_digits(s: &[u16]) -> Option<usize> {
    if s.is_empty() {
        return None;
    }
    let mut val = 0usize;
    for &u in s {
        if !(b'0' as u16..=b'9' as u16).contains(&u) {
            return None;
        }
        val = val
            .checked_mul(10)?
            .checked_add(usize::from(u - u16::from(b'0')))?;
    }
    Some(val)
}

#[cfg(test)]
#[must_use]
pub fn u16_from_str(s: &str) -> Vec<u16> {
    s.encode_utf16().collect()
}

#[cfg(test)]
#[must_use]
pub fn str_from_u16_lossy(s: &[u16]) -> String {
    String::from_utf16_lossy(s)
}
