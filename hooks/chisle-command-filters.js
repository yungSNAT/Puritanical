/* global module, require -- Node.js runtime */
// chisle — command-aware output filters (RTK-style, but post-hoc).
//
// Design credit: RTK (https://github.com/rtk-ai/rtk). The short git status,
// one-line log headers with capped bodies, folded remote branches and compact
// ls/wc/test output follow its output shapes; no RTK code is used.
//
// The size tiers in chisle-compress-output.js only start at 1k/8k chars and
// know nothing about which command ran. Most shell noise is smaller and
// command-specific: git's "(use git add ...)" hints, passing-test lines,
// install/download progress, padded columns. Knowing the command lets us drop
// or reformat exactly those lines at any size, from inside the hook that
// already runs, so it works on every OS and harness Chisle hooks, with no
// wrapper binary. chisle-run.js reuses the same filters before a command
// exits, so Claude Code failures (which no PostToolUse hook can rewrite) get
// filtered too.
//
// Safety rules:
//   - Only commands we recognise are touched; anything else returns null.
//   - A segment piped into anything but head/tail is skipped: the agent shaped
//     that output itself (`npm i | grep WARN` must keep its WARN lines).
//     `| cat` therefore always shows raw output.
//   - Failures, errors, warnings and summaries are never dropped. Lossy
//     reformatting is limited to multi-item overviews (git log bodies and
//     stats past a cap, ls dates), with a "[+N]" count where lines go.
//   - Output already filtered by rtk (`rtk git status`) is left alone.
//   - The result is used only if it is shorter.

// CSI and OSC escapes; chisle-compress-output.js shares this.
// eslint-disable-next-line no-control-regex -- ANSI escapes are delimited by the ESC and BEL control characters.
const ANSI_RE = /\x1b\[[0-9;?]*[ -/]*[@-~]|\x1b\][^\x07\x1b]*(?:\x07|\x1b\\)/g;

// Wrappers that run another program: skip them to find the real one.
const RUNNERS = new Set(['sudo', 'time', 'command', 'exec', 'nice', 'env', 'npx', 'bunx', 'pnpx']);
const RUNNER_PAIRS = /^(?:(?:pnpm|yarn) (?:exec|dlx)|(?:uv|poetry|pipenv|rye|hatch|pdm) run|bundle exec|python[\d.]* -m|py -m)\s+/;

// `NAME=value ` assignments: a name, then quoted or bare value pieces, then
// whitespace. Sticky regexes walk them one piece at a time.
const ENV_NAME = /[A-Za-z_]\w*=/y;
const ENV_VALUE = /'[^']*'|"(?:\\.|[^"\\])*"|[^\s'"\\]+/y;
const ENV_SPACE = /\s+/y;

function stickyEnd(re, s, i) {
  re.lastIndex = i;
  return re.test(s) ? re.lastIndex : -1;
}

// End offset of the run of env assignments starting at `start` (start if none).
function envAssignEnd(s, start) {
  let end = start;
  for (;;) {
    let i = stickyEnd(ENV_NAME, s, end);
    if (i < 0) return end;
    for (let next = stickyEnd(ENV_VALUE, s, i); next >= 0; next = stickyEnd(ENV_VALUE, s, i)) i = next;
    i = stickyEnd(ENV_SPACE, s, i);
    if (i < 0) return end;
    end = i;
  }
}

const TEST_PROGRAMS = new Set(['pytest', 'py.test', 'jest', 'vitest', 'mocha', 'ava', 'tap', 'rspec', 'phpunit']);
const PKG_MANAGERS = new Set(['npm', 'pnpm', 'yarn', 'bun']);

