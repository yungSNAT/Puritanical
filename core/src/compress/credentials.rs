#![forbid(unsafe_code)]

//! Secret and credential detection to guard spill files in compressed output.

use super::{is_ascii_alnum, is_ascii_digit, is_ascii_upper};
use crate::utf16::{is_js_whitespace, is_js_word, starts_with_ascii, starts_with_ascii_ci};

const AUTH_WINDOW: usize = 200;

fn is_base64ish_slash_plus(u: u16) -> bool {
    is_ascii_alnum(u) || u == u16::from(b'/') || u == u16::from(b'+')
}

fn is_token_dash(u: u16) -> bool {
    is_js_word(u) || u == u16::from(b'-')
}

fn is_aad_char(u: u16) -> bool {
    is_js_word(u) || matches!(u, 0x007E | 0x002E | 0x002D)
}

fn count_run(slice: &[u16], pred: fn(u16) -> bool) -> usize {
    slice.iter().take_while(|&&u| pred(u)).count()
}

fn is_pem_private_key(text: &[u16]) -> bool {
    let prefix = b"-----BEGIN ";
    let suffix = b"PRIVATE KEY";
    let mut i = 0;
    while i + prefix.len() + suffix.len() <= text.len() {
        if starts_with_ascii(&text[i..], prefix) {
            let mut k = i + prefix.len();
            while k < text.len() && (matches!(text[k], 0x0041..=0x005A | 0x0030..=0x0039 | 0x0020))
            {
                if starts_with_ascii(&text[k..], suffix) {
                    return true;
                }
                k += 1;
            }
        }
        i += 1;
    }
    false
}

fn is_aws_key_id(text: &[u16], i: usize) -> bool {
    let rest = &text[i..];
    rest.len() >= 20
        && [b"AKIA", b"ASIA", b"ABIA", b"ACCA"]
            .iter()
            .any(|&p| starts_with_ascii(rest, p))
        && rest[4..20]
            .iter()
            .all(|&u| is_ascii_upper(u) || is_ascii_digit(u))
        && (i + 20 == text.len() || !is_js_word(text[i + 20]))
}

fn is_prefixed_token_at(rest: &[u16]) -> bool {
    if (starts_with_ascii(rest, b"AIza") && count_run(&rest[4..], is_token_dash) >= 35)
        || (starts_with_ascii(rest, b"ya29.") && count_run(&rest[5..], is_token_dash) >= 20)
        || (starts_with_ascii(rest, b"GOCSPX-") && count_run(&rest[7..], is_token_dash) >= 20)
        || (starts_with_ascii(rest, b"github_pat_") && count_run(&rest[11..], is_js_word) >= 22)
    {
        return true;
    }
    rest.len() >= 40
        && starts_with_ascii(rest, b"gh")
        && matches!(rest[2], 0x0070 | 0x006F | 0x0075 | 0x0073 | 0x0072)
        && rest[3] == u16::from(b'_')
        && count_run(&rest[4..], is_ascii_alnum) >= 36
}

fn has_cloud_or_git_token(text: &[u16]) -> bool {
    (0..text.len()).any(|i| {
        let left_bound = i == 0 || !is_js_word(text[i - 1]);
        left_bound && (is_aws_key_id(text, i) || is_prefixed_token_at(&text[i..]))
    })
}

fn is_aws_secret_at(rest: &[u16]) -> bool {
    let aws_kw = b"aws_secret_access_key";
    if !starts_with_ascii_ci(rest, aws_kw) {
        return false;
    }
    let mut k = aws_kw.len() + usize::from(matches!(rest.get(aws_kw.len()), Some(0x0022 | 0x0027)));
    while matches!(rest.get(k), Some(0x0020 | 0x0009)) {
        k += 1;
    }
    if !matches!(rest.get(k), Some(0x003A | 0x003D)) {
        return false;
    }
    k += 1;
    while matches!(rest.get(k), Some(0x0020 | 0x0009)) {
        k += 1;
    }
    k += usize::from(matches!(rest.get(k), Some(0x0022 | 0x0027)));
    rest.get(k..k + 40)
        .is_some_and(|s| s.iter().all(|&u| is_base64ish_slash_plus(u)))
}

