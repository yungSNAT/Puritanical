//! Safe UTF-16LE linear-memory ABI for `wasm32-unknown-unknown`.

use std::cell::RefCell;

use crate::compress::{
    bound_for_opencode, compress, contains_credential, duplicate_marker, minify_json, salvage_line,
    scrub, unsafe_tool_name,
};
use crate::filters::{env_assign_end, filter_command, kinds_for_csv, wrappable};
use crate::mode::{Mode, requested_mode};
use crate::sections::{filter_sections, is_old_statusline_command, is_owned_statusline_command};

const MAX_CACHED_U16: usize = 65_536;

thread_local! {
    static ARG0: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    static ARG1: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    static ARG2: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
    static RESULT: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
}

fn with_slot<R>(slot: u32, f: impl FnOnce(&mut Vec<u16>) -> R) -> R {
    match slot {
        0 => ARG0.with(|c| f(&mut c.borrow_mut())),
        1 => ARG1.with(|c| f(&mut c.borrow_mut())),
        _ => ARG2.with(|c| f(&mut c.borrow_mut())),
    }
}

fn take_slot(slot: u32) -> Vec<u16> {
    with_slot(slot, std::mem::take)
}

fn return_slot(slot: u32, mut buf: Vec<u16>) {
    if buf.capacity() <= MAX_CACHED_U16 {
        buf.clear();
        with_slot(slot, |cell| *cell = buf);
    }
}

fn store_opt_result(res: Option<Vec<u16>>) -> isize {
    match res {
        None => -1,
        Some(v) => store_result(v),
    }
}

fn store_result(v: Vec<u16>) -> isize {
    let n = v.len() as isize;
    RESULT.with(|r| *r.borrow_mut() = v);
    n
}

/// Splits NUL-terminated heading lists passed as `u16` slices, preserving empty elements.
fn split_nul(buf: &[u16]) -> Vec<&[u16]> {
    if buf.is_empty() {
        return Vec::new();
    }
    let body = buf.strip_suffix(&[0]).unwrap_or(buf);
    body.split(|&u| u == 0).collect()
}

/// Resizes input buffer `slot` (`0..=2`) to `len` initialised `u16` units and returns its pointer.
#[unsafe(no_mangle)]
pub extern "C" fn arg_buf(slot: u32, len: usize) -> *mut u16 {
    with_slot(slot, |v| {
        v.resize(len, 0);
        v.as_mut_ptr()
    })
}

/// Returns the linear-memory pointer to the latest result written by an export.
#[unsafe(no_mangle)]
pub extern "C" fn result_ptr() -> *const u16 {
    RESULT.with(|r| r.borrow().as_ptr())
}

/// Returns `-1` for `null`, `0` for `'off'`, `1` for `'on'`.
#[unsafe(no_mangle)]
pub extern "C" fn requested_mode_u16() -> isize {
    let prompt = take_slot(0);
    let code = match requested_mode(&prompt) {
        None => -1,
        Some(Mode::Off) => 0,
        Some(Mode::On) => 1,
    };
    return_slot(0, prompt);
    code
}

/// Returns `-1` if unchanged, else the UTF-16LE unit count stored at `result_ptr()`.
#[unsafe(no_mangle)]
pub extern "C" fn filter_sections_u16(
    prose: u32,
    code: u32,
    has_prose_h: u32,
    has_code_h: u32,
) -> isize {
    let content = take_slot(0);
    let prose_buf = (has_prose_h != 0).then(|| take_slot(1));
    let code_buf = (has_code_h != 0).then(|| take_slot(2));
    let custom_prose = prose_buf.as_deref().map(split_nul);
    let custom_code = code_buf.as_deref().map(split_nul);
    let res = filter_sections(
        &content,
        prose != 0,
        code != 0,
        custom_prose.as_deref(),
        custom_code.as_deref(),
    );
    return_slot(0, content);
    if let Some(b) = prose_buf {
        return_slot(1, b);
    }
    if let Some(b) = code_buf {
        return_slot(2, b);
    }
    store_opt_result(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn is_old_statusline_u16() -> u32 {
    let cmd = take_slot(0);
    let out = u32::from(is_old_statusline_command(&cmd));
    return_slot(0, cmd);
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn is_owned_statusline_u16() -> u32 {
    let cmd = take_slot(0);
    let out = u32::from(is_owned_statusline_command(&cmd));
    return_slot(0, cmd);
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn unsafe_tool_name_u16() -> u32 {
    let name = take_slot(0);
    let out = u32::from(unsafe_tool_name(&name));
    return_slot(0, name);
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn contains_credential_u16() -> u32 {
    let text = take_slot(0);
    let out = u32::from(contains_credential(&text));
    return_slot(0, text);
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn salvage_line_u16() -> isize {
    let line = take_slot(0);
    let res = salvage_line(&line);
    return_slot(0, line);
    store_opt_result(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn minify_json_u16() -> isize {
    let text = take_slot(0);
    let res = minify_json(&text);
    return_slot(0, text);
    store_opt_result(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn scrub_u16() -> isize {
    let text = take_slot(0);
    let res = scrub(&text);
    return_slot(0, text);
    store_result(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn duplicate_marker_u16(is_earlier: u32) -> isize {
    let tool = take_slot(0);
    let text = take_slot(1);
    let res = duplicate_marker(&tool, &text, is_earlier != 0);
    return_slot(0, tool);
    return_slot(1, text);
    store_result(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn compress_u16(
    has_spill: u32,
    max_chars: u32,
    head_lines: u32,
    tail_lines: u32,
) -> isize {
    let text = take_slot(0);
    let spill = (has_spill != 0).then(|| take_slot(1));
    let res = compress(
        &text,
        max_chars as usize,
        head_lines as usize,
        tail_lines as usize,
        spill.as_deref(),
    );
    return_slot(0, text);
    if let Some(b) = spill {
        return_slot(1, b);
    }
    store_opt_result(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn bound_for_opencode_u16(has_spill: u32, max_chars: u32) -> isize {
    let text = take_slot(0);
    let spill = (has_spill != 0).then(|| take_slot(1));
    let res = bound_for_opencode(&text, spill.as_deref(), max_chars as usize);
    return_slot(0, text);
    if let Some(b) = spill {
        return_slot(1, b);
    }
    store_opt_result(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn env_assign_end_u16(start: u32) -> u32 {
    let text = take_slot(0);
    let out = env_assign_end(&text, start as usize) as u32;
    return_slot(0, text);
    out
}

/// Returns `-1` when no kinds match, else comma-separated kind names in `Set` insertion order.
#[unsafe(no_mangle)]
pub extern "C" fn kinds_for_u16() -> isize {
    let cmd = take_slot(0);
    let res = kinds_for_csv(&cmd);
    return_slot(0, cmd);
    store_opt_result(res)
}

#[unsafe(no_mangle)]
pub extern "C" fn wrappable_u16() -> u32 {
    let seg = take_slot(0);
    let out = u32::from(wrappable(&seg));
    return_slot(0, seg);
    out
}

#[unsafe(no_mangle)]
pub extern "C" fn filter_command_u16(has_me: u32) -> isize {
    let cmd = take_slot(0);
    let text = take_slot(1);
    let me = (has_me != 0).then(|| take_slot(2));
    let res = filter_command(&cmd, &text, me.as_deref());
    return_slot(0, cmd);
    return_slot(1, text);
    if let Some(b) = me {
        return_slot(2, b);
    }
    store_opt_result(res)
}
