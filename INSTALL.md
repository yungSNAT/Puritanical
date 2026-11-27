# Installation

## One-line installer (recommended)

```bash
npx github:yungSNAT/Puritanical
```

Detects supported agents and installs Puritanical for each:

| Agent | What gets installed | Scope |
|-------|--------------------|-------|
| Claude Code | plugin, or standalone hooks + statusline fallback | global |
| Pi | package containing extension + skill | global |
| Gemini CLI | Gemini extension | global |
| Codex | fenced ruleset in `~/.codex/AGENTS.md` | global |
| Copilot CLI | fenced ruleset in `~/.copilot/copilot-instructions.md` + `postToolUse` compression hook | global |
| OpenCode | fenced ruleset + skills + compression plugin under the configured OpenCode root | global |
| Hermes Agent | skills in `~/.hermes/skills` (Agent Skills standard, `/puritanical` commands) | global |
| Cursor / Windsurf / Cline / Kiro / Copilot / Antigravity | rule file in current project | project |

```bash
npx github:yungSNAT/Puritanical --list          # detect only
npx github:yungSNAT/Puritanical --only pi       # one agent; repeatable
npx github:yungSNAT/Puritanical --dry-run       # preview without mutation
npx github:yungSNAT/Puritanical --stats         # tool-output savings so far; read-only
npx github:yungSNAT/Puritanical --update # refresh what is installed; add nothing new
npx github:yungSNAT/Puritanical --force         # reinstall / overwrite
npx github:yungSNAT/Puritanical --uninstall     # remove everything installed
npx github:yungSNAT/Puritanical -u --only pi    # remove only Pi package
```

Shell launchers forward the same flags:

```bash
curl -fsSL https://raw.githubusercontent.com/yungSNAT/Puritanical/main/install.sh | bash
```

```powershell
irm https://raw.githubusercontent.com/yungSNAT/Puritanical/main/install.ps1 | iex
```

## Manual Claude Code plugin

```bash
claude plugin marketplace add yungSNAT/Puritanical
claude plugin install puritanical@puritanical
```

Restart Claude Code. Puritanical activates automatically.

## Manual Pi package

```bash
pi install git:github.com/yungSNAT/Puritanical
# or directly from Git:
pi install git:github.com/yungSNAT/Puritanical
```

Pi loads `pi-extension/index.js` and `skills/puritanical/SKILL.md` from the package. The extension supplies always-on rules, `/puritanical`, the status badge, and safe `tool_result` compression. Pi already reads a project's `AGENTS.md`; when that file contains Puritanical, the extension does not inject a duplicate rules message. The skill follows progressive disclosure and is also available as `/skill:puritanical`.

**Trust boundary:** Pi extensions execute with full user permissions. Review package source before installing. Global packages load for every project. A project-local install:

```bash
pi install -l git:github.com/yungSNAT/Puritanical
```

writes `.pi/settings.json` and loads only after Pi trusts that project.

## Manual OpenCode

```bash
npx github:yungSNAT/Puritanical --only opencode
```

Writes the fenced Puritanical ruleset into the global OpenCode config root
(appended after your existing instructions, never replacing them) and copies the
bundled skills into its `skills/` directory for on-demand loading through
OpenCode's native `skill` tool. The root follows `OPENCODE_CONFIG_DIR`, then
`$XDG_CONFIG_HOME/opencode`, then `~/.config/opencode`. No `instructions` entry
is added, so nothing loads twice. The ruleset is always on; `/puritanical` loads the
skill but does not toggle the global instructions.

