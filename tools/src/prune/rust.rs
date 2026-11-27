// SPDX-License-Identifier: Apache-2.0

//! Rust items the compiler reports dead in every build prune runs, with each
//! item's full extent taken from a parsed syntax tree.

use super::edit::Range;
use super::{Cut, safe_path};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use syn::spanned::Spanned;

/// A `dead_code` primary span: file as rustc names it, byte range.
type Span = (String, usize, usize);

/// The spans dead in every build, each file's package folder, and whether
/// the all-features builds ran.
type Builds = (BTreeSet<Span>, BTreeMap<String, PathBuf>, bool);

/// What the finder found and anything the user should know.
#[derive(Default)]
pub struct Found {
    pub cuts: Vec<Cut>,
    pub notes: Vec<String>,
}

fn cargo(
    root: &Path,
    env: &HashMap<String, String>,
    args: &[&str],
) -> Result<(bool, String), String> {
    let out = Command::new("cargo")
        .args(args)
        .current_dir(root)
        .env_clear()
        .envs(env)
        .output()
        .map_err(super::cannot("cargo"))?;
    Ok((
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    ))
}

/// The `dead_code` spans one build reports in lib and bin targets, and the
/// package folder of each file.
fn dead_spans(json: &str, packages: &mut BTreeMap<String, PathBuf>) -> BTreeSet<Span> {
    let mut spans = BTreeSet::new();
    for line in json.lines() {
        let Ok(v) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let kinds = v["target"]["kind"]
            .as_array()
            .map_or(&[][..], Vec::as_slice);
        let code = &v["message"]["code"]["code"];
        if v["reason"] != "compiler-message"
            || code != "dead_code"
            || kinds.iter().any(|k| k == "custom-build")
        {
            continue;
        }
        let dir = v["manifest_path"]
            .as_str()
            .map(|m| Path::new(m).parent().unwrap_or(Path::new("")).to_path_buf());
        for s in v["message"]["spans"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|s| s["is_primary"] == true)
        {
            let (Some(file), Some(a), Some(b)) = (
                s["file_name"].as_str(),
                s["byte_start"].as_u64(),
                s["byte_end"].as_u64(),
            ) else {
                continue;
            };
            if let Some(d) = &dir {
                packages.insert(file.to_string(), d.clone());
            }
            spans.insert((file.to_string(), a as usize, b as usize));
        }
    }
    spans
}

/// Spans reported dead in the production and `cfg(test)` builds, with the
/// default features and with all features. Returns the spans, each file's
/// package folder and whether the all-features builds ran.
fn builds(root: &Path, env: &HashMap<String, String>) -> Result<Builds, String> {
    let mut packages = BTreeMap::new();
    let mut all: Option<BTreeSet<Span>> = None;
    let mut all_features = true;
    for features in [false, true] {
        for test in [false, true] {
            let mut args = vec!["check", "--workspace", "--message-format=json", "--quiet"];
            args.extend(test.then_some(["--profile", "test"]).into_iter().flatten());
            args.extend(features.then_some("--all-features"));
            let (ok, json) = cargo(root, env, &args)?;
            if !ok && !features {
                return Err(format!(
                    "`cargo {}` fails; prune needs a crate that builds",
                    args.join(" ")
                ));
            }
            all_features &= ok || !features;
            if ok {
                let spans = dead_spans(&json, &mut packages);
                all =
                    Some(all.map_or(spans.clone(), |a| a.intersection(&spans).cloned().collect()));
            }
        }
    }
    Ok((all.unwrap_or_default(), packages, all_features))
}