fn is_azure_key_at(text: &[u16], i: usize) -> bool {
    let rest = &text[i..];
    if (i == 0 || !is_js_word(text[i - 1]))
        && (starts_with_ascii(rest, b"AccountKey=") || starts_with_ascii(rest, b"SharedAccessKey="))
    {
        let k = if starts_with_ascii(rest, b"AccountKey=") {
            11
        } else {
            16
        };
        if count_run(&rest[k..], is_base64ish_slash_plus) >= 20 {
            return true;
        }
    }
    (starts_with_ascii(rest, b"?sig=") || starts_with_ascii(rest, b"&sig="))
        && count_run(&rest[5..], |u| {
            is_base64ish_slash_plus(u) || u == u16::from(b'%')
        }) >= 20
}

fn is_aad_secret_at(text: &[u16], i: usize) -> bool {
    let rest = &text[i..];
    rest.len() >= 37
        && has_word_start_only(text, i)
        && is_aad_char(rest[0])
        && is_aad_char(rest[1])
        && is_aad_char(rest[2])
        && is_ascii_digit(rest[3])
        && rest[4] == u16::from(b'Q')
        && rest[5] == u16::from(b'~')
        && rest[6..37].iter().all(|&u| is_aad_char(u))
}

fn has_azure_or_secret_patterns(text: &[u16]) -> bool {
    (0..text.len()).any(|i| {
        is_aws_secret_at(&text[i..]) || is_azure_key_at(text, i) || is_aad_secret_at(text, i)
    })
}

/// Exact ECMAScript `\b` match at `i`.
fn has_word_start_only(s: &[u16], i: usize) -> bool {
    let prev_word = i > 0 && is_js_word(s[i - 1]);
    let curr_word = is_js_word(s[i]);
    prev_word != curr_word
}

fn is_auth_word(word: &[u16]) -> bool {
    if word.len() >= 24 {
        return true;
    }
    if word.len() < 8 {
        return false;
    }
    word.iter().enumerate().any(|(idx, &u)| {
        is_ascii_digit(u)
            || matches!(
                u,
                0x003D | 0x002B | 0x002F | 0x005F | 0x002E | 0x007E | 0x002D
            )
            || (idx > 0 && is_ascii_upper(u))
    })
}

fn contains_auth_header(text: &[u16]) -> bool {
    let kw = b"authorization";
    let mut i = 0;
    while i + kw.len() < text.len() {
        if starts_with_ascii_ci(&text[i..], kw) {
            let mut k =
                i + kw.len() + usize::from(matches!(text.get(i + kw.len()), Some(0x0022 | 0x0027)));
            while matches!(text.get(k), Some(0x0020 | 0x0009)) {
                k += 1;
            }
            if matches!(text.get(k), Some(0x003A | 0x003D)) {
                let cap_end = (k + 1 + AUTH_WINDOW).min(text.len());
                let mut end = k + 1;
                while end < cap_end && !matches!(text[end], 0x000A | 0x000D) {
                    end += 1;
                }
                let has_secret = text[k + 1..end]
                    .split(|&u| {
                        is_js_whitespace(u) || matches!(u, 0x0022 | 0x0027 | 0x002C | 0x003B)
                    })
                    .any(|w| !w.is_empty() && is_auth_word(w));
                if has_secret {
                    return true;
                }
                i = end;
                continue;
            }
        }
        i += 1;
    }
    false
}

fn contains_jwt(text: &[u16]) -> bool {
    let is_token_char = |u: u16| is_js_word(u) || u == u16::from(b'.') || u == u16::from(b'-');
    for token in text.split(|&u| !is_token_char(u)) {
        if token.is_empty() {
            continue;
        }
        let parts: Vec<&[u16]> = token.split(|&u| u == u16::from(b'.')).collect();
        if parts.len() < 3 {
            continue;
        }
        for i in 0..parts.len() - 2 {
            if let Some(at) = parts[i]
                .windows(3)
                .position(|w| w == [b'e' as u16, b'y' as u16, b'J' as u16])
                && parts[i].len() - at >= 13
                && parts[i + 1].len() >= 10
                && parts[i + 2].len() >= 10
            {
                return true;
            }
        }
    }
    false
}

#[must_use]
pub fn contains_credential(text: &[u16]) -> bool {
    is_pem_private_key(text)
        || has_cloud_or_git_token(text)
        || has_azure_or_secret_patterns(text)
        || contains_jwt(text)
        || contains_auth_header(text)
}
