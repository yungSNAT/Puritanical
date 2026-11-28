// SPDX-License-Identifier: Apache-2.0

//! Walks a crate's module tree to list the files it compiles and spot
//! attributes or macro invocations whose expansion prune cannot see.

use super::items;
use proc_macro2::{Spacing, TokenStream, TokenTree};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use syn::ext::IdentExt;
use syn::visit::Visit;

/// Standard-library macros that never expand to a hidden `#[cfg]` reference
/// to a dead item (note `cfg!` is checked separately by `uncovered_cfg`, and
/// `debug_assert!` only adds uses in the dev profile prune already builds).
const STD_MACROS: [&str; 33] = [
    "println",
    "eprintln",
    "print",
    "eprint",
    "format",
    "format_args",
    "write",
    "writeln",
    "vec",
    "assert",
    "assert_eq",
    "assert_ne",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "panic",
    "unreachable",
    "unimplemented",
    "todo",
    "dbg",
    "matches",
    "stringify",
    "concat",
    "env",
    "option_env",
    "line",
    "column",
    "file",
    "module_path",
    "cfg",
    "compile_error",
    "include_str",
    "include_bytes",
];

/// Keywords that may precede unary `!` before a group (`if !(x)`, `for ! {}`).
const NOT_MACROS: [&str; 9] = [
    "if", "while", "match", "return", "break", "yield", "in", "else", "for",
];

/// Item-introducing keywords that cannot be safely ignored inside opaque token
/// streams (a macro argument block or a `macro_rules!` body).
const OPAQUE_KEYWORDS: [&str; 4] = ["use", "extern", "mod", "macro_rules"];

fn path_segs(p: &syn::Path) -> Vec<String> {
    p.segments
        .iter()
        .map(|s| s.ident.unraw().to_string())
        .collect()
}

/// Whether a `macro_rules!` body contains `#`, `!`, `:tt` or an item keyword
/// (`use`/`extern`/`mod`/`macro_rules`), any of which could synthesize an
/// attribute, import or nested macro call (`$m!$arg`, `$m $b $a`).
fn macro_body_unsafe(stream: TokenStream) -> bool {
    let mut iter = stream.into_iter().peekable();
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Group(g) if macro_body_unsafe(g.stream()) => return true,
            TokenTree::Punct(p)
                if p.as_char() == '#'
                    || p.as_char() == '!'
                    || (p.as_char() == ':'
                        && matches!(iter.peek(), Some(TokenTree::Ident(id)) if id == "tt")) =>
            {
                return true;
            }
            TokenTree::Ident(id) if OPAQUE_KEYWORDS.contains(&id.to_string().as_str()) => {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn push_macro_path(leading: bool, path: Vec<String>, calls: &mut Vec<(bool, Vec<String>)>) {
    // Raw idents (`r#if!()`) are macro calls, not unary `!` after a keyword.
    let kw = !leading && path.len() == 1 && NOT_MACROS.contains(&path[0].as_str());
    if !kw {
        let segs = path
            .into_iter()
            .map(|s| s.strip_prefix("r#").unwrap_or(&s).to_string())
            .collect();
        calls.push((leading, segs));
    }
}

#[derive(Default)]
struct TokenPath {
    leading: bool,
    colon: bool,
    segs: Vec<String>,
}

impl TokenPath {
    fn reset(&mut self) {
        self.leading = false;
        self.colon = false;
        self.segs.clear();
    }

    fn push_ident(&mut self, raw: String) {
        if !self.colon {
            self.leading = false;
            self.segs.clear();
        }
        self.colon = false;
        self.segs.push(raw);
    }
}

/// Scans `stream` (a macro's arguments) recursively for nested `path!(...)` calls,
/// failing closed (`*unknown = true`) on `#` attributes or `use`/`extern`/`mod`/`macro_rules`.
fn collect_token_macros(
    stream: TokenStream,
    calls: &mut Vec<(bool, Vec<String>)>,
    unknown: &mut bool,
) {
    let mut p = TokenPath::default();
    let mut iter = stream.into_iter().peekable();
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Group(g) => {
                p.reset();
                collect_token_macros(g.stream(), calls, unknown);
            }
            TokenTree::Punct(ch) if ch.as_char() == '#' => {
                *unknown = true;
                return;
            }
            TokenTree::Ident(id) => {
                let raw = id.to_string();
                if OPAQUE_KEYWORDS.contains(&raw.as_str()) {
                    *unknown = true;
                    return;
                }
                p.push_ident(raw);
            }
            TokenTree::Punct(ch)
                if ch.as_char() == ':'
                    && ch.spacing() == Spacing::Joint
                    && matches!(iter.peek(), Some(TokenTree::Punct(q)) if q.as_char() == ':') =>
            {
                let _ = iter.next();
                p.leading |= p.segs.is_empty();
                p.colon = true;
            }
            TokenTree::Punct(ch)
                if ch.as_char() == '!'
                    && !p.segs.is_empty()
                    && !p.colon
                    && matches!(iter.peek(), Some(TokenTree::Group(_))) =>
            {
                push_macro_path(p.leading, std::mem::take(&mut p.segs), calls);
                p.leading = false;
            }
            _ => p.reset(),
        }
    }
}