/// An item prune may cut: its name's span, its full range, and the ranges cut with it.
struct Item {
    name: String,
    kind: &'static str,
    name_span: Range,
    ranges: Vec<Range>,
    risky: Option<String>,
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
    let tr = i
        .trait_
        .as_ref()
        .and_then(|(p, _)| p.segments.last())
        .map(|s| s.ident.to_string());
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
fn collect(items: &[syn::Item], out: &mut Vec<Item>) {
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
fn blank_prelude(text: &str) -> String {
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

/// Whether `text` has a `cfg` whose predicate names anything but `test` and
/// features: code this build does not see may use an item it calls dead.
fn platform_cfg(text: &str) -> bool {
    text.match_indices("cfg").any(|(i, _)| {
        let rest = &text[i + 3..];
        let attr = rest.starts_with("_attr");
        let rest = rest
            .strip_prefix("_attr")
            .or_else(|| rest.strip_prefix('!'))
            .unwrap_or(rest)
            .trim_start();
        let Some(body) = rest.strip_prefix('(') else {
            return false;
        };
        let mut depth = 1;
        let end = body
            .char_indices()
            .find(|&(_, c)| {
                depth += i32::from(c == '(') - i32::from(c == ')');
                depth == 0 || attr && depth == 1 && c == ','
            })
            .map_or(body.len(), |(j, _)| j);
        let pred: String = body[..end]
            .split('"')
            .step_by(2)
            .flat_map(str::chars)
            .map(|c| {
                if c.is_alphanumeric() || c == '_' {
                    c
                } else {
                    ' '
                }
            })
            .collect();
        pred.split_whitespace()
            .any(|w| !["test", "feature", "all", "any", "not"].contains(&w))
    })
}

/// Whether any Rust source in the package folder `dir` has a platform cfg.
fn package_has_platform_cfg(dir: &Path) -> bool {
    std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| {
            let p = e.path();
            let name = e.file_name().to_string_lossy().into_owned();
            let kind = e.file_type().ok();
            if kind.is_some_and(|t| t.is_dir()) {
                return name != "target" && !name.starts_with('.') && package_has_platform_cfg(&p);
            }
            kind.is_some_and(|t| t.is_file())
                && name.ends_with(".rs")
                && std::fs::read_to_string(&p).is_ok_and(|s| platform_cfg(&s))
        })
}

/// The workspace root that rustc's file names are relative to.
fn workspace(root: &Path, env: &HashMap<String, String>) -> Result<PathBuf, String> {
    let (ok, out) = cargo(
        root,
        env,
        &["locate-project", "--workspace", "--message-format", "plain"],
    )?;
    let manifest = PathBuf::from(out.trim());
    match manifest.parent() {
        Some(p) if ok => Ok(p.to_path_buf()),
        _ => Err("no Cargo workspace here".into()),
    }
}

/// The path of `file` (relative to `ws`) inside `root`, `/`-separated.
fn relative(root: &Path, ws: &Path, file: &str) -> Option<String> {
    let abs = ws.join(file).canonicalize().ok()?;
    let rel = abs.strip_prefix(root).ok()?;
    Some(
        rel.components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
    )
}

/// Finds the dead items in the Cargo workspace at `root`.
///
/// # Errors
/// No workspace, the default build fails, or a reported file cannot be read or parsed.
pub fn find(root: &Path, env: &HashMap<String, String>) -> Result<Found, String> {
    let ws = workspace(root, env)?;
    let (spans, packages, all_features) = builds(root, env)?;
    let mut found = Found::default();
    if !all_features {
        found.notes.push("the --all-features build fails, so code behind features was not seen: every candidate is risky".into());
    }
    let mut by_file: BTreeMap<&str, Vec<Range>> = BTreeMap::new();
    for (file, a, b) in &spans {
        by_file.entry(file).or_default().push((*a, *b));
    }
    let mut platform: BTreeMap<PathBuf, bool> = BTreeMap::new();
    for (file, names) in by_file {
        let Some(rel) = relative(root, &ws, file) else {
            found
                .notes
                .push(format!("skipped {file}: outside the repository"));
            continue;
        };
        if rel
            .split('/')
            .any(|p| ["tests", "benches", "examples"].contains(&p))
        {
            continue;
        }
        let gated = packages.get(file).is_some_and(|d| {
            *platform
                .entry(d.clone())
                .or_insert_with(|| package_has_platform_cfg(d))
        });
        let why = if all_features {
            gated.then_some("package has a platform cfg")
        } else {
            Some("features not checked")
        };
        found.cuts.extend(file_cuts(root, &rel, &names, why)?);
    }
    found.cuts = super::outermost(found.cuts);
    Ok(found)
}

/// The items of the repository file `rel` whose names rustc reported dead;
/// `why` makes each risky.
fn file_cuts(
    root: &Path,
    rel: &str,
    names: &[Range],
    why: Option<&str>,
) -> Result<Vec<Cut>, String> {
    let text = std::fs::read_to_string(safe_path(root, rel)?).map_err(|e| format!("{rel}: {e}"))?;
    let tree =
        syn::parse_str::<syn::File>(&blank_prelude(&text)).map_err(|e| format!("{rel}: {e}"))?;
    let mut items = Vec::new();
    collect(&tree.items, &mut items);
    Ok(items
        .into_iter()
        .filter(|it| names.contains(&it.name_span))
        .map(|it| Cut {
            path: rel.to_string(),
            line: text[..it.name_span.0].matches('\n').count() + 1,
            risky: it.risky.or_else(|| why.map(str::to_string)),
            ranges: it.ranges,
            kind: it.kind,
            name: it.name,
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_cfgs_are_told_from_test_and_feature_ones() {
        assert!(!platform_cfg(
            "#[cfg(test)] #[cfg(feature = \"x-unix\")] cfg!(not(any(test, feature = \"y\")))"
        ));
        assert!(!platform_cfg("#[cfg_attr(test, derive(Debug))] config(a)"));
        assert!(!platform_cfg("a cfg is not a cfg("));
        assert!(platform_cfg("#[cfg(unix)]"));
        assert!(platform_cfg("if cfg!(target_os = \"linux\") {}"));
        assert!(platform_cfg("#[cfg_attr(debug_assertions, inline)]"));
        assert!(platform_cfg("#[cfg(all(test, windows)"));
    }

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
    fn spans_come_only_from_dead_code_in_lib_and_bin_targets() {
        let msg = |code: &str, kind: &str, primary: bool| {
            format!(
                r#"{{"reason":"compiler-message","manifest_path":"/w/p/Cargo.toml","target":{{"kind":["{kind}"]}},"message":{{"code":{{"code":"{code}"}},"spans":[{{"file_name":"src/lib.rs","byte_start":3,"byte_end":7,"is_primary":{primary}}}]}}}}"#
            )
        };
        let json = [
            msg("dead_code", "lib", true),
            msg("dead_code", "lib", false),
            msg("unused_imports", "lib", true),
            msg("dead_code", "custom-build", true),
            "not json".into(),
            r#"{"reason":"compiler-message","target":{"kind":["bin"]},"message":{"code":{"code":"dead_code"},"spans":[{"file_name":"src/main.rs","is_primary":true}]}}"#.into(),
            r#"{"reason":"compiler-message","target":{"kind":["bin"]},"message":{"code":{"code":"dead_code"},"spans":[{"file_name":"src/main.rs","byte_start":0,"byte_end":1,"is_primary":true}]}}"#.into(),
        ]
        .join("\n");
        let mut packages = BTreeMap::new();
        let spans = dead_spans(&json, &mut packages);
        assert_eq!(
            spans.into_iter().collect::<Vec<_>>(),
            [
                ("src/lib.rs".to_string(), 3, 7),
                ("src/main.rs".to_string(), 0, 1)
            ]
        );
        assert_eq!(
            (packages["src/lib.rs"].as_path(), packages.len()),
            (Path::new("/w/p"), 1)
        );
    }

    #[test]
    fn collects_the_items_prune_can_cut_with_their_impl_blocks() {
        let src = "#[rustfmt::skip] fn f() {} #[repr(C)] struct S; impl S { m!(); const C: u8 = 0; fn g() {} } \
                   impl Tr for S {} impl Tr for &S {} trait Tr {} impl<T> Tr for Vec<T> {} enum E {} union U { x: u8 } \
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