// Subcommands that mean "run the tests" / "download or build", per program.
const TEST_SUBCOMMANDS = new Map([
  ['go', /^test$/], ['deno', /^test$/], ['dotnet', /^test$/], ['bun', /^(?:test|t)$/],
  ['npm', /^(?:test|t)$/], ['pnpm', /^(?:test|t)$/], ['yarn', /^(?:test|t)$/],
  ['cargo', /^(?:test|nextest)$/], ['make', /\b(?:test|check)\b/],
]);
const PKG_INSTALL = /^(?:install|i|ci|add|update|upgrade)$/;
const PROGRESS_SUBCOMMANDS = new Map([
  ['npm', PKG_INSTALL], ['pnpm', PKG_INSTALL], ['yarn', PKG_INSTALL], ['bun', PKG_INSTALL],
  ['uv', /^(?:pip|sync|add)$/], ['poetry', /^(?:install|add|update)$/],
  ['cargo', /^(?:build|check|run|test|nextest|clippy|install|update|fetch|doc)$/],
  ['go', /^(?:build|test|get|mod|run|install|vet)$/], ['docker', /^(?:build|pull|compose|buildx)$/],
  ['bundle', /^(?:install|update)$/], ['composer', /^(?:install|update)$/],
]);

function stripQuotes(command) {
  return command.replace(/'[^']*'|"(?:\\.|[^"\\])*"/g, '""');
}

function subcommandMatches(table, { prog, sub }) {
  const re = table.get(prog);
  return !!re && re.test(sub);
}

function isTest(c) {
  if (TEST_PROGRAMS.has(c.prog) || (c.prog === 'node' && /\s--test\b/.test(c.text))) return true;
  if (PKG_MANAGERS.has(c.prog) && /^run(?:-script)?$/.test(c.sub) && /\s(?:run|run-script)\s+test/.test(c.text)) return true;
  return subcommandMatches(TEST_SUBCOMMANDS, c);
}

function isProgress(c) {
  return c.prog === 'docker-compose' || /^pip(?:\d*|\d*\.\d+)$/.test(c.prog) || subcommandMatches(PROGRESS_SUBCOMMANDS, c);
}

function isGrep({ prog, flags }) {
  if (prog === 'rg') return true;
  return (prog === 'grep' || prog === 'egrep') && (flags.has('n') || flags.has('--line-number')) &&
    ['r', 'R', 'H', '--recursive'].some(f => flags.has(f));
}

const isGit = (c, sub) => c.prog === 'git' && sub.test(c.sub);

// [kind, test] over { prog, sub, flags, text }; a segment gets every kind that matches.
const KIND_RULES = [
  ['git-status', c => isGit(c, /^status$/) && !['s', '--short', '--porcelain'].some(f => c.flags.has(f))],
  ['progress', c => isGit(c, /^(?:push|pull|fetch|clone)$/)],
  ['git-log', c => isGit(c, /^(?:log|show)$/) && !/--(?:format|pretty|oneline)\b|--graph\b/.test(c.text)],
  ['git-diff', c => isGit(c, /^(?:diff|show|log)$/)],
  ['git-branch', c => isGit(c, /^branch$/) && !c.flags.has('v') && !c.flags.has('--verbose')],
  ['ls', c => c.prog === 'ls' && c.flags.has('l')],
  ['wc', c => c.prog === 'wc'],
  ['grep', isGrep],
  ['test', isTest],
  ['progress', isProgress],
];

// Drop env assignments and runner wrappers (`sudo`, `npx`, `uv run`, ...).
function stripRunners(segment) {
  let s = segment.slice(envAssignEnd(segment, 0));
  for (;;) {
    const word = s.split(/\s+/)[0];
    if (RUNNERS.has(word)) {
      s = s.slice(word.length).trim();
      s = s.slice(envAssignEnd(s, 0));
      continue;
    }
    const pair = s.match(RUNNER_PAIRS);
    if (!pair) return s;
    s = s.slice(pair[0].length);
  }
}

// First non-option argument after the program, skipping `-C dir`-style pairs.
function subcommand(words) {
  for (let i = 1; i < words.length; i++) {
    if (/^-[Cc]$/.test(words.at(i))) { i++; continue; }
    if (!words.at(i).startsWith('-')) return words.at(i);
  }
  return '';
}

