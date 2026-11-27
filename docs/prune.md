# Prune: remove dead code with proof

`puritanical-tools prune` removes the dead code a compiler proves dead from an existing repository. It keeps each cut only if your own check still passes, and it can put every byte back.

```sh
cargo install --locked --path tools          # once, from a clone of this repository; puts puritanical-tools on PATH
puritanical-tools prune --repo <repo>        # dry run: lists what it would cut, changes nothing
puritanical-tools prune --repo <repo> --apply -- cargo test
puritanical-tools prune --repo <repo> undo   # put every changed file back, byte for byte
puritanical-tools prune --repo <repo> keep   # accept the cuts and drop the journal
```

`--repo` defaults to the current folder, which must be the top of a Git work tree; `undo` and `keep` may come before or after it. The check after `--` runs as given, without a shell; for a pipeline pass `sh -c '...'` (on Windows, `pwsh -c '...'` or Git's `sh`).

## What it finds

Rust only, in a Cargo workspace at the repository root: functions, constants, statics, structs, enums, unions, type aliases, traits, and methods and associated constants of inherent `impl` blocks that rustc reports as `dead_code`. A struct, enum, union or trait goes together with the `impl` blocks in the same module that name it.

An item is a candidate only when rustc reports it dead in all 4 builds prune runs: `cargo check --workspace` and `cargo check --workspace --profile test`, each with the default features and with `--all-features`. Liveness is per crate, so a file that 2 crates compile (a library and a binary that both declare `mod util;`) gives a candidate only where every crate whose module tree reaches the file reports it dead. So:

- an item used only by unit tests, or only under a feature that adds code, is not a candidate;
- public items of a library, `#[no_mangle]`, `#[export_name]` and `#[used]` items, and items under `#[allow(dead_code)]` or `#[expect(dead_code)]` are never candidates, because rustc does not report them;
- code reached through a macro, a trait or a re-export is seen, because rustc resolves names rather than counting them.

Each item's extent, with its attributes and doc comments, comes from a parse of the file with `syn`, matched to rustc's span of the item's name. Whole lines go, and a blank line goes with them where 2 would otherwise be left together or one would be left at the edge of a block or the file.

## What it does not find

Unused imports, macros, enum variants and struct fields; dead files and modules; unused dependencies; non-item comments next to a dead item (they stay); and every language other than Rust. Files under a `tests`, `benches` or `examples` folder and `#[cfg(test)]` modules are never cut.

## Risky candidates

These are listed but cut only with `--risky`:

- an item behind an attribute other than `doc`, lint levels, `inline`, `cold`, `must_use`, `deprecated`, a `derive` of standard traits only (`Debug`, `Clone`, `Copy`, `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Hash`, `Default`), `track_caller`, `cfg`, `rustfmt::*` or `clippy::*` (for example `#[repr(C)]`), or inside an inline module or impl block behind such an attribute;
- every item of a package whose sources hold a `cfg` naming anything but `test` and features (such as `unix`, `target_os` or `debug_assertions`), because code compiled only on another platform or profile may use an item this build calls dead;
- every item of a package with `not` around a feature in a `cfg` (such as `#[cfg(not(feature = "std"))]` for `no_std` code), because a build without that feature compiles code the 4 builds never see;
- every item of a package with a `#[path]` or attributed module (inline or external), a non-built-in attribute or custom derive anywhere in the crate, an item-level macro call, or a function-body or nested macro call other than an unshadowed standard-library macro or clean local `macro_rules!`, because external or procedural macros can load modules or generate `cfg`-gated references prune cannot see;
- every item of a package with a comment inside a `cfg`, which the scan above does not read through;
- every item in a file Git does not track (an ignored, generated file), because Git holds no copy of it;
- every item when the `--all-features` build fails, because code behind features was not seen.

If you cut risky items, put the other platforms' and profiles' builds in your check.

## How a cut is proved

- Prune refuses to start when the tree has any change or untracked file, when a journal from an earlier run exists, when the finder's build changes the tree, when the baseline check fails, changes tracked or non-ignored files, or damages the journal (any files the baseline check changed are named and recorded in an incomplete journal so `prune undo` puts them back, and a failing check shows the end of its output), when the check prints no libtest count above 0 (pass `--uncounted` to rely on the exit status alone), or when a file to be cut changed after the finder read it.
- It sums the passed counts of libtest's summary lines (`test result: ... N passed`): a cut stays only if the check exits 0 and the count is there and not lower than at the start.
- All candidates are tried at once. A failing batch is split in halves until each failing cut stands alone; a cut rejected only because another dead item still used it is tried again after a pass that kept something.
- Each check run may take 3 times the first run's time plus 60 seconds; a run past that is killed, with every process it started (on Unix the check leads its own process group, so Ctrl-C stops prune but not a check already running), and counts as a failure. When a run ends, on Unix prune also kills whatever the check left running in its process group, so a background job cannot change files after prune has looked; on Windows (where prune uses no `unsafe` FFI and has no job object), a passing check is assumed to leave no background writers running.
- Before a file is written, the result is parsed again; a set of cuts that would not parse is not written.
- If the check changes, creates or deletes a tracked or non-ignored file, or damages the journal (removing or overwriting `journal.json`, `.gitignore` or any saved copy, or replacing a journal entry or subfolder with a symbolic link), the run stops. Prune rebuilds a damaged journal in place from memory without following symlinks, records every changed file's bytes from before the run and what the check left, puts back every cut file the check did not touch, and leaves the files the check changed for you to inspect; `prune undo` puts them back. If prune cannot put its own files back, it says so and `prune undo` does it.
- The proof is what your check exercises: a check that ignores failures (`|| true`) or prints counts of its own proves nothing.

## Undo

Before a file's first edit, its original bytes go to `.puritanical-prune/` in the repository (a folder that ignores itself, so `git status` stays clean). Every write replaces the file atomically. For every file prune cut, and every tracked or non-ignored file prune recorded before a stop (including a baseline check), `undo` restores every original byte and removes the journal; it refuses, changing nothing, when a file holds bytes neither prune nor its stopped check left, so a later edit is never overwritten. New stop entries flush both copies to disk before `journal.json` publishes them. `keep` refuses a journal whose run did not finish.

A file the check changed that prune never cut has no copy from before the run, as prune cannot know in advance which files a check will touch. Since the tree was clean, `undo` restores Git's checkout of it from the `HEAD` tree the run began with (`git cat-file --filters`; not the index, which the check may have staged into: `undo` leaves the check's staging, and `git reset` drops it), which matches the earlier bytes unless a Git filter or line-ending setting writes the file differently from how it was on disk; a file the check created is removed.

If a check replaces a tracked or cut file with a symbolic link, prune never follows the link: it records the file's original bytes as absent, reports the link, refuses `undo` whilst the link stays, and restores the original bytes once you remove the link.

Assumptions and crash boundaries: nothing else edits the files whilst prune or `undo` runs; the check does not commit (a commit leaves `git status` clean, so prune would not see what it changed); the check leaves `.gitignore`d non-cut files alone, as `git status` does not list them; on Windows, a passing check leaves no background writers running and renames rely on the file system's own journal because the standard library cannot flush a folder there. If prune itself crashes whilst a check is running or before recording a check's edits, unrecorded edits to non-cut files are outside the journal (originals of cut files remain in `.puritanical-prune/orig/`). If a check deletes `.puritanical-prune/` and prune crashes before repair finishes, Git-tracked files remain in `HEAD`, whilst an untracked `--risky` file has no other durable copy.

Prune cuts only the candidate files and writes their temporary siblings and its journal folder; after a stopped run, `undo` also puts back the files the check changed. The finder's `cargo check` builds write `target/` and, when it is missing, `Cargo.lock`, as any build does. A path that leaves the repository, names `.git` or passes through a symbolic link is refused.