struct CrateScan {
    dir: PathBuf,
    todo: Vec<(PathBuf, PathBuf)>,
    unknown: bool,
    ext_glob: bool,
    std_globs: BTreeSet<String>,
    shadowed: BTreeSet<String>,
    defined: BTreeSet<String>,
    clean_macros: BTreeSet<String>,
    dirty_macros: BTreeSet<String>,
    calls: Vec<(bool, Vec<String>)>,
    derives: Vec<(bool, Vec<String>)>,
    attr_macros: Vec<(bool, Vec<String>)>,
}

impl CrateScan {
    fn new(file: &Path, dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            todo: vec![(file.to_path_buf(), dir.to_path_buf())],
            unknown: false,
            ext_glob: false,
            std_globs: BTreeSet::new(),
            shadowed: BTreeSet::new(),
            defined: BTreeSet::new(),
            clean_macros: BTreeSet::new(),
            dirty_macros: BTreeSet::new(),
            calls: Vec::new(),
            derives: Vec::new(),
            attr_macros: Vec::new(),
        }
    }

    fn record_use(&mut self, tree: &syn::UseTree, prefix: &mut Vec<String>) {
        match tree {
            syn::UseTree::Path(p) => {
                prefix.push(p.ident.unraw().to_string());
                self.record_use(&p.tree, prefix);
                prefix.pop();
            }
            syn::UseTree::Group(g) => {
                for t in &g.items {
                    self.record_use(t, prefix);
                }
            }
            syn::UseTree::Name(n) => {
                let name = n.ident.unraw().to_string();
                if name == "self" {
                    self.shadowed.extend(prefix.last().cloned());
                } else {
                    self.shadowed.insert(name);
                }
            }
            syn::UseTree::Rename(r) => {
                self.shadowed.insert(r.rename.unraw().to_string());
            }
            syn::UseTree::Glob(_) => match prefix.first().map(String::as_str) {
                Some(root @ ("std" | "core" | "alloc")) => {
                    self.std_globs.insert(root.to_string());
                }
                Some(_)
                    if prefix
                        .iter()
                        .all(|s| ["self", "super", "crate"].contains(&s.as_str())) => {}
                _ => self.ext_glob = true,
            },
        }
    }

    fn has_ext_glob(&self) -> bool {
        self.ext_glob || self.std_globs.iter().any(|r| self.shadowed.contains(r))
    }

    fn safe_std_path(&self, segs: &[String], roots: &[&str], names: &[&str]) -> bool {
        matches!(
            segs,
            [root, .., name]
                if roots.contains(&root.as_str())
                    && names.contains(&name.as_str())
                    && !self.has_ext_glob()
                    && !self.shadowed.contains(root)
        )
    }

    fn safe_call(&self, leading: bool, segs: &[String]) -> bool {
        if segs.len() == 2 && self.safe_std_path(segs, &["std", "core", "alloc"], &STD_MACROS) {
            return true;
        }
        match segs {
            [name] if !leading && !self.has_ext_glob() && !self.shadowed.contains(name) => {
                let local = self.clean_macros.contains(name) && !self.dirty_macros.contains(name);
                let std = !self.defined.contains(name) && STD_MACROS.contains(&name.as_str());
                local || std
            }
            _ => false,
        }
    }

    fn safe_derive(&self, leading: bool, segs: &[String]) -> bool {
        match segs {
            [name] => {
                !leading
                    && !self.has_ext_glob()
                    && !self.shadowed.contains(name)
                    && items::STD_DERIVES.contains(&name.as_str())
            }
            _ => self.safe_std_path(segs, &["std", "core"], &items::STD_DERIVES),
        }
    }

    fn safe_attr_macro(&self, leading: bool, segs: &[String]) -> bool {
        match segs {
            [name] => {
                !leading
                    && !self.has_ext_glob()
                    && !self.shadowed.contains(name)
                    && !self.defined.contains(name)
                    && ["test", "bench"].contains(&name.as_str())
            }
            [root, _, ..] if root == "rustfmt" || root == "clippy" => {
                !leading && !self.has_ext_glob() && !self.shadowed.contains(root)
            }
            _ => self.safe_std_path(segs, &["std", "core"], &["test", "bench"]),
        }
    }
}