// Short-flag letters (`-la` → l, a) plus long flags, for ls/grep/git branch.
function flagSet(words) {
  const flags = new Set();
  for (const w of words.slice(1)) {
    if (/^--\w/.test(w)) flags.add(w.replace(/=.*/, ''));
    else if (/^-[A-Za-z]+$/.test(w)) for (const c of w.slice(1)) flags.add(c);
  }
  return flags;
}

// One command segment → { kinds, prog, sub, flags }, or null if unknown/piped.
function classify(segment) {
  const parts = segment.split('|').map(s => s.trim());
  const pipedTo = parts.slice(1).map(p => p.split(/\s+/)[0]);
  if (pipedTo.some(p => p !== 'head' && p !== 'tail')) return null;

  const s = stripRunners(parts[0]);
  const words = s.split(/\s+/).filter(w => w && !/^[<>&]|^\d>/.test(w));
  const prog = (words[0] || '').replace(/^.*[\\/]/, '').replace(/\.(?:exe|cmd)$/i, '');
  if (prog === 'rtk') return null;
  const c = { prog, sub: subcommand(words), flags: flagSet(words), text: s };
  const kinds = new Set(KIND_RULES.filter(([, matches]) => matches(c)).map(([kind]) => kind));
  return kinds.size ? { kinds, ...c } : null;
}

function kindsFor(command) {
  if (typeof command !== 'string' || !command.trim()) return null;
  const kinds = new Set();
  for (const seg of stripQuotes(command).split(/&&|\|\||;|\n/)) {
    const c = seg.trim() && classify(seg.trim());
    if (c) for (const x of c.kinds) kinds.add(x);
  }
  return kinds.size ? kinds : null;
}

// Worth running through chisle-run.js: commands that end on their own and fail
// loudly (tests, builds, installs). Not git/docker, not `run`, not watch mode.
function wrappable(segment) {
  const c = classify(stripQuotes(segment).trim());
  if (!c || /--watch\b/.test(c.text)) return false;
  if (c.kinds.has('test')) return true;
  return c.kinds.has('progress') && !/^(?:git|docker|docker-compose)$/.test(c.prog) && c.sub !== 'run';
}

