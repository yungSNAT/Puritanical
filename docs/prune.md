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

- an item behind an attribute other than `doc`, lint levels, `inline`, `cold`, `must_use`, `deprecated`, `derive`, `track_caller`, `cfg`, `rustfmt::*` or `clippy::*` (for example `#[repr(C)]` or an attribute macro);
- every item of a package whose sources hold a `cfg` naming anything but `test` and features (such as `unix`, `target_os` or `debug_assertions`), because code compiled only on another platform or profile may use an item this build calls dead;
- every item of a package with `not` around a feature in a `cfg` (such as `#[cfg(not(feature = "std"))]` for `no_std` code), because a build without that feature compiles code the 4 builds never see;
- every item of a package with a `#[path]` module or an `include!` item, because prune cannot tell which crates compile which files;
- every item in a file Git does not track (an ignored, generated file), because Git holds no copy of it;
- every item when the `--all-features` build fails, because code behind features was not seen.

If you cut risky items, put the other platforms' and profiles' builds in your check.

## How a cut is proved

- Prune refuses to start when the tree has any change or untracked file, when a journal from an earlier run exists, when the finder's build or the check changes the tree, when the check fails before any cut (it shows the end of the check's output), when the check prints no libtest count above 0 (pass `--uncounted` to rely on the exit status alone), or when a file to be cut changed after the finder read it.
- It sums the passed counts of libtest's summary lines (`test result: ... N passed`): a cut stays only if the check exits 0 and the count is there and not lower than at the start.
- All candidates are tried at once. A failing batch is split in halves until each failing cut stands alone; a cut rejected only because another dead item still used it is tried again after a pass that kept something.
- Each check run may take 3 times the first run's time plus 60 seconds; a run past that is killed, with every process it started (on Unix the check leads its own process group, so Ctrl-C stops prune but not a check already running), and counts as a failure.
- Before a file is written, the result is parsed again; a set of cuts that would not parse is not written.
- If the check itself changes, creates or deletes a file, or damages the journal, the run stops. Prune puts back every file it cut that the check did not touch and lists the files the check changed, leaving them as the check left them for you to inspect; their bytes from before the run are in the journal, and `prune undo` puts them back. If prune cannot put its own files back, it says so and `prune undo` does it.
- The proof is what your check exercises: a check that ignores failures (`|| true`) or prints counts of its own proves nothing.

## Undo

Before a file's first edit, its original bytes go to `.puritanical-prune/` in the repository (a folder that ignores itself, so `git status` stays clean). Every write replaces the file atomically. After a finished, failed, stopped or crashed run, `undo` restores every original byte and removes the journal; it refuses, changing nothing, when a file holds bytes neither prune nor its stopped check left, so a later edit is never overwritten. `keep` refuses a journal whose run did not finish.

A file the check changed that prune never cut has no copy from before the run, as prune cannot know in advance which files a check will touch. Since the tree was clean, `undo` restores Git's checkout of it from the `HEAD` tree the run began with (`git cat-file --filters`; not the index, which the check may have staged into), which matches the earlier bytes unless a Git filter or line-ending setting writes the file differently from how it was on disk; a file the check created is removed.

Assumptions: nothing else edits the files whilst prune or undo runs; on Windows, renames rely on the file system's own journal, as the standard library cannot flush a folder there.

Prune cuts only the candidate files and writes their temporary siblings and its journal folder; after a stopped run, `undo` also puts back the files the check changed. The finder's `cargo check` builds write `target/` and, when it is missing, `Cargo.lock`, as any build does. A path that leaves the repository, names `.git` or passes through a symbolic link is refused.
