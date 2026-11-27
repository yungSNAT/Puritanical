// SPDX-License-Identifier: Apache-2.0

//! Rust items the compiler reports dead in every build prune runs, with each
//! item's full extent taken from a parsed syntax tree.

mod items;

use super::edit::Range;
use super::{Cut, safe_path};
use items::{blank_prelude, collect};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::process::Command;
use syn::ext::IdentExt;

/// A `dead_code` primary span: file as rustc names it, byte range.
type Span = (String, usize, usize);

/// A span with the root file of the target (crate) that reported it.
type Report = (PathBuf, Span);

/// The reports made in every build, each file's package folder, and whether
/// the all-features builds ran.
type Builds = (BTreeSet<Report>, BTreeMap<String, PathBuf>, bool);

/// Target kinds `cargo check --workspace` builds, which prune searches.
const CRATE_KINDS: [&str; 7] = [
    "lib",
    "rlib",
    "dylib",
    "cdylib",
    "staticlib",
    "proc-macro",
    "bin",
];

/// What the finder found and anything the user should know.
#[derive(Default)]
pub struct Found {
    pub cuts: Vec<Cut>,
    pub notes: Vec<String>,
    /// The text of each file with cuts, as the finder parsed it.
    pub texts: BTreeMap<String, String>,
}

/// `p` made canonical where it exists.
fn canon(p: &str) -> PathBuf {
    Path::new(p)
        .canonicalize()
        .unwrap_or_else(|_| PathBuf::from(p))
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

/// The `dead_code` spans one build reports in lib and bin targets, each with
/// its target, and the package folder of each file.
fn dead_spans(json: &str, packages: &mut BTreeMap<String, PathBuf>) -> BTreeSet<Report> {
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
        let target = canon(v["target"]["src_path"].as_str().unwrap_or_default());
        let dir = v["manifest_path"]
            .as_str()
            .map(|m| canon(m).parent().unwrap_or(Path::new("")).to_path_buf());
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
            spans.insert((target.clone(), (file.to_string(), a as usize, b as usize)));
        }
    }
    spans
}

/// Spans reported dead in the production and `cfg(test)` builds, with the
/// default features and with all features. Returns the spans, each file's
/// package folder and whether the all-features builds ran.
fn builds(root: &Path, env: &HashMap<String, String>) -> Result<Builds, String> {
    let mut packages = BTreeMap::new();
    let mut all: Option<BTreeSet<Report>> = None;
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

/// `body` (the text after an open `(`) up to its closing `)`, or up to the
/// first comma at that level when `comma`.
fn group(body: &str, comma: bool) -> &str {
    let mut depth = 1;
    let end = body
        .char_indices()
        .find(|&(_, c)| {
            depth += i32::from(c == '(') - i32::from(c == ')');
            depth == 0 || comma && depth == 1 && c == ','
        })
        .map_or(body.len(), |(j, _)| j);
    &body[..end]
}

/// Whether `text` has a `cfg` prune's 4 builds do not cover: one naming
/// anything but `test` and features (code built only on another platform or
/// profile may use an item these builds call dead), or one with `not` around
/// a feature (code built only without that feature, as `no_std` code is).
fn uncovered_cfg(text: &str) -> bool {
    text.match_indices("cfg").any(|(i, _)| {
        let rest = &text[i + 3..];
        let attr = rest.starts_with("_attr");
        let rest = rest
            .strip_prefix("_attr")
            .or_else(|| rest.strip_prefix('!'))
            .unwrap_or(rest)
            .trim_start();
        rest.strip_prefix('(').is_some_and(|body| {
            // String literals (feature names) dropped; brackets kept.
            let pred: String = group(body, attr).split('"').step_by(2).collect();
            let other = pred
                .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                .any(|w| !["", "test", "feature", "all", "any", "not"].contains(&w));
            let negated = pred.match_indices("not").any(|(j, _)| {
                let after = pred[j + 3..].trim_start();
                after
                    .strip_prefix('(')
                    .is_some_and(|b| group(b, false).contains("feature"))
            });
            other || negated
        })
    })
}

/// Whether any Rust source in the package folder `dir` has a cfg prune's
/// builds do not cover.
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
                && std::fs::read_to_string(&p).is_ok_and(|s| uncovered_cfg(&s))
        })
}