// ── Drop rules: [kind, regex] ────────────────────────────────────────────────
const DROP = [
  ['git-status', /^\s*\((?:use "git |commit or discard )[^)]*\)\s*$/],
  ['progress', /^(?:remote: )?(?:Enumerating|Counting|Compressing|Writing|Receiving|Unpacking) objects:|^(?:remote: )?Resolving deltas:|^(?:remote: )?Total \d+ \(delta|^Delta compression using|^(?:Updating|Checking out) files:/],
  // npm / pnpm
  ['progress', /^\d+ packages? (?:is|are) looking for funding$|^\s*run `npm fund` for details$|^Progress: resolved \d+|^\+{5,}$/],
  // pip
  ['progress', /^Requirement already satisfied: |^\s*(?:Collecting|Downloading|Using cached|Obtaining) \S|^\s*━+/],
  // cargo / go
  ['progress', /^\s+(?:Compiling|Checking|Downloaded|Downloading|Fresh|Locking|Adding|Blocking) \S|^\s+Updating crates\.io index$|^go: (?:downloading|finding|extracting) /],
  // docker pull / buildkit
  ['progress', /^[0-9a-f]{12}: (?:Pulling fs layer|Waiting|Verifying Checksum|Download complete|Pull complete|Already exists|Downloading|Extracting)|^#\d+ (?:sha256:[0-9a-f]+ [\d.]+[kMG]?B \/|transferring |extracting sha256:)/],
  // passing tests: pytest dots/-v, jest/vitest/mocha/node spec ticks, cargo, go
  ['test', /^\S+\.py\s+[.sx]+\s*\[\s*\d+%\]$|^\S+::\S.*\sPASSED$|^\S+::\S.*\sPASSED\s+\[\s*\d+%\]$|^\s*[✓✔√]\s|^\s*PASS\s+\S|^\s*▶\s|^test \S+ \.\.\. ok$|^\s*--- PASS: |^\s*# Subtest: /],
  // cargo test scaffolding: build/run banners, per-suite headers, empty suites,
  // and the backtrace hint
  ['test', /^\s+Finished `|^\s+Running \S.* \(target[\\/]|^\s+Doc-tests \S+$|^running \d+ tests?$|^test result: ok\. 0 passed; 0 failed;|^note: run with `RUST_BACKTRACE=1`/],
];

// `go test -v`: a test's log lines sit between its `=== RUN`/`CONT` line and
// the next marker. Passing tests go whole (header and logs), so no log line
// is left orphaned beside a failure; failing and skipped tests keep theirs.
function goVerbose(lines) {
  if (!lines.some(l => /^=== RUN\s/.test(l))) return lines;
  const keep = new Set(lines.map(l => l.match(/^\s*--- (?:FAIL|SKIP): (\S+)/)).filter(Boolean).map(m => m[1]));
  const out = [];
  let cur = null;
  for (const l of lines) {
    const m = l.match(/^=== (RUN|PAUSE|CONT|NAME)\s+(\S+)/);
    if (m) { cur = m[2]; if (keep.has(cur) && m[1] !== 'PAUSE') out.push(l); continue; }
    if (/^\s*--- (?:PASS|FAIL|SKIP): /.test(l)) cur = null;
    else if (cur && !keep.has(cur)) continue;
    out.push(l);
  }
  return out;
}

// node --test TAP: `ok N - name` plus its indented YAML block.
function dropTapPasses(lines) {
  const out = [];
  for (let i = 0; i < lines.length; i++) {
    if (/^\s*ok \d+ - /.test(lines.at(i))) {
      if (lines[i + 1] && /^\s*---$/.test(lines[i + 1])) {
        let j = i + 2;
        while (j < lines.length && !/^\s*\.\.\.$/.test(lines.at(j))) j++;
        i = j;
      }
      continue;
    }
    out.push(lines.at(i));
  }
  return out;
}

// cargo: zero counts carry nothing; the trailing `failures:` name list repeats
// the `---- name stdout ----` blocks above it.
function compactCargo(lines) {
  const out = [];
  for (let i = 0; i < lines.length; i++) {
    const l = lines.at(i);
    if (l === 'failures:' && /^ {4}\S+$/.test(lines[i + 1] || '')) {
      while (/^ {4}\S+$/.test(lines[i + 1] || '')) i++;
      continue;
    }
    out.push(l.replace(/^(test result: \w+\. .*?); finished in (\S+)$/, (_, head, t) =>
      `${head.replace(/; 0 [a-z ]+(?=;|$)/g, '')} (${t})`));
  }
  return out;
}

// Long `git status` → the short format (`XY path`), as `git status -sb` prints.
const STATUS_SECTIONS = new Map(Object.entries({
  'Changes to be committed:': 'x', 'Changes not staged for commit:': 'y',
  'Unmerged paths:': 'u', 'Untracked files:': '??', 'Ignored files:': '!!',
}));
const STATUS_CODES = new Map(Object.entries({
  modified: 'M', 'new file': 'A', deleted: 'D', renamed: 'R', copied: 'C', typechange: 'T',
}));
const UNMERGED_CODES = new Map(Object.entries({
  'both modified': 'UU', 'added by us': 'AU', 'added by them': 'UA', 'deleted by us': 'DU',
  'deleted by them': 'UD', 'both added': 'AA', 'both deleted': 'DD',
}));

function shortStatus(lines) {
  const out = [];
  const files = new Map();
  let sec = null;
  let at = -1;
  for (const l of lines) {
    const section = STATUS_SECTIONS.get(l);
    if (section) { sec = section; if (at < 0) at = out.length; continue; }
    if (!l.trim()) { sec = null; continue; }
    if (sec && l.startsWith('\t')) {
      const e = l.slice(1);
      if (sec === '??' || sec === '!!') { files.set(sec + e, `${sec} ${e}`); continue; }
      const m = e.match(/^([a-z ]+):\s+(.*)$/);
      const code = m && (sec === 'u' ? UNMERGED_CODES : STATUS_CODES).get(m[1]);
      if (code) {
        if (sec === 'u') { files.set(m[2], `${code} ${m[2]}`); continue; }
        const prev = files.get(m[2]);
        const x = sec === 'x' ? code : (prev ? prev[0] : ' ');
        const y = sec === 'y' ? code : ' ';
        files.set(m[2], `${x}${y} ${m[2]}`);
        continue;
      }
    }
    if (/^no changes added to commit|^nothing added to commit but untracked files present/.test(l)) continue;
    out.push(l);
  }
  if (at >= 0) out.splice(at, 0, ...files.values());
  // Branch header, `## branch...upstream [ahead N, behind M]` style.
  const text = out.join('\n')
    .replace(/^On branch (\S+)\nYour branch is up to date with '([^']+)'\.$/m, '* $1...$2')
    .replace(/^On branch (\S+)\nYour branch is (ahead of|behind) '([^']+)' by (\d+) commits?(?:, and can be fast-forwarded)?\.$/m,
      (_, b, dir, up, n) => `* ${b}...${up} [${dir === 'behind' ? 'behind' : 'ahead'} ${n}]`)
    .replace(/^On branch (\S+)\nYour branch and '([^']+)' have diverged,\nand have (\d+) and (\d+) different commits each, respectively\.$/m,
      '* $1...$2 [ahead $3, behind $4]')
    .replace(/^On branch (\S+)$/m, '* $1');
  return text.split('\n');
}

// Diff headers (`diff --git`, `index`, `---`, `+++`) → one path line.
function compactDiff(lines) {
  const out = [];
  for (let i = 0; i < lines.length; i++) {
    const m = lines.at(i).match(/^diff --git a\/(.*) b\/(.*)$/);
    if (!m) { out.push(lines.at(i)); continue; }
    let path = m[2];
    const notes = [];
    let j = i + 1;
    for (; j < lines.length; j++) {
      const h = lines.at(j);
      let x;
      if (/^index [0-9a-f]+\.\.[0-9a-f]+/.test(h) || /^similarity index /.test(h) || /^dissimilarity index /.test(h)) continue;
      if ((x = h.match(/^new file mode (\d+)/))) { notes.push('new'); continue; }
      if ((x = h.match(/^deleted file mode (\d+)/))) { notes.push('deleted'); continue; }
      if ((x = h.match(/^old mode (\d+)/))) { notes.push(`mode ${x[1]}`); continue; }
      if ((x = h.match(/^new mode (\d+)/))) { notes.push(`-> ${x[1]}`); continue; }
      if ((x = h.match(/^(?:rename|copy) from (.*)/))) { path = `${x[1]} -> ${path}`; continue; }
      if (/^(?:rename|copy) to /.test(h)) continue;
      if ((x = h.match(/^--- (?:a\/(.*)|\/dev\/null)$/))) continue;
      if ((x = h.match(/^\+\+\+ (?:b\/(.*)|\/dev\/null)$/))) { if (x[1]) path = path.includes(' -> ') ? path : x[1]; continue; }
      break;
    }
    out.push(notes.length ? `${path} (${notes.join(' ')})` : path);
    i = j - 1;
  }
  return out;
}

// --stat blocks: drop the +/- bar and the alignment padding. Only lines in
// the run right above a ` N files changed` summary count, so a diff context
// line that happens to read ` Total | 42` is never touched.
const STAT_RE = /^ (\S.*?)\s+\|\s+(\d+(?= ?[+-]*$)|Bin .*)/;
const STAT_FILES = /^ (\d+) files? changed(.*)$/;
const STAT_INSERTIONS = /^, (\d+) insertions?\(\+\)/;
const STAT_DELETIONS = /^, (\d+) deletions?\(-\)$/;

// ` N files changed[, N insertions(+)][, N deletions(-)]` → [line, files, ins, del].
function statSummary(line) {
  const t = line.match(STAT_FILES);
  if (!t) return null;
  let rest = t[2];
  const ins = rest.match(STAT_INSERTIONS);
  if (ins) rest = rest.slice(ins[0].length);
  const del = rest.match(STAT_DELETIONS);
  if (rest && !del) return null;
  return [line, t[1], ins ? ins[1] : undefined, del ? del[1] : undefined];
}

function compactStat(lines) {
  const out = lines.slice();
  for (let i = 0; i < lines.length; i++) {
    const t = statSummary(lines.at(i));
    if (!t) continue;
    out.splice(i, 1, `${t[1]} file${t[1] === '1' ? '' : 's'}${t[2] ? ` +${t[2]}` : ''}${t[3] ? ` -${t[3]}` : ''}`);
    for (let j = i - 1, m; j >= 0 && (m = lines.at(j).match(STAT_RE)); j--) out.splice(j, 1, `${m[1]} | ${m[2]}`);
  }
  return out;
}

const MONTHS = { Jan: '01', Feb: '02', Mar: '03', Apr: '04', May: '05', Jun: '06', Jul: '07', Aug: '08', Sep: '09', Oct: '10', Nov: '11', Dec: '12' };
const BODY_CAP = 3;

// Default `git log`/`git show` blocks → `hash subject (date author)`, body
// indented. Hash shortened to 10 (unambiguous in all but the largest
// repos; git's default 7 collides there), email dropped,
// date to the day, an author shared by every commit named once. With more
// than one commit, bodies past a cap become a count (`git show <hash>` has
// the rest). --stat file lists are never capped: a hidden file would read
// as "this commit did not touch it".
const authorName = (raw) => raw.replace(/\s*<[^>]*>$/, '');

// `Mon Jan 2 15:04:05 2006 -0700` → `2006-01-02`; anything else unchanged.
function shortDate(raw) {
  const d = raw.match(/^\w{3} (\w{3}) (\d{1,2}) [\d:]+ (\d{4}) [+-]\d{4}$/);
  return d && MONTHS[d[1]] ? `${d[3]}-${MONTHS[d[1]]}-${d[2].padStart(2, '0')}` : raw;
}

// `Merge:`/`Author:`/`Date:` lines from `start` → { merge, meta, end }.
function commitMeta(lines, start, oneAuthor) {
  let meta = '';
  let merge = '';
  let j = start;
  for (; j < lines.length; j++) {
    const h = lines.at(j).match(/^(Merge|Author|Date):\s+(.*)$/);
    if (!h) break;
    if (h[1] === 'Merge') merge = ` merge ${h[2]}`;
    else if (h[1] === 'Author') meta += oneAuthor ? '' : ` ${authorName(h[2])}`;
    else meta = ` ${shortDate(h[2])}` + meta;
  }
  return { merge, meta, end: j };
}

// Message: 4-space-indented lines (blank lines inside it dropped) → { body, end }.
function commitBody(lines, start) {
  let j = lines.at(start) === '' ? start + 1 : start;
  const body = [];
  for (; j < lines.length && (lines.at(j).startsWith('    ') || lines.at(j).trim() === ''); j++) {
    if (lines.at(j).trim()) body.push(lines.at(j).slice(4));
    else if (j + 1 < lines.length && !lines[j + 1].startsWith('    ')) { j++; break; }
  }
  return { body, end: j };
}

function compactGitLog(lines) {
  const commits = lines.filter(l => /^commit [0-9a-f]{40}/.test(l)).length;
  const many = commits > 1;
  const authors = new Set(lines.filter(l => /^Author:\s/.test(l)).map(l => authorName(l.replace(/^Author:\s+/, ''))));
  const oneAuthor = many && authors.size === 1 ? [...authors][0] : null;
  const out = oneAuthor ? [`(all by ${oneAuthor})`] : [];
  for (let i = 0; i < lines.length; i++) {
    const m = lines.at(i).match(/^commit ([0-9a-f]{40})(.*)$/);
    if (!m) { out.push(lines.at(i)); continue; }
    const { merge, meta, end } = commitMeta(lines, i + 1, oneAuthor);
    const { body, end: bodyEnd } = commitBody(lines, end);
    out.push(`${m[1].slice(0, 10)}${m[2]}${merge} ${body.shift() || ''} (${meta.trim()})`);
    const keep = many ? body.slice(0, BODY_CAP) : body;
    for (const b of keep) out.push(`  ${b}`);
    if (keep.length < body.length) out.push(`  [+${body.length - keep.length} lines]`);
    // Stat block or patch (if any) follows.
    let j = bodyEnd;
    for (; j < lines.length && !/^commit [0-9a-f]{40}/.test(lines.at(j)); j++) {
      if (many && !lines.at(j).trim()) continue;
      out.push(lines.at(j));
    }
    i = j - 1;
  }
  return out;
}

// `git branch -a`: remote branches folded into one line per remote.
function compactBranches(lines, allRemote) {
  const out = [];
  const remotes = new Map();
  for (const l of lines) {
    const m = allRemote ? l.match(/^ {2}([^/\s]+)\/(\S+|\S+ -> \S+)$/) : l.match(/^ {2}remotes\/([^/\s]+)\/(\S+|\S+ -> \S+)$/);
    if (!m) { out.push(l); continue; }
    if (!remotes.has(m[1])) { remotes.set(m[1], []); out.push(m[1]); }
    remotes.get(m[1]).push(m[2].replace(` -> ${m[1]}/`, ' -> '));
  }
  return out.map(l => (remotes.has(l) && !l.startsWith(' ') ? `${l}/: ${remotes.get(l).join(', ')}` : l));
}

// `ls -l`: octal mode, size, name. Owner named once up top unless it is you
// for every entry, then only where an entry differs; date only with -t
// (sorted by it). Link count, `total` and `..` dropped; `.` kept (the
// directory's own mode); ACL/xattr marks (`+`, `@`) kept on the mode.
// Split in two: mode, links, owner, group and size, then date and name. The
// ISO date lists its time/zone variants longest first, as the optional
// groups they replace would try them.
const LS_HEAD = /^([-dlcbps])([rwxsStT-]{9})([.+@]?)\s+\d+\s+(\S+)\s+(\S+)\s+(\d+|\d+,\s*\d+|[\d.]+[KMGTP]?)\s+/;
const LS_TAIL = /^(\w{3}\s+\d{1,2}\s+(?:\d{1,2}:\d{2}|\d{4})|\d{4}-\d{2}-\d{2} [\d:.]+ [+-]\d{4}|\d{4}-\d{2}-\d{2} [\d:.]+|\d{4}-\d{2}-\d{2})\s(.+)$/;

// One `ls -l` row → [row, type, perm, mark, owner, group, size, date, name].
function lsRow(line) {
  const head = line.match(LS_HEAD);
  const tail = head && line.slice(head[0].length).match(LS_TAIL);
  return tail ? [line, ...head.slice(1), tail[1], tail[2]] : null;
}
let me = null;
try { me = require('os').userInfo().username; } catch {}

function octal(perm) {
  let special = 0;
  let digits = '';
  for (let k = 0; k < 3; k++) {
    const t = perm.slice(k * 3, k * 3 + 3);
    let v = (t[0] === 'r' ? 4 : 0) + (t[1] === 'w' ? 2 : 0);
    if ('xst'.includes(t[2])) v += 1;
    if ('sStT'.includes(t[2])) special += [4, 2, 1].at(k);
    digits += v;
  }
  return special ? special + digits : digits;
}

function compactLs(lines, keepDate) {
  const parsed = lines.map(lsRow);
  const owners = new Map();
  for (const p of parsed) if (p) owners.set(`${p[4]}:${p[5]}`, (owners.get(`${p[4]}:${p[5]}`) || 0) + 1);
  const main = [...owners.entries()].sort((a, b) => b[1] - a[1])[0];
  const out = [];
  if (main && (owners.size > 1 || main[0].split(':')[0] !== me)) out.push(`(owner ${main[0]}${owners.size > 1 ? ' unless shown' : ''})`);
  lines.forEach((l, i) => {
    const p = parsed.at(i);
    if (!p) { if (!/^total \d/.test(l)) out.push(l); return; }
    let name = p[8];
    if (name === '..') return;
    const own = `${p[4]}:${p[5]}` === main[0] ? '' : ` ${p[4]}:${p[5]}`;
    const date = keepDate ? ` ${p[7].replace(/\s+/g, ' ')}` : '';
    if (p[1] === 'd') name += '/';
    const size = p[1] === 'd' ? '' : ` ${p[6]}`;
    const type = '-dl'.includes(p[1]) ? '' : p[1];
    out.push(`${type}${octal(p[2])}${p[3] === '.' ? '' : p[3]}${size}${own}${date} ${name}`);
  });
  return out;
}

// `wc`: column padding dropped; a directory shared by every path said once,
// as a header line like grep's.
function compactWc(lines) {
  const rows = lines.map(l => l.trim().split(/\s+/));
  const paths = rows.filter(r => r.length > 1 && r[r.length - 1] !== 'total').map(r => r[r.length - 1]);
  const dir = paths.length > 1 && paths[0].includes('/') ? paths[0].slice(0, paths[0].lastIndexOf('/') + 1) : '';
  const shared = dir && paths.every(p => p.startsWith(dir) && !p.slice(dir.length).includes('/'));
  const out = rows.map(r => {
    if (shared && r.length > 1 && r[r.length - 1] !== 'total') r[r.length - 1] = r[r.length - 1].slice(dir.length);
    return r.join(' ');
  });
  return shared ? [dir, ...out] : out;
}

// `grep -rn` / `rg`: `path:line:text` → path once, then `line:text`.
function groupGrep(lines) {
  const out = [];
  let cur = null;
  for (const l of lines) {
    const m = l.match(/^(.+?):(\d+):(.*)$/);
    const ctx = !m && cur && l.startsWith(cur + '-') && l.slice(cur.length).match(/^-(\d+)-(.*)$/);
    if (m) {
      if (m[1] !== cur) { cur = m[1]; out.push(cur); }
      out.push(`${m[2]}:${m[3]}`);
    } else if (ctx) out.push(`${ctx[1]}-${ctx[2]}`);
    else { if (l !== '--') cur = null; out.push(l); }
  }
  return out;
}

const PIPELINE_SAFE = /(?:error|fail|warn|denied|fatal|panic|exception|traceback)/i;

function filterCommand(command, text) {
  if (typeof text !== 'string' || !text) return null;
  const kinds = kindsFor(command);
  if (!kinds) return null;
  const segs = stripQuotes(command).split(/&&|\|\||;|\n/).map(s => s.trim() && classify(s.trim())).filter(Boolean);
  const has = (flag) => segs.some(c => c.flags.has(flag));

  // Progress bars redraw with \r; only the last frame was ever visible.
  let lines = text.replace(ANSI_RE, '').split('\n').map(l => {
    const t = l.replace(/\r$/, '');
    return t.includes('\r') ? t.split('\r').filter(Boolean).pop() || '' : t;
  });
  if (kinds.has('test')) lines = goVerbose(dropTapPasses(lines));
  lines = lines.filter(l => !DROP.some(([kind, re]) => kinds.has(kind) && re.test(l) && !(kind !== 'test' && PIPELINE_SAFE.test(l))));
  if (kinds.has('test')) lines = compactCargo(lines);
  if (kinds.has('git-status')) lines = shortStatus(lines);
  if (kinds.has('git-diff')) lines = compactStat(compactDiff(lines));
  if (kinds.has('git-log')) lines = compactGitLog(lines);
  if (kinds.has('git-branch')) lines = compactBranches(lines, has('r') || has('--remotes'));
  if (kinds.has('ls')) lines = compactLs(lines, has('t'));
  if (kinds.has('wc') && kinds.size === 1) lines = compactWc(lines);
  if (kinds.has('grep') && kinds.size === 1) lines = groupGrep(lines);

  const out = lines.join('\n').replace(/\n{3,}/g, '\n\n').replace(/^\n+|\n+$/g, '');
  return out.length < text.replace(/^\n+|\n+$/g, '').length ? out : null;
}

module.exports = { filterCommand, kindsFor, wrappable, envAssignEnd, ANSI_RE };