It also installs the tool-output compression plugin into that root's `plugins/`.
OpenCode v1 keeps `puritanical.js` plus `puritanical-hooks/`; v2 gets the directory plugin
`puritanical/index.js` plus its colocated core. The installer selects the layout from
`opencode --version`. The v1 hooks remain unchanged. The v2 entrypoint uses
`Plugin.define({ id, setup })` and `ctx.tool.hook("execute.after", ...)`, eliding
oversized model-visible text while preserving structured output and metadata.
Both reuse the same compressor and configuration code. MCP tools are not
assumed safe from their names; add known read-only tools with
`PURITANICAL_COMPRESS_TOOLS`. Source-bearing and mutating tools remain excluded even
when configured. Disable compression with `PURITANICAL_COMPRESS=0` or
`PURITANICAL_DEFAULT_MODE=off`; this does not remove the always-on instructions.

## Manual Hermes Agent

```bash
npx github:yungSNAT/Puritanical --only hermes
```

Copies the bundled skills verbatim into `~/.hermes/skills/` (Agent Skills
standard), where Hermes discovers them as `/puritanical` slash commands. No ruleset
is injected and no config file is rewritten: Hermes already reads project
`AGENTS.md`, which carries the always-on axis when the repo ships it.

## Manual Copilot CLI

```bash
npx github:yungSNAT/Puritanical --only copilot-cli
```

Writes the ruleset into `~/.copilot/copilot-instructions.md` (fenced, your own
instructions kept), copies the compressor to `~/.copilot/puritanical-hooks/`, and
registers it as a user-level `postToolUse` hook in `~/.copilot/hooks/puritanical.json`.
Oversized `bash`/`grep`/`glob`/web/`task` results are compressed;
`view`/`create`/`edit` never are. Honors `COPILOT_HOME`. Savings:
`npx github:yungSNAT/Puritanical --stats --only copilot-cli`.

## MCP output compression (any agent)

Agents whose hooks cannot rewrite a tool result (Cursor, Windsurf, Cline, Kiro,
Gemini CLI, Qwen Code, Antigravity, Codex, GitHub Copilot in VS Code) still launch stdio MCP servers from a command line.
`puritanical mcp wrap` routes those servers through a small proxy that compresses
oversized `tools/call` results with the same core and passes every other byte
through unchanged:

```bash
npx github:yungSNAT/Puritanical mcp wrap            # every MCP config found below
npx github:yungSNAT/Puritanical mcp wrap --only cursor
npx github:yungSNAT/Puritanical mcp wrap --file ./.cursor/mcp.json   # a project-level file
npx github:yungSNAT/Puritanical mcp status          # wrapped servers + chars saved
npx github:yungSNAT/Puritanical mcp unwrap          # restore original commands
```

Covers `~/.cursor/mcp.json`, `~/.codeium/windsurf/mcp_config.json`,
`~/.gemini/settings.json`, `~/.qwen/settings.json`, `~/.kiro/settings/mcp.json`,
`~/.copilot/mcp-config.json`, `~/.gemini/config/mcp_config.json` (Antigravity),
`~/.codex/config.toml` (Codex, honors `CODEX_HOME`), the VS Code user `mcp.json`
(GitHub Copilot, `servers` key) in Code, Code Insiders and VSCodium, and Cline's
`cline_mcp_settings.json` in VS Code, Cursor and Windsurf. Restart the agent afterwards.

- Opt-in, never part of plain `npx github:yungSNAT/Puritanical`. Files with comments are reported and
  left alone; the first write keeps `<file>.puritanical-bak`.
- Only stdio servers are wrapped; `url`/HTTP servers are untouched.
- Tools whose names suggest file content, source, LSP-style queries or edits
  (`read_file`, `get_file_contents`, `search_code`, `find_symbol`, `hover`,
  `edit_file`, …) are never compressed. The check is by name only. Claude
  Code's and Pi's `mcp__*` tools follow the same rule.
- Output containing a credential (private key, JWT, cloud key, GitHub token,
  Authorization header) is still compressed but never written to a spill file.
- Claude Code is never wrapped: its own hook already compresses `mcp__*` output.
- Codex: only the `command`/`args` lines of `[mcp_servers.<name>]` tables change;
  a table with values other than plain strings is skipped. Unwrap restores the
  values (a multi-line `args` array comes back on one line); the untouched
  original stays in `config.toml.puritanical-bak`.