/// Queues the files the `mod` declarations in `items` load (a module whose
/// child files live in `dir`). `None` when a `#[path]` module or an
/// `include!` item hides where code comes from.
fn mods(items: &[syn::Item], dir: &Path, todo: &mut Vec<(PathBuf, PathBuf)>) -> Option<()> {
    for it in items {
        match it {
            syn::Item::Mod(m) => {
                let path_attr = |a: &syn::Attribute| {
                    a.path().is_ident("path")
                        || a.meta
                            .require_list()
                            .is_ok_and(|l| l.tokens.to_string().contains("path"))
                };
                if m.attrs.iter().any(path_attr) {
                    return None;
                }
                let sub = dir.join(m.ident.unraw().to_string());
                if let Some((_, inner)) = &m.content {
                    mods(inner, &sub, todo)?;
                } else {
                    let flat = sub.with_extension("rs");
                    let file = if flat.is_file() {
                        flat
                    } else {
                        sub.join("mod.rs")
                    };
                    todo.push((file, sub));
                }
            }
            // `include!`, `std::include!` or `core::include!`.
            syn::Item::Macro(m)
                if m.mac
                    .path
                    .segments
                    .last()
                    .is_some_and(|s| s.ident == "include") =>
            {
                return None;
            }
            _ => {}
        }
    }
    Some(())
}

/// The files the module tree of the crate rooted at `file` reaches, or
/// `None` when prune cannot follow it.
fn tree(file: &Path) -> Option<BTreeSet<PathBuf>> {
    let mut seen = BTreeSet::new();
    let mut todo = vec![(file.to_path_buf(), file.parent()?.to_path_buf())];
    while let Some((f, dir)) = todo.pop() {
        // A module file that is not there (behind a cfg for another
        // platform, say) leaves the tree unknown.
        let f = f.canonicalize().ok()?;
        if seen.insert(f.clone()) {
            let text = std::fs::read_to_string(&f).ok()?;
            let items = syn::parse_file(&blank_prelude(&text)).ok()?.items;
            let queued = mods(&items, &dir, &mut todo);
            proc_macro2::extra::invalidate_current_thread_spans();
            queued?;
        }
    }
    Some(seen)
}

/// For each source file, the lib and bin targets whose module trees reach
/// it; and the package folders with a target whose tree prune cannot follow.
#[derive(Default)]
struct Owners {
    by_file: BTreeMap<PathBuf, BTreeSet<PathBuf>>,
    unknown: BTreeSet<PathBuf>,
}

fn owners(root: &Path, env: &HashMap<String, String>) -> Result<Owners, String> {
    let args = ["metadata", "--no-deps", "--format-version", "1"];
    let (_, json) = cargo(root, env, &args)?;
    let v: Value = serde_json::from_str(&json).map_err(|e| format!("cargo metadata: {e}"))?;
    let mut o = Owners::default();
    for p in v["packages"].as_array().into_iter().flatten() {
        let manifest = canon(p["manifest_path"].as_str().unwrap_or_default());
        let dir = manifest.parent().unwrap_or(Path::new("")).to_path_buf();
        for t in p["targets"].as_array().into_iter().flatten() {
            let kinds = t["kind"].as_array().map_or(&[][..], Vec::as_slice);
            if !kinds.iter().any(|k| CRATE_KINDS.iter().any(|c| k == c)) {
                continue;
            }
            let src = canon(t["src_path"].as_str().unwrap_or_default());
            let Some(files) = tree(&src) else {
                o.unknown.insert(dir.clone());
                continue;
            };
            for f in files {
                o.by_file.entry(f).or_default().insert(src.clone());
            }
        }
    }
    Ok(o)
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
    let (reports, packages, all_features) = builds(root, env)?;
    let owners = owners(root, env)?;
    let mut found = Found::default();
    if !all_features {
        found.notes.push("the --all-features build fails, so code behind features was not seen: every candidate is risky".into());
    }
    let mut reporters: BTreeMap<&Span, BTreeSet<&PathBuf>> = BTreeMap::new();
    for (target, span) in &reports {
        reporters.entry(span).or_default().insert(target);
    }
    let mut by_file: BTreeMap<&str, Vec<Range>> = BTreeMap::new();
    for ((file, a, b), who) in reporters {
        // Liveness is per crate: a file 2 crates compile is cut only where
        // every crate whose module tree reaches it reports the item dead.
        let need = ws.join(file).canonicalize().ok();
        let need = need.and_then(|f| owners.by_file.get(&f));
        if need.is_none_or(|n| n.iter().all(|t| who.contains(t))) {
            by_file.entry(file).or_default().push((*a, *b));
        }
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
        let dir = packages.get(file);
        let unknown = dir.is_some_and(|d| owners.unknown.contains(d));
        let gated = dir.is_some_and(|d| {
            *platform
                .entry(d.clone())
                .or_insert_with(|| package_has_platform_cfg(d))
        });
        let why = (!all_features)
            .then_some("features not checked")
            .or(unknown.then_some("package has a #[path] module or an include! item"))
            .or(gated.then_some("package has a cfg its builds do not cover"));
        let (cuts, text) = file_cuts(root, &rel, &names, why)?;
        found.cuts.extend(cuts);
        found.texts.insert(rel, text);
    }
    found.cuts = super::outermost(found.cuts);
    Ok(found)
}

