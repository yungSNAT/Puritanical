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

/// Whether `stream` contains an attribute `#` or a macro call `! (...)` at
/// any nesting depth. A local `macro_rules!` without either cannot hide a
/// `cfg` attribute or call an unseen macro.
fn tokens_have_attr_or_macro(stream: TokenStream) -> bool {
    let mut iter = stream.into_iter().peekable();
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Group(g) if tokens_have_attr_or_macro(g.stream()) => return true,
            TokenTree::Punct(p)
                if p.as_char() == '#'
                    || (p.as_char() == '!' && matches!(iter.peek(), Some(TokenTree::Group(_)))) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

fn push_macro_path(leading: bool, path: Vec<String>, calls: &mut Vec<(bool, Vec<String>)>) {
    let kw = !leading && path.len() == 1 && NOT_MACROS.contains(&path[0].as_str());
    if !kw {
        calls.push((leading, path));
    }
}

/// Scans `stream` (a macro's arguments) recursively for nested `path!(...)` calls.
fn collect_token_macros(stream: TokenStream, calls: &mut Vec<(bool, Vec<String>)>) {
    let (mut leading, mut colon, mut path) = (false, false, Vec::new());
    let mut iter = stream.into_iter().peekable();
    while let Some(tt) = iter.next() {
        match tt {
            TokenTree::Group(g) => {
                (leading, colon) = (false, false);
                path.clear();
                collect_token_macros(g.stream(), calls);
            }
            TokenTree::Ident(id) => {
                if !colon {
                    leading = false;
                    path.clear();
                }
                colon = false;
                path.push(id.unraw().to_string());
            }
            TokenTree::Punct(p)
                if p.as_char() == ':'
                    && p.spacing() == Spacing::Joint
                    && matches!(iter.peek(), Some(TokenTree::Punct(q)) if q.as_char() == ':') =>
            {
                let _ = iter.next();
                leading |= path.is_empty();
                colon = true;
            }
            TokenTree::Punct(p)
                if p.as_char() == '!'
                    && !path.is_empty()
                    && !colon
                    && matches!(iter.peek(), Some(TokenTree::Group(_))) =>
            {
                push_macro_path(leading, std::mem::take(&mut path), calls);
                leading = false;
            }
            _ => {
                (leading, colon) = (false, false);
                path.clear();
            }
        }
    }
}

struct CrateScan {
    dir: PathBuf,
    todo: Vec<(PathBuf, PathBuf)>,
    unknown: bool,
    ext_glob: bool,
    shadowed: BTreeSet<String>,
    defined: BTreeSet<String>,
    clean_macros: BTreeSet<String>,
    dirty_macros: BTreeSet<String>,
    calls: Vec<(bool, Vec<String>)>,
}

impl CrateScan {
    fn new(file: &Path, dir: &Path) -> Self {
        Self {
            dir: dir.to_path_buf(),
            todo: vec![(file.to_path_buf(), dir.to_path_buf())],
            unknown: false,
            ext_glob: false,
            shadowed: BTreeSet::new(),
            defined: BTreeSet::new(),
            clean_macros: BTreeSet::new(),
            dirty_macros: BTreeSet::new(),
            calls: Vec::new(),
        }
    }

    fn record_use(&mut self, tree: &syn::UseTree, root: Option<&str>) {
        match tree {
            syn::UseTree::Path(p) => {
                let name = p.ident.unraw().to_string();
                self.record_use(&p.tree, Some(root.unwrap_or(name.as_str())));
            }
            syn::UseTree::Group(g) => {
                for t in &g.items {
                    self.record_use(t, root);
                }
            }
            syn::UseTree::Name(n) => {
                self.shadowed.insert(n.ident.unraw().to_string());
            }
            syn::UseTree::Rename(r) => {
                self.shadowed.insert(r.rename.unraw().to_string());
            }
            syn::UseTree::Glob(_) => {
                let local = root.is_some_and(|r| {
                    ["self", "super", "crate", "std", "core", "alloc"].contains(&r)
                });
                if !local {
                    self.ext_glob = true;
                }
            }
        }
    }

    fn safe_call(&self, leading: bool, segs: &[String]) -> bool {
        match segs {
            [root, name]
                if ["std", "core", "alloc"].contains(&root.as_str())
                    && STD_MACROS.contains(&name.as_str()) =>
            {
                leading || !self.shadowed.contains(root)
            }
            [name] if !leading && !self.ext_glob && !self.shadowed.contains(name) => {
                let local = self.clean_macros.contains(name) && !self.dirty_macros.contains(name);
                let std = !self.defined.contains(name) && STD_MACROS.contains(&name.as_str());
                local || std
            }
            _ => false,
        }
    }
}

impl<'ast> Visit<'ast> for CrateScan {
    fn visit_attribute(&mut self, a: &'ast syn::Attribute) {
        if !items::builtin_attr(a) {
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
        if m.mac.path.is_ident("macro_rules") && !tokens_have_attr_or_macro(m.mac.tokens.clone()) {
            self.clean_macros.insert(name);
        } else {
            self.dirty_macros.insert(name);
        }
    }

    fn visit_item_use(&mut self, u: &'ast syn::ItemUse) {
        for a in &u.attrs {
            self.visit_attribute(a);
        }
        self.record_use(&u.tree, None);
    }

    fn visit_item_extern_crate(&mut self, e: &'ast syn::ItemExternCrate) {
        for a in &e.attrs {
            self.visit_attribute(a);
        }
        let name = e.rename.as_ref().map_or(&e.ident, |(_, i)| i);
        self.shadowed.insert(name.unraw().to_string());
    }

    fn visit_macro(&mut self, m: &'ast syn::Macro) {
        let segs = m
            .path
            .segments
            .iter()
            .map(|s| s.ident.unraw().to_string())
            .collect();
        self.calls.push((m.path.leading_colon.is_some(), segs));
        collect_token_macros(m.tokens.clone(), &mut self.calls);
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
    scan.calls
        .iter()
        .all(|(leading, segs)| scan.safe_call(*leading, segs))
        .then_some(seen)
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
             fn f(x: bool) { c!(g); std::println!(\"ok\"); ::core::assert!(if !(x) { true } else { false }); }\n"));
        // Custom macro call, nested macro in std macro args, dirty local macro_rules!.
        assert!(!ok("fn f() { pm::call!(); }\n"));
        assert!(!ok("fn f() { assert!(::pm::call!()); }\n"));
        assert!(!ok(
            "macro_rules! d { () => { #[allow(x)] fn g() {} }; } fn f() { d!(); }\n"
        ));
        assert!(!ok(
            "macro_rules! d { () => { pm::x!(); }; } fn f() { d!(); }\n"
        ));
        // Shadowed std macros and external globs.
        assert!(!ok(
            "use evil::{a, b as println};\nfn f() { println!(); }\n"
        ));
        assert!(!ok("use evil::*;\nfn f() { println!(); }\n"));
        assert!(!ok(
            "extern crate evil as std;\nfn f() { std::println!(); }\n"
        ));
        // Attribute macros anywhere in the crate (inline item, external mod, inner attr).
        assert!(!ok("#[pm::attr]\nfn live() {}\n"));
        assert!(!ok("#[pm::attr]\nmod ext;\n"));
        assert!(!ok("#![pm::attr]\nfn live() {}\n"));
        assert!(!ok("#[allow(x)]\nuse a::b;\n#[pm::attr]\nuse a::c;\n"));
        assert!(!ok("#[pm::attr]\nextern crate core;\n"));
        assert!(!ok("#[pm::attr]\nmacro_rules! m { () => {} }\n"));
    }
}
