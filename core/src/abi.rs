//! Thin UTF-16LE linear-memory ABI for `wasm32-unknown-unknown`.

use std::cell::RefCell;

use crate::compress::{
    bound_for_opencode, compress, contains_credential, duplicate_marker, minify_json, salvage_line,
    scrub, unsafe_tool_name,
};
use crate::filters::{env_assign_end, filter_command, kinds_for, wrappable};
use crate::mode::{Mode, requested_mode};
use crate::sections::{filter_sections, is_old_statusline_command, is_owned_statusline_command};

thread_local! {
    static RESULT: RefCell<Vec<u16>> = const { RefCell::new(Vec::new()) };
}

fn store_result(v: Vec<u16>) -> isize {
    let n = v.len() as isize;
    RESULT.with(|r| *r.borrow_mut() = v);
    n
}

/// # Safety
/// When `len > 0`, `ptr` must point to `len` aligned initialized `u16` elements for `'a`.
unsafe fn slice_from_raw<'a>(ptr: *const u16, len: usize) -> &'a [u16] {
    if len == 0 {
        &[]
    } else {
        // SAFETY: Enforced by caller contract on `slice_from_raw`.
        unsafe { std::slice::from_raw_parts(ptr, len) }
    }
}

/// # Safety
/// When `len > 0`, `ptr` must point to `len` aligned initialized `u16` elements for `'a`.
unsafe fn opt_slice_from_raw<'a>(ptr: *const u16, len: isize) -> Option<&'a [u16]> {
    if len >= 0 {
        // SAFETY: Forwarded from caller contract after checking `len >= 0`.
        Some(unsafe { slice_from_raw(ptr, len as usize) })
    } else {
        None
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn alloc(n: usize) -> *mut u16 {
    let mut v = Vec::<u16>::with_capacity(n.max(1));
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `p` and `n` must match one preceding `alloc(n)` call.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn dealloc(p: *mut u16, n: usize) {
    // SAFETY: Reconstructs the Vec capacity allocated by `alloc(n)`.
    drop(unsafe { Vec::from_raw_parts(p, 0, n.max(1)) });
}

#[unsafe(no_mangle)]
pub extern "C" fn result_ptr() -> *const u16 {
    RESULT.with(|r| r.borrow().as_ptr())
}

/// Returns `-1` for `null`, `0` for `'off'`, `1` for `'on'`.
///
/// # Safety
/// `tp` must point to `tl` valid `u16` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn requested_mode_u16(tp: *const u16, tl: usize) -> isize {
    // SAFETY: Caller guarantees `tp` points to `tl` initialized `u16` units.
    let prompt = unsafe { slice_from_raw(tp, tl) };
    match requested_mode(prompt) {
        None => -1,
        Some(Mode::Off) => 0,
        Some(Mode::On) => 1,
    }
}

/// Splits NUL-delimited heading lists passed as `u16` slices.
fn split_nul(buf: &[u16]) -> Vec<&[u16]> {
    if buf.is_empty() {
        return Vec::new();
    }
    buf.split(|&u| u == 0).filter(|s| !s.is_empty()).collect()
}

/// `prose_h_len < 0` or `code_h_len < 0` selects the built-in heading list.
/// Returns `-1` if `content` is unchanged.
///
/// # Safety
/// Pointer arguments with non-negative lengths must reference valid `u16` buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn filter_sections_u16(
    cp: *const u16,
    cl: usize,
    prose: u32,
    code: u32,
    php: *const u16,
    phl: isize,
    chp: *const u16,
    chl: isize,
) -> isize {
    // SAFETY: Caller guarantees each non-negative pointer/len pair is valid.
    let content = unsafe { slice_from_raw(cp, cl) };
    let custom_prose = unsafe { opt_slice_from_raw(php, phl) }.map(split_nul);
    let custom_code = unsafe { opt_slice_from_raw(chp, chl) }.map(split_nul);
    match filter_sections(
        content,
        prose != 0,
        code != 0,
        custom_prose.as_deref(),
        custom_code.as_deref(),
    ) {
        None => -1,
        Some(v) => store_result(v),
    }
}

/// # Safety
/// `tp` must point to `tl` valid `u16` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn is_old_statusline_u16(tp: *const u16, tl: usize) -> u32 {
    // SAFETY: Caller guarantees `tp` points to `tl` initialized `u16` units.
    u32::from(is_old_statusline_command(unsafe { slice_from_raw(tp, tl) }))
}

/// # Safety
/// `tp` must point to `tl` valid `u16` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn is_owned_statusline_u16(tp: *const u16, tl: usize) -> u32 {
    // SAFETY: Caller guarantees `tp` points to `tl` initialized `u16` units.
    u32::from(is_owned_statusline_command(unsafe {
        slice_from_raw(tp, tl)
    }))
}

/// # Safety
/// `tp` must point to `tl` valid `u16` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn unsafe_tool_name_u16(tp: *const u16, tl: usize) -> u32 {
    // SAFETY: Caller guarantees `tp` points to `tl` initialized `u16` units.
    u32::from(unsafe_tool_name(unsafe { slice_from_raw(tp, tl) }))
}