- `npx github:yungSNAT/Puritanical --uninstall` unwraps everything and removes the proxy.

## `puritanical-tools` binary

`puritanical-tools` is the Rust command-line tool in `tools/`. With Rust 1.100 or later, one command builds and installs it from this repository:

```bash
cargo install --locked --git https://github.com/yungSNAT/Puritanical puritanical-tools
```

Releases from 4.0.0 also attach prebuilt binaries for Linux (x86_64 and arm64, statically linked, so no glibc version is needed), Windows (x86_64) and macOS (Apple silicon), a `SHA256SUMS` file and a signed build provenance attestation for each binary. Download one and check where it was built:

```bash
gh release download --repo yungSNAT/Puritanical --pattern 'puritanical-tools-x86_64-unknown-linux-musl'
gh attestation verify puritanical-tools-x86_64-unknown-linux-musl --repo yungSNAT/Puritanical
```

The other names are `puritanical-tools-aarch64-unknown-linux-musl`, `puritanical-tools-x86_64-pc-windows-msvc.exe` and `puritanical-tools-aarch64-apple-darwin`. Without `gh`, download from `https://github.com/yungSNAT/Puritanical/releases/latest/download/<name>` and compare its SHA-256 with `SHA256SUMS`. Commands that read or write this repository's files (`rules`, `readme-art`, `answers`, and the benchmark commands without `RAW_DIR`) need `PURITANICAL_ROOT` set to a clone; `rules` and `readme-art` refuse a root whose `package.json` does not name `puritanical`. On macOS a browser download is quarantined: clear it with `xattr -d com.apple.quarantine puritanical-tools-aarch64-apple-darwin`. The tool has its own version number, separate from the plugin's.

## Upgrading

One command covers every agent that already has Puritanical:

```bash
npx github:yungSNAT/Puritanical --update
```

It refreshes what is installed and adds nothing new. Plain `npx github:yungSNAT/Puritanical` skips agents that already have Puritanical installed, so running it to refresh Puritanical changes nothing. Upgrading an older 3.x Chisle install replaces the old wiring under either `--update` or a plain install so the two never run side by side.

### Upgrading from 3.x

Puritanical was formerly Chisle, and 4.0.0 renames everything it installs. `--update` finds a Chisle install and replaces it: the `chisle@chisle` Claude Code plugin and its marketplace (a copy at project scope is left with the command to remove it), standalone hooks and a status line that runs only the old badge, the Gemini CLI extension, pi sources, skills, the Copilot CLI hook, the OpenCode plugin, fenced rule blocks and project rule files identical to a released copy; an edited one moves aside to `<file>.chisle-bak`. It moves the `.chisle-*` state (the savings ledger and the `tools trim` record among it) and the `chisle` config folder's settings to the new names, and points MCP servers wrapped by the old proxy at the new one. The Claude Code hook moves the state itself at the next session start. `CHISLE_*` variables, 'stop chisle' and 'chisle mode' keep working; `--uninstall` removes both sets of names.

### Per agent

Add `--only <id>` to update one agent. The native command is listed where the agent owns its own installer — either works.

