#![forbid(unsafe_code)]

//! Port of `hooks/puritanical-command-filters.js`.

mod classify;
mod git_branch;
mod git_diff;
mod git_log;
mod git_status;
mod ls_wc_grep;
mod progress;
mod shell;
mod tests_filter;

pub use classify::{
    KIND_GIT_BRANCH, KIND_GIT_DIFF, KIND_GIT_LOG, KIND_GIT_STATUS, KIND_GREP, KIND_LS,
    KIND_PROGRESS, KIND_TEST, KIND_WC, kinds_for, kinds_for_csv, wrappable,
};
pub use shell::env_assign_end;

use crate::compress::strip_ansi;
use crate::utf16::{join_u16, split_lf, trim};
use classify::{Classified, classify, cmd_has_flag};
use git_branch::compact_branches;
use git_diff::{compact_diff, compact_stat};
use git_log::compact_git_log;
use git_status::short_status;
use ls_wc_grep::{
    collapse_blank_and_trim_newlines, compact_ls, compact_wc, group_grep, trim_newlines,
};
use progress::should_drop_line;
use shell::{split_segments, strip_quotes};
use tests_filter::{compact_cargo, drop_tap_passes, go_verbose};

pub(crate) fn is_ascii_digit(u: u16) -> bool {
    matches!(u, 0x0030..=0x0039)
}

pub(crate) fn is_ascii_alpha(u: u16) -> bool {
    matches!(u, 0x0041..=0x005A | 0x0061..=0x007A)
}

pub(crate) fn is_ascii_hex_lower(u: u16) -> bool {
    matches!(u, 0x0030..=0x0039 | 0x0061..=0x0066)
}

fn normalise_cr_lines(clean: &[u16]) -> Vec<Vec<u16>> {
    split_lf(clean)
        .into_iter()
        .map(|l| {
            let t = l.strip_suffix(&[0x000D]).unwrap_or(l);
            if t.contains(&0x000D) {
                t.split(|&u| u == 0x000D)
                    .rfind(|p| !p.is_empty())
                    .unwrap_or(&[])
                    .to_vec()
            } else {
                t.to_vec()
            }
        })
        .collect()
}

fn apply_filters(
    kinds: u16,
    mut lines: Vec<Vec<u16>>,
    all_remotes: bool,
    keep_date: bool,
    me: Option<&[u16]>,
) -> Vec<Vec<u16>> {
    if kinds & KIND_TEST != 0 {
        lines = go_verbose(drop_tap_passes(lines));
    }
    lines.retain(|l| !should_drop_line(kinds, l));
    if kinds & KIND_TEST != 0 {
        lines = compact_cargo(lines);
    }
    if kinds & KIND_GIT_STATUS != 0 {
        lines = short_status(lines);
    }
    if kinds & KIND_GIT_DIFF != 0 {
        lines = compact_stat(compact_diff(lines));
    }
    if kinds & KIND_GIT_LOG != 0 {
        lines = compact_git_log(lines);
    }
    if kinds & KIND_GIT_BRANCH != 0 {
        lines = compact_branches(lines, all_remotes);
    }
    if kinds & KIND_LS != 0 {
        lines = compact_ls(lines, keep_date, me);
    }
    if kinds == KIND_WC {
        lines = compact_wc(lines);
    }
    if kinds == KIND_GREP {
        lines = group_grep(lines);
    }
    lines
}

#[must_use]
pub fn filter_command(command: &[u16], text: &[u16], me: Option<&[u16]>) -> Option<Vec<u16>> {
    if text.is_empty() {
        return None;
    }
    let kinds = kinds_for(command);
    if kinds == 0 {
        return None;
    }
    let stripped = strip_quotes(command);
    let segs: Vec<Classified> = split_segments(&stripped)
        .into_iter()
        .filter_map(|s| {
            let t = trim(s);
            (!t.is_empty()).then(|| classify(t)).flatten()
        })
        .collect();
    let lines = normalise_cr_lines(&strip_ansi(text));
    let filtered = apply_filters(
        kinds,
        lines,
        cmd_has_flag(&segs, b"r") || cmd_has_flag(&segs, b"--remotes"),
        cmd_has_flag(&segs, b"t"),
        me,
    );
    let refs: Vec<&[u16]> = filtered.iter().map(Vec::as_slice).collect();
    let out = collapse_blank_and_trim_newlines(&join_u16(&refs, 0x000A));
    (out.len() < trim_newlines(text).len()).then_some(out)
}

#[cfg(test)]
mod tests {
    use super::{
        KIND_GIT_STATUS, env_assign_end, filter_command, kinds_for, kinds_for_csv, wrappable,
    };
    use crate::utf16::{str_from_u16_lossy, u16_from_str};

    #[test]
    fn basic_filter_checks() {
        assert_eq!(env_assign_end(&u16_from_str("FOO=1 BAR=\"a\" cmd"), 0), 14);
        assert_eq!(kinds_for(&u16_from_str("git status")), KIND_GIT_STATUS);
        assert_eq!(
            str_from_u16_lossy(&kinds_for_csv(&u16_from_str("cargo test && git status")).unwrap()),
            "test,progress,git-status"
        );
        assert!(wrappable(&u16_from_str("cargo test")));
        assert!(!wrappable(&u16_from_str("git push")));
        let out = filter_command(
            &u16_from_str("git status"),
            &u16_from_str("On branch main\nChanges not staged for commit:\n\tmodified:   x.js\n"),
            None,
        )
        .unwrap();
        assert_eq!(str_from_u16_lossy(&out), "* main\n M x.js");
        let pip_out = filter_command(
            &u16_from_str("pip install x"),
            &u16_from_str("   \u{2501}\u{2501}\u{2501} 10.2/10.2 MB 5.1 MB/s\nInstalled x\n"),
            None,
        )
        .unwrap();
        assert_eq!(str_from_u16_lossy(&pip_out), "Installed x");
        let cargo_out = filter_command(
            &u16_from_str("cargo test"),
            &u16_from_str(
                "test result: ok. 1 passed; 0 failed; finished in 0.01s; finished in 0.02s\n",
            ),
            None,
        )
        .unwrap();
        assert!(str_from_u16_lossy(&cargo_out).ends_with("(0.02s)"));
        let ls_out = filter_command(
            &u16_from_str("ls -lt"),
            &u16_from_str(
                "drwxr-xr-x 2 alice staff 64 Jan 5 10:20 dir\ndrw-r--r-- 1 bob staff 420 2026-03-01 12:00:00 file.txt\ndrwsr-sr-t+ 2 alice wheel 64 2026-01-01 ..\n",
            ),
            None,
        )
        .unwrap();
        assert!(!str_from_u16_lossy(&ls_out).contains(".."));
    }
}