/// # Safety
/// `tp` must point to `tl` valid `u16` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn contains_credential_u16(tp: *const u16, tl: usize) -> u32 {
    // SAFETY: Caller guarantees `tp` points to `tl` initialized `u16` units.
    u32::from(contains_credential(unsafe { slice_from_raw(tp, tl) }))
}

/// # Safety
/// `tp` must point to `tl` valid `u16` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn salvage_line_u16(tp: *const u16, tl: usize) -> isize {
    // SAFETY: Caller guarantees `tp` points to `tl` initialized `u16` units.
    match salvage_line(unsafe { slice_from_raw(tp, tl) }) {
        None => -1,
        Some(v) => store_result(v),
    }
}

/// # Safety
/// `tp` must point to `tl` valid `u16` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn minify_json_u16(tp: *const u16, tl: usize) -> isize {
    // SAFETY: Caller guarantees `tp` points to `tl` initialized `u16` units.
    match minify_json(unsafe { slice_from_raw(tp, tl) }) {
        None => -1,
        Some(v) => store_result(v),
    }
}

/// # Safety
/// `tp` must point to `tl` valid `u16` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn scrub_u16(tp: *const u16, tl: usize) -> isize {
    // SAFETY: Caller guarantees `tp` points to `tl` initialized `u16` units.
    store_result(scrub(unsafe { slice_from_raw(tp, tl) }))
}

/// # Safety
/// `tool_p` (`tool_l`) and `tp` (`tl`) must reference valid `u16` buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn duplicate_marker_u16(
    tool_p: *const u16,
    tool_l: usize,
    tp: *const u16,
    tl: usize,
    is_earlier: u32,
) -> isize {
    // SAFETY: Caller guarantees both pointer/len pairs are valid.
    let tool = unsafe { slice_from_raw(tool_p, tool_l) };
    let text = unsafe { slice_from_raw(tp, tl) };
    store_result(duplicate_marker(tool, text, is_earlier != 0))
}

/// # Safety
/// `tp` (`tl`) and optional `sp` (`sl >= 0`) must reference valid `u16` buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn compress_u16(
    tp: *const u16,
    tl: usize,
    sp: *const u16,
    sl: isize,
    max_chars: u32,
    head_lines: u32,
    tail_lines: u32,
) -> isize {
    // SAFETY: Caller guarantees `tp` (`tl`) and optional `sp` (`sl >= 0`) are valid.
    let text = unsafe { slice_from_raw(tp, tl) };
    let spill = unsafe { opt_slice_from_raw(sp, sl) };
    match compress(
        text,
        max_chars as usize,
        head_lines as usize,
        tail_lines as usize,
        spill,
    ) {
        None => -1,
        Some(v) => store_result(v),
    }
}

/// # Safety
/// `tp` (`tl`) and optional `sp` (`sl >= 0`) must reference valid `u16` buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn bound_for_opencode_u16(
    tp: *const u16,
    tl: usize,
    sp: *const u16,
    sl: isize,
    max_chars: u32,
) -> isize {
    // SAFETY: Caller guarantees `tp` (`tl`) and optional `sp` (`sl >= 0`) are valid.
    let text = unsafe { slice_from_raw(tp, tl) };
    let spill = unsafe { opt_slice_from_raw(sp, sl) };
    match bound_for_opencode(text, spill, max_chars as usize) {
        None => -1,
        Some(v) => store_result(v),
    }
}

/// # Safety
/// `tp` must point to `tl` valid `u16` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn env_assign_end_u16(tp: *const u16, tl: usize, start: u32) -> u32 {
    // SAFETY: Caller guarantees `tp` points to `tl` initialized `u16` units.
    env_assign_end(unsafe { slice_from_raw(tp, tl) }, start as usize) as u32
}

/// # Safety
/// `tp` must point to `tl` valid `u16` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn kinds_for_u16(tp: *const u16, tl: usize) -> u32 {
    // SAFETY: Caller guarantees `tp` points to `tl` initialized `u16` units.
    u32::from(kinds_for(unsafe { slice_from_raw(tp, tl) }))
}

/// # Safety
/// `tp` must point to `tl` valid `u16` code units.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn wrappable_u16(tp: *const u16, tl: usize) -> u32 {
    // SAFETY: Caller guarantees `tp` points to `tl` initialized `u16` units.
    u32::from(wrappable(unsafe { slice_from_raw(tp, tl) }))
}

/// # Safety
/// `cp` (`cl`), `tp` (`tl`), and optional `mp` (`ml >= 0`) must reference valid `u16` buffers.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn filter_command_u16(
    cp: *const u16,
    cl: usize,
    tp: *const u16,
    tl: usize,
    mp: *const u16,
    ml: isize,
) -> isize {
    // SAFETY: Caller guarantees all pointer/len pairs with `len >= 0` are valid.
    let cmd = unsafe { slice_from_raw(cp, cl) };
    let text = unsafe { slice_from_raw(tp, tl) };
    let me = unsafe { opt_slice_from_raw(mp, ml) };
    match filter_command(cmd, text, me) {
        None => -1,
        Some(v) => store_result(v),
    }
}