/// The items of the repository file `rel` whose names rustc reported dead,
/// and the text they were found in; `why` makes each risky.
fn file_cuts(
    root: &Path,
    rel: &str,
    names: &[Range],
    why: Option<&str>,
) -> Result<(Vec<Cut>, String), String> {
    let text = std::fs::read_to_string(safe_path(root, rel)?).map_err(|e| format!("{rel}: {e}"))?;
    let tree =
        syn::parse_str::<syn::File>(&blank_prelude(&text)).map_err(|e| format!("{rel}: {e}"))?;
    let mut items = Vec::new();
    collect(&tree.items, &mut items);
    proc_macro2::extra::invalidate_current_thread_spans();
    let cuts = items
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
        .collect();
    Ok((cuts, text))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cfgs_the_builds_cover_are_told_from_the_rest() {
        assert!(!uncovered_cfg(
            "#[cfg(test)] #[cfg(feature = \"x-unix\")] cfg!(all(not(test), any(test, feature = \"y\")))"
        ));
        assert!(!uncovered_cfg("#[cfg_attr(test, derive(Debug))] config(a)"));
        assert!(!uncovered_cfg("a cfg is not a cfg("));
        assert!(!uncovered_cfg("#[cfg(feature = \"not(feature)\")]"));
        assert!(uncovered_cfg("#[cfg(unix)]"));
        assert!(uncovered_cfg("if cfg!(target_os = \"linux\") {}"));
        assert!(uncovered_cfg("#[cfg_attr(debug_assertions, inline)]"));
        assert!(uncovered_cfg("#[cfg(all(test, windows)"));
        // `not` around a feature: code only a build without it compiles.
        assert!(uncovered_cfg("#[cfg(not(feature = \"std\"))]"));
        assert!(uncovered_cfg("#[cfg_attr(not(feature = \"a\"), no_std)]"));
        assert!(uncovered_cfg("cfg!(not (any(test, feature = \"y\")))"));
    }

    #[cfg(unix)]
    #[test]
    fn a_module_tree_lists_each_file_once() {
        let d = tempfile::tempdir().expect("temp dir");
        let at = |p: &str| d.path().join(p);
        std::fs::write(at("lib.rs"), "mod a;\nmod b;\nmod inner {\n    mod c;\n}\n").expect("lib");
        std::fs::write(at("a.rs"), "fn x() {}\n").expect("a");
        // `b` loads the same file as `a`, through a symbolic link.
        std::os::unix::fs::symlink("a.rs", at("b.rs")).expect("symlink");
        std::fs::create_dir(at("inner")).expect("dir");
        std::fs::write(at("inner/c.rs"), "").expect("c");
        let files = tree(&at("lib.rs")).expect("tree");
        let names: Vec<_> = files
            .iter()
            .map(|f| f.file_name().expect("name").to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["a.rs", "c.rs", "lib.rs"]);
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
            spans.into_iter().map(|(_, s)| s).collect::<Vec<_>>(),
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
}
