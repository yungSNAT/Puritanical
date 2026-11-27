// SPDX-License-Identifier: Apache-2.0

//! The items of a parsed file that prune knows how to cut, each with its
//! full byte range and the impl blocks cut with it.

use crate::prune::edit::Range;
use syn::spanned::Spanned;

/// An item prune may cut: its name's span, its full range, and the ranges cut with it.
pub(super) struct Item {
    pub(super) name: String,
    pub(super) kind: &'static str,
    pub(super) name_span: Range,
    pub(super) ranges: Vec<Range>,
    pub(super) risky: Option<String>,
}

fn range(s: proc_macro2::Span) -> Range {
    let r = s.byte_range();
    (r.start, r.end)
}

/// Attributes that neither register an item nor generate code that uses it.
fn plain_attr(a: &syn::Attribute) -> bool {
    let p = a.path();
    let tool = p.segments.len() > 1
        && p.segments
            .first()
            .is_some_and(|s| s.ident == "rustfmt" || s.ident == "clippy");
    tool || [
        "doc",
        "allow",
        "warn",
        "deny",
        "forbid",
        "expect",
        "inline",
        "cold",
        "must_use",
        "deprecated",
        "derive",
        "track_caller",
        "cfg",
    ]
    .iter()
    .any(|w| p.is_ident(w))
}

fn risky(attrs: &[syn::Attribute]) -> Option<String> {
    attrs.iter().find(|a| !plain_attr(a)).map(|a| {
        let p = a
            .path()
            .segments
            .iter()
            .map(|s| s.ident.to_string())
            .collect::<Vec<_>>()
            .join("::");
        format!("behind attribute #[{p}]")
    })
}

fn is_test_mod(attrs: &[syn::Attribute]) -> bool {
    attrs.iter().any(|a| {
        a.path().is_ident("cfg")
            && a.meta
                .require_list()
                .is_ok_and(|l| l.tokens.to_string() == "test")
    })
}

/// The bare name an impl's self type has, or for a trait impl the trait's name.
fn impl_names(i: &syn::ItemImpl) -> [Option<String>; 2] {
    let ty = match &*i.self_ty {
        syn::Type::Path(p) if p.qself.is_none() && p.path.segments.len() == 1 => {
            Some(p.path.segments[0].ident.to_string())
        }
        _ => None,
    };
    // A trait path of one segment only: `impl std::error::Error` is not an
    // impl of a local `trait Error`.
    let tr = i
        .trait_
        .as_ref()
        .filter(|(p, _)| p.segments.len() == 1)
        .map(|(p, _)| p.segments[0].ident.to_string());
    [ty, tr]
}

fn item(name: &syn::Ident, kind: &'static str, whole: Range, attrs: &[syn::Attribute]) -> Item {
    Item {
        name: name.to_string(),
        kind,
        name_span: range(name.span()),
        ranges: vec![whole],
        risky: risky(attrs),
    }
}

/// Every item in `items` (recursing into inline modules other than
/// `cfg(test)` ones) that prune knows how to cut.
pub(super) fn collect(items: &[syn::Item], out: &mut Vec<Item>) {
    let first = out.len();
    for it in items {
        let whole = range(it.span());
        let found = match it {
            syn::Item::Fn(f) => item(&f.sig.ident, "function", whole, &f.attrs),
            syn::Item::Const(c) => item(&c.ident, "constant", whole, &c.attrs),
            syn::Item::Static(s) => item(&s.ident, "static", whole, &s.attrs),
            syn::Item::Struct(s) => item(&s.ident, "struct", whole, &s.attrs),
            syn::Item::Enum(e) => item(&e.ident, "enum", whole, &e.attrs),
            syn::Item::Union(u) => item(&u.ident, "union", whole, &u.attrs),
            syn::Item::Type(t) => item(&t.ident, "type alias", whole, &t.attrs),
            syn::Item::Trait(t) => item(&t.ident, "trait", whole, &t.attrs),
            syn::Item::Mod(m) if !is_test_mod(&m.attrs) => {
                collect(m.content.as_ref().map_or(&[][..], |c| &c.1[..]), out);
                continue;
            }
            syn::Item::Impl(i) if i.trait_.is_none() => {
                for ii in &i.items {
                    let found = match ii {
                        syn::ImplItem::Fn(f) => {
                            Some(item(&f.sig.ident, "method", range(ii.span()), &f.attrs))
                        }
                        syn::ImplItem::Const(c) => Some(item(
                            &c.ident,
                            "associated constant",
                            range(ii.span()),
                            &c.attrs,
                        )),
                        _ => None,
                    };
                    out.extend(found);
                }
                continue;
            }
            _ => continue,
        };
        out.push(found);
    }
    group_impls(items, &mut out[first..]);
}

/// Adds to each type or trait in `owners` the impl blocks in `items` (its
/// module) that name it.
fn group_impls(items: &[syn::Item], owners: &mut [Item]) {
    for it in items {
        if let syn::Item::Impl(i) = it {
            let names = impl_names(i);
            for o in owners.iter_mut() {
                let owner = matches!(o.kind, "struct" | "enum" | "union")
                    && names[0].as_ref() == Some(&o.name)
                    || o.kind == "trait" && names[1].as_ref() == Some(&o.name);
                if owner {
                    o.ranges.push(range(it.span()));
                }
            }
        }
    }
}

/// `text` with a BOM or shebang line blanked to spaces, so byte offsets match rustc's.
pub(super) fn blank_prelude(text: &str) -> String {
    let mut t = text.to_string();
    if t.starts_with('\u{feff}') {
        t.replace_range(..3, "   ");
    }
    if t.starts_with("#!") && !t.starts_with("#![") {
        let end = t.find('\n').unwrap_or(t.len());
        t.replace_range(..end, &" ".repeat(end));
    }
    t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bom_or_shebang_keeps_byte_offsets() {
        assert_eq!(blank_prelude("\u{feff}fn a() {}"), "   fn a() {}");
        assert_eq!(
            blank_prelude("#!/usr/bin/env run\nfn a() {}"),
            "                  \nfn a() {}"
        );
        assert_eq!(blank_prelude("#![allow(x)]\n"), "#![allow(x)]\n");
    }

    #[test]
    fn collects_the_items_prune_can_cut_with_their_impl_blocks() {
        let src = "#[rustfmt::skip] fn f() {} #[repr(C)] struct S; impl S { m!(); const C: u8 = 0; fn g() {} } \
                   impl Tr for S {} impl Tr for &S {} trait Tr {} impl<T> Tr for Vec<T> {} impl a::Tr for u8 {} enum E {} union U { x: u8 } \
                   type A = u8; static X: u8 = 0; const K: u8 = 0; use a::b; mod decl; \
                   mod inner { fn deep() {} } #[cfg(test)] mod t { fn x() {} }";
        let mut out = Vec::new();
        collect(
            &syn::parse_str::<syn::File>(src).expect("parse").items,
            &mut out,
        );
        let got: Vec<_> = out
            .iter()
            .map(|i| (i.kind, i.name.as_str(), i.ranges.len(), i.risky.as_deref()))
            .collect();
        assert_eq!(
            got,
            [
                ("function", "f", 1, None),
                ("struct", "S", 3, Some("behind attribute #[repr]")),
                ("associated constant", "C", 1, None),
                ("method", "g", 1, None),
                ("trait", "Tr", 4, None),
                ("enum", "E", 1, None),
                ("union", "U", 1, None),
                ("type alias", "A", 1, None),
                ("static", "X", 1, None),
                ("constant", "K", 1, None),
                ("function", "deep", 1, None),
            ]
        );
    }
}
