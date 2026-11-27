# Prune: remove dead code with proof

`puritanical-tools prune` removes the dead code a compiler proves dead from an existing repository. It keeps each cut only if your own check still passes, and it can put every byte back.

```sh
cd tools && cargo build --release            # once; the binary is tools/target/release/puritanical-tools
puritanical-tools prune --repo <repo>        # dry run: lists what it would cut, changes nothing
puritanical-tools prune --repo <repo> --apply -- cargo test
puritanical-tools prune --repo <repo> undo   # put every changed file back, byte for byte
puritanical-tools prune --repo <repo> keep   # accept the cuts and drop the journal
```

`--repo` defaults to the current folder, which must be the top of a Git work tree. The check after `--` runs as given, without a shell; for a pipeline pass `sh -c '...'`.

## What it finds

Rust only, in a Cargo workspace at the repository root: functions, constants, statics, structs, enums, unions, type aliases, traits, and methods and associated constants of inherent `impl` blocks that rustc reports as `dead_code`. A struct, enum, union or trait goes together with the `impl` blocks in the same module that name it.

An item is a candidate only when rustc reports it dead in all 4 builds prune runs: `cargo check --workspace` and `cargo check --workspace --profile test`, each with the default features and with `--all-features`. So:

- an item used only by unit tests, or only under a feature, is not a candidate;
- public items of a library, `#[no_mangle]`, `#[export_name]` and `#[used]` items, and items under `#[allow(dead_code)]` or `#[expect(dead_code)]` are never candidates, because rustc does not report them;
- code reached through a macro, a trait or a re-export is seen, because rustc resolves names rather than counting them.

Each item's extent, with its attributes and doc comments, comes from a parse of the file with `syn`, matched to rustc's span of the item's name. Whole lines go, and a blank line goes with them where 2 would otherwise be left together or one would be left at the edge of a block or the file.

## What it does not find

Unused imports, macros, enum variants and struct fields; dead files and modules; unused dependencies; non-item comments next to a dead item (they stay); and every language other than Rust. Files under a `tests`, `benches` or `examples` folder and `#[cfg(test)]` modules are never cut.

## Risky candidates

These are listed but cut only with `--risky`:

- an item behind an attribute other than `doc`, lint levels, `inline`, `cold`, `must_use`, `deprecated`, `derive`, `track_caller`, `cfg`, `rustfmt::*` or `clippy::*` (for example `#[repr(C)]` or an attribute macro);
- every item of a package whose sources hold a `cfg` naming anything but `test` and features (such as `unix`, `target_os` or `debug_assertions`), because code compiled only on another platform or profile may use an item this build calls dead;
- every item when the `--all-features` build fails, because code behind features was not seen.

If you cut risky items, put the other platforms' and profiles' builds in your check.

## How a cut is proved

- Prune refuses to start when the tree has any change or untracked file, when a journal from an earlier run exists, when the finder's build or the check changes the tree, when the check fails before any cut, or when the check prints no test count it can read (pass `--uncounted` to rely on the exit status alone).
- It reads passed-test counts from libtest (`test result: ... N passed`), Node's test runner (`# pass N`), pytest (`N passed ... in`), Jest (`Tests: N passed`) and Vitest (`Tests N passed`), each kept apart: a cut stays only if the check exits 0 and no runner's count is missing or lower than at the start.
- All candidates are tried at once. A failing batch is split in halves until each failing cut stands alone; a cut rejected only because another dead item still used it is tried again after a pass that kept something.
- Each check run may take 3 times the first run's time plus 60 seconds; a run past that is killed and counts as a failure.
- Before a file is written, the result is parsed again; a set of cuts that would not parse is not written.
- If the check itself changes a file, the run stops, prune's own edits are undone, and the files the check changed are listed and left as the check left them.
- The proof is what your check exercises: a check that ignores failures (`|| true`) or prints counts of its own proves nothing.

## Undo

Before a file's first edit, its original bytes go to `.puritanical-prune/` in the repository (a folder that ignores itself, so `git status` stays clean). Every write replaces the file atomically. After a finished, failed or crashed run, `undo` restores every original byte and removes the journal; it refuses, changing nothing, when a file holds bytes prune did not write, so a later edit is never overwritten. `keep` refuses a journal whose run did not finish.

Assumptions: nothing else edits the files whilst prune or undo runs; on Windows, renames rely on the file system's own journal, as the standard library cannot flush a folder there.

Prune writes only the candidate files, their temporary siblings and its journal folder. A path that leaves the repository, names `.git` or passes through a symbolic link is refused.