| Agent | Update command | Notes |
|-------|----------------|-------|
| Claude Code | `npx github:yungSNAT/Puritanical --update --only claude`<br>or `claude plugin update puritanical@puritanical` | Restart the session afterwards. |
| Pi | `npx github:yungSNAT/Puritanical --update --only pi`<br>or `pi install git:github.com/yungSNAT/Puritanical` | Needs `pi` on `PATH`, or Puritanical reports it as not installed. |
| Gemini CLI | `npx github:yungSNAT/Puritanical --update --only gemini`<br>or `gemini extensions install https://github.com/yungSNAT/Puritanical` | Needs `gemini` on `PATH`. |
| Codex | `npx github:yungSNAT/Puritanical --update --only codex` | Refreshes the fenced block in `~/.codex/AGENTS.md`, leaving your own instructions alone. |
| Copilot CLI | `npx github:yungSNAT/Puritanical --update --only copilot-cli` | Refreshes the ruleset, compressor and hook under `~/.copilot`. Wrapped MCP servers pick up a new proxy after `npx github:yungSNAT/Puritanical mcp wrap`. |
| OpenCode | `npx github:yungSNAT/Puritanical --update --only opencode` | Refreshes the ruleset, skills, and compression plugin under the configured OpenCode root. |
| Hermes Agent | `npx github:yungSNAT/Puritanical --update --only hermes` | Refreshes the skills in `~/.hermes/skills/`. |
| Cursor | `npx github:yungSNAT/Puritanical --update --only cursor` | **Run inside the project.** |
| Windsurf | `npx github:yungSNAT/Puritanical --update --only windsurf` | **Run inside the project.** |
| Cline | `npx github:yungSNAT/Puritanical --update --only cline` | **Run inside the project.** |
| Kiro | `npx github:yungSNAT/Puritanical --update --only kiro` | **Run inside the project.** |
| GitHub Copilot | `npx github:yungSNAT/Puritanical --update --only copilot` | **Run inside the project.** |
| Antigravity | `npx github:yungSNAT/Puritanical --update --only antigravity` | **Run inside the project.** Writes `.agents/rules/puritanical.md`. |

The last six keep their rule file inside the repo, because that is how those agents load rules. `--update` looks for that file in the current directory, so running it from `~` reports *"Nothing to update"* even when the project is set up correctly. Run it once per repo that has one.

Preview any update without touching disk by adding `--dry-run`.

Puritanical's Claude hook checks this fork's GitHub releases at session start and mentions major releases once (cached three days). `PURITANICAL_UPDATE_CHECK=0` disables that check.


From 2.x to 3.x, replace legacy `lite` / `full` / `ultra` config with `on` / `off`, or remove it.

## Verify

Claude Code: start a session and run `/puritanical off`, then `/puritanical`.

OpenCode: the configured root's `AGENTS.md` contains the fenced block and `/puritanical` resolves as a skill. OpenCode v1 has `plugins/puritanical.js`; v2 has `plugins/puritanical/index.js`. Oversized `bash` (v1), `shell` (v2), grep, and webfetch output is elided in transcripts.

Hermes: `~/.hermes/skills/puritanical/SKILL.md` exists and `/puritanical` loads the skill.

Pi: start a session. Footer shows `[PURITANICAL]`; `/puritanical off`, `stop puritanical`, or `normal mode` clears it and disables both rules and tool-output compression. `/puritanical` re-enables it. Default mode `off` only changes session startup behavior.

Pi's default compression allowlist includes `bash`, `powershell`, `grep`, `find`, `ls`, and MCP tools. Add extension tools explicitly with `PURITANICAL_COMPRESS_TOOLS`. `read`, `edit`, and `write` are always untouched because exact output may feed later edits.

## Status badge

Pi's extension configures its footer badge automatically. Claude Code's standalone fallback configures the bundled statusline when no existing statusline is present. Manual Claude setup:

```json
{
  "statusLine": {
    "type": "command",
    "command": "bash \"/path/to/puritanical/hooks/puritanical-statusline.sh\""
  }
}
```

## Uninstall

```bash
npx github:yungSNAT/Puritanical --uninstall
# or Pi only:
pi remove git:github.com/yungSNAT/Puritanical
pi remove git:github.com/yungSNAT/Puritanical
```

Pi owns its package entry and cache; `pi remove` removes both without rewriting unrelated `settings.json` values. Project-scoped static rule files remain versioned project files and must be removed per repository.

Global `AGENTS.md` blocks keep surrounding user content: uninstall removes only the fenced `<!-- puritanical-begin -->` … `<!-- puritanical-end -->` block. Skill copies remove only the `puritanical*` skill dirs; foreign skills are untouched.