impl<'ast> Visit<'ast> for CrateScan {
    fn visit_attribute(&mut self, a: &'ast syn::Attribute) {
        if items::compiler_attr(a) {
            return;
        }
        let p = a.path();
        if p.is_ident("derive") {
            let Ok(ps) = a.parse_args_with(
                syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
            ) else {
                self.unknown = true;
                return;
            };
            for d in ps {
                self.derives
                    .push((d.leading_colon.is_some(), path_segs(&d)));
            }
            return;
        }
        let segs = path_segs(p);
        let prelude = segs.last().is_some_and(|n| n == "test" || n == "bench");
        if prelude || items::tool_attr(a) {
            self.attr_macros.push((p.leading_colon.is_some(), segs));
        } else {
            self.unknown = true;
        }
    }

    fn visit_item_mod(&mut self, m: &'ast syn::ItemMod) {
        // Any non-plain attribute (`#[path]`, `#[cfg_attr]`, or an attribute
        // macro on an inline or external module) hides what the module expands to.
        if m.attrs.iter().any(|a| !items::plain_attr(a)) {
            self.unknown = true;
            return;
        }
        for a in &m.attrs {
            self.visit_attribute(a);
        }
        let name = m.ident.unraw().to_string();
        self.shadowed.insert(name.clone());
        let sub = self.dir.join(name);
        if let Some((_, inner)) = &m.content {
            let prev = std::mem::replace(&mut self.dir, sub);
            for it in inner {
                self.visit_item(it);
            }
            self.dir = prev;
        } else {
            let flat = sub.with_extension("rs");
            let file = if flat.is_file() {
                flat
            } else {
                sub.join("mod.rs")
            };
            self.todo.push((file, sub));
        }
    }

    fn visit_item_macro(&mut self, m: &'ast syn::ItemMacro) {
        for a in &m.attrs {
            self.visit_attribute(a);
        }
        // Any item-level macro call (`include!`, `thread_local!`, ...) can expand to `mod x;`.
        let Some(ident) = &m.ident else {
            self.unknown = true;
            return;
        };
        let name = ident.unraw().to_string();
        self.defined.insert(name.clone());
        if m.mac.path.is_ident("macro_rules") && !macro_body_unsafe(m.mac.tokens.clone()) {
            self.clean_macros.insert(name);
        } else {
            self.dirty_macros.insert(name);
        }
    }

    fn visit_item_use(&mut self, u: &'ast syn::ItemUse) {
        for a in &u.attrs {
            self.visit_attribute(a);
        }
        self.record_use(&u.tree, &mut Vec::new());
    }

    fn visit_item_extern_crate(&mut self, e: &'ast syn::ItemExternCrate) {
        for a in &e.attrs {
            self.visit_attribute(a);
        }
        let name = e.rename.as_ref().map_or(&e.ident, |(_, i)| i);
        self.shadowed.insert(name.unraw().to_string());
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        self.calls
            .push((m.path.leading_colon.is_some(), path_segs(&m.path)));
        collect_token_macros(m.tokens.clone(), &mut self.calls, &mut self.unknown);
    }
}

