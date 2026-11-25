#![forbid(unsafe_code)]

//! Git branch remote folding.

use crate::utf16::{find_ascii, find_subslice, is_js_whitespace, push_ascii, starts_with_ascii};

fn parse_branch_ref(line: &[u16], all_remote: bool) -> Option<(&[u16], &[u16])> {
    let after_sp = if all_remote {
        starts_with_ascii(line, b"  ").then(|| &line[2..])?
    } else {
        starts_with_ascii(line, b"  remotes/").then(|| &line[10..])?
    };
    let slash = after_sp.iter().position(|&u| u == u16::from(b'/'))?;
    if slash == 0 || after_sp[..slash].iter().any(|&u| is_js_whitespace(u)) {
        return None;
    }
    let remote = &after_sp[..slash];
    let rest = &after_sp[slash + 1..];
    if rest.is_empty() {
        return None;
    }
    let valid_ref = if !rest.iter().any(|&u| is_js_whitespace(u)) {
        true
    } else if let Some(arrow) = find_ascii(rest, b" -> ") {
        let lhs = &rest[..arrow];
        let rhs = &rest[arrow + 4..];
        !lhs.is_empty()
            && !rhs.is_empty()
            && !lhs.iter().any(|&u| is_js_whitespace(u))
            && !rhs.iter().any(|&u| is_js_whitespace(u))
    } else {
        false
    };
    valid_ref.then_some((remote, rest))
}

pub(crate) fn compact_branches(lines: Vec<Vec<u16>>, all_remote: bool) -> Vec<Vec<u16>> {
    let mut out: Vec<Vec<u16>> = Vec::with_capacity(lines.len());
    let mut remotes: Vec<(Vec<u16>, Vec<Vec<u16>>)> = Vec::new();

    for l in lines {
        let Some((remote, branch_spec)) = parse_branch_ref(&l, all_remote) else {
            out.push(l);
            continue;
        };
        let mut arrow_remote = b" -> ".iter().map(|&x| u16::from(x)).collect::<Vec<u16>>();
        arrow_remote.extend_from_slice(remote);
        arrow_remote.push(u16::from(b'/'));

        let cleaned = if let Some(pos) = find_subslice(branch_spec, &arrow_remote) {
            let mut c = branch_spec[..pos].to_vec();
            push_ascii(&mut c, " -> ");
            c.extend_from_slice(&branch_spec[pos + arrow_remote.len()..]);
            c
        } else {
            branch_spec.to_vec()
        };

        if let Some((_, list)) = remotes.iter_mut().find(|(r, _)| r.as_slice() == remote) {
            list.push(cleaned);
        } else {
            out.push(remote.to_vec());
            remotes.push((remote.to_vec(), vec![cleaned]));
        }
    }

    out.into_iter()
        .map(|l| {
            if l.first() != Some(&u16::from(b' '))
                && let Some((_, list)) = remotes.iter().find(|(r, _)| r == &l)
            {
                let mut row = l;
                push_ascii(&mut row, "/: ");
                for (idx, b) in list.iter().enumerate() {
                    if idx > 0 {
                        push_ascii(&mut row, ", ");
                    }
                    row.extend_from_slice(b);
                }
                return row;
            }
            l
        })
        .collect()
}