## Config

Resolution: environment → config file → `on`.

```bash
export PURITANICAL_DEFAULT_MODE=off
```

```json
// ~/.config/puritanical/config.json
{ "defaultMode": "off" }
```

At the end of a first install or update in an interactive terminal, the installer asks
whether to star the repo and follow @JayPokale on GitHub (`[y]es / [n]o / [L]ater`),
leading with the tokens Puritanical has saved you when its ledger has any. Only a `y` does
anything, and then your own `gh` login stars and follows (without `gh` it prints the
links). Enter means later: nothing is saved and it asks at the next install or update.
An explicit `y` or `n` is saved as `"starPrompt"` in `~/.config/puritanical/config.json` and
never asked again. CI, `--dry-run` and piped runs never ask.

Input-axis switches work across Claude Code, Pi, Copilot, and OpenCode where supported:

```bash
PURITANICAL_COMPRESS=0
PURITANICAL_COMPRESS_SCRUB=0
PURITANICAL_COMPRESS_FILTERS=0       # command-aware filters (git, tests, installs)
PURITANICAL_COMPRESS_WRAP=0          # Claude Code: never wrap commands (=1: wrap in every permission mode)
PURITANICAL_COMPRESS_DEDUP=0
PURITANICAL_COMPRESS_MAX_CHARS=8000  # also the final OpenCode output bound
PURITANICAL_COMPRESS_HEAD_LINES=60
PURITANICAL_COMPRESS_TAIL_LINES=40
PURITANICAL_COMPRESS_TOOLS=bash,grep
```

Command-aware filters work like [RTK](https://github.com/rtk-ai/rtk) without a
binary: when the shell command is known they drop `git status` hints, `git push`/`pull`
progress, passing-test lines (pytest, jest, vitest, mocha, `node --test`, `go test`,
`cargo test`) and install/download noise (npm, pnpm, pip, uv, cargo, go, docker). They
also reformat: long `git status` becomes the short `XY path` form, default `git log` and
`git show` get one header line per commit, `git diff` headers shrink to a path line,
`git branch -a` folds remotes onto one line, and `ls -l`, `wc` and `grep -rn` lose
padding and repeated paths. Failures, errors and summaries are never dropped. With more
than one commit, `git log` keeps 3 body lines per commit and counts the rest
(`git show <hash>` has it all; `--stat` file lists are never cut); `ls -l` drops dates unless sorted by
time (`-t`). A command piped into anything but `head`/`tail` is left alone, so
`git status | cat` shows raw output; commands already run through `rtk` are skipped.

Claude Code passes a failed command's output to `PostToolUseFailure`, which cannot
rewrite it, so a `PreToolUse` hook (`hooks/puritanical-run.js`) runs test, build and install
commands through a small Node wrapper that filters their output even when they fail,
and keeps the exit status. Claude Code checks permission rules against the rewritten
command, so the wrapper is on only in `bypassPermissions` mode, never touches a
program named by a deny or ask `Bash(...)` rule, and skips anything it cannot parse
safely (subshells, `$(...)`, background jobs, watch mode). Output reaches Claude when
the command ends, not as it streams. `PURITANICAL_COMPRESS_WRAP=1` turns it on in every
permission mode (expect prompts for the wrapped command unless you allow it);
`PURITANICAL_COMPRESS_WRAP=0` turns it off. Not on Windows.

`PURITANICAL_COMPRESS_TOOLS` replaces the allowlist. OpenCode defaults to `bash`,
`grep`, `glob`, `webfetch`, `websearch`, and `list`; configure read-only MCP
tools explicitly. The override cannot enable source-bearing or mutating tools
such as `read`, `edit`, `write`, `patch`, or notebook edits. Their output can
feed later exact-match edits, so this is a correctness guarantee, not a default.