/// The files the module tree of the crate rooted at `file` reaches, or `None`
/// when prune cannot follow the tree or sees a non-standard attribute/macro
/// call whose expansion it cannot inspect.
pub(super) fn tree(file: &Path) -> Option<BTreeSet<PathBuf>> {
    let mut seen = BTreeSet::new();
    let mut scan = CrateScan::new(file, file.parent()?);
    while let Some((f, dir)) = scan.todo.pop() {
        // A module file that is not there (behind a cfg for another
        // platform, say) leaves the tree unknown.
        let f = f.canonicalize().ok()?;
        if seen.insert(f.clone()) {
            let text = std::fs::read_to_string(&f).ok()?;
            let parsed = syn::parse_file(&items::blank_prelude(&text)).ok()?;
            scan.dir = dir;
            scan.visit_file(&parsed);
            proc_macro2::extra::invalidate_current_thread_spans();
            if scan.unknown {
                return None;
            }
        }
    }
    let ok = scan.calls.iter().all(|(l, s)| scan.safe_call(*l, s))
        && scan.derives.iter().all(|(l, s)| scan.safe_derive(*l, s))
        && scan
            .attr_macros
            .iter()
            .all(|(l, s)| scan.safe_attr_macro(*l, s));
    ok.then_some(seen)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ok(src: &str) -> bool {
        let d = tempfile::tempdir().expect("temp dir");
        let lib = d.path().join("lib.rs");
        std::fs::write(&lib, src).expect("lib");
        tree(&lib).is_some()
    }

    #[test]
    fn allows_std_and_clean_local_macros_and_flags_the_rest() {
        assert!(ok("macro_rules! c { ($f:ident) => { $f() }; }\n\
             use super::*;\n\
             use std::prelude::v1::*;\n\
             #[derive(Debug, core::fmt::Debug)]\n\
             struct S;\n\
             #[rustfmt::skip]\n\
             #[core::prelude::v1::test]\n\
             fn f(x: bool) { c!(g); std::println!(\"ok\"); ::core::assert!(if !(x) { true } else { false }); }\n"));
        // Custom macro call, nested macro / attr / import / raw-ident call in macro args.
        assert!(!ok("fn f() { pm::call!(); }\n"));
        assert!(!ok("fn f() { assert!(::pm::call!()); }\n"));
        assert!(!ok("fn f() { assert!({ #[pm::attr] fn h() {} true }); }\n"));
        assert!(!ok("fn f() { assert!({ use pm::x as println; true }); }\n"));
        assert!(!ok("fn f() { assert!({ r#if!(); true }); }\n"));
        // Dirty local macro_rules! (#, !, :tt, item keywords).
        assert!(!ok(
            "macro_rules! d { () => { #[allow(x)] fn g() {} }; } fn f() { d!(); }\n"
        ));
        assert!(!ok(
            "macro_rules! d { ($m:ident, $a:tt) => { $m!$a }; } fn f() { d!(x, ()); }\n"
        ));
        assert!(!ok(
            "macro_rules! d { ($m:ident, $b:tt) => { $m $b }; } fn f() { d!(x, y); }\n"
        ));
        assert!(!ok(
            "macro_rules! d { () => { use pm::x; }; } fn f() { d!(); }\n"
        ));
    }

    #[test]
    fn flags_shadowed_derives_attrs_and_globs() {
        assert!(!ok(
            "use evil::{a, b as println};\nfn f() { println!(); }\n"
        ));
        assert!(!ok("use evil::*;\nfn f() { println!(); }\n"));
        assert!(!ok("use crate::p::*;\nfn f() { println!(); }\n"));
        assert!(!ok(
            "extern crate evil as core;\nuse core::*;\nfn f() { println!(); }\n"
        ));
        assert!(!ok(
            "extern crate evil as std;\nfn f() { ::std::println!(); }\n"
        ));
        assert!(!ok("use evil::D as Debug;\n#[derive(Debug)]\nstruct S;\n"));
        assert!(!ok("#[derive(123)]\nstruct S;\n"));
        assert!(!ok("use evil::t as test;\n#[test]\nfn f() {}\n"));
        assert!(!ok("#[pm::attr]\nfn live() {}\n"));
        assert!(!ok("#[pm::attr]\nmod ext;\n"));
        assert!(!ok("#![pm::attr]\nfn live() {}\n"));
        assert!(!ok("#[allow(x)]\nuse a::b;\n#[pm::attr]\nuse a::c;\n"));
        assert!(!ok("#[pm::attr]\nextern crate core;\n"));
        assert!(!ok("#[pm::attr]\nmacro_rules! m { () => {} }\n"));
        assert!(!ok("#[::rustfmt::skip]\nfn f() {}\n"));
        assert!(!ok("mod rustfmt {}\n#[rustfmt::skip]\nfn f() {}\n"));
        assert!(!ok(
            "use dep::core::{self};\nfn f() { core::println!(); }\n"
        ));
        assert!(!ok("use dep::*;\nfn f() { std::println!(); }\n"));
    }
}
