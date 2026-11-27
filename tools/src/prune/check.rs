// SPDX-License-Identifier: Apache-2.0

//! Runs the user's check and reads its passed-test counts.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Passed tests per runner format. Formats are never summed together.
pub type Counts = BTreeMap<&'static str, u64>;

/// One run of the check.
#[derive(Debug)]
pub struct Run {
    pub ok: bool,
    pub counts: Counts,
    pub took: Duration,
}

/// The digits that end `text` before `word`, as in `12 passed`.
fn number_before(text: &str, word: &str) -> Option<u64> {
    let head = &text[..text.find(word)?];
    let digits = head.len() - head.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    head[head.len() - digits..].parse().ok()
}

/// The runner format and passed count one output line reports, if any.
fn count_line(line: &str) -> Option<(&'static str, u64)> {
    let t = line.trim();
    if let Some(rest) = t.strip_prefix("test result: ") {
        return Some(("libtest", number_before(rest, " passed")?));
    }
    if let Some(rest) = t
        .strip_prefix("# pass ")
        .or_else(|| t.strip_prefix("\u{2139} pass "))
    {
        return Some(("node", rest.trim().parse().ok()?));
    }
    if let Some(rest) = t.strip_prefix("Tests:") {
        return Some(("jest", number_before(rest, " passed")?));
    }
    if let Some(rest) = t.strip_prefix("Tests ") {
        return Some(("vitest", number_before(rest, " passed")?));
    }
    let core = t.trim_matches(|c| c == '=' || c == ' ');
    (core.contains(" in ") && core.ends_with('s'))
        .then(|| number_before(core, " passed"))
        .flatten()
        .map(|n| ("pytest", n))
}

/// Passed-test totals per format in a check's output.
#[must_use]
pub fn counts(output: &str) -> Counts {
    let mut c = Counts::new();
    for (format, n) in output.lines().filter_map(count_line) {
        *c.entry(format).or_default() += n;
    }
    c
}

/// Whether every format the baseline reported is still there and not lower.
#[must_use]
pub fn holds(baseline: &Counts, now: &Counts) -> bool {
    baseline
        .iter()
        .all(|(f, n)| now.get(f).is_some_and(|m| m >= n))
}

/// `libtest 12, node 3`, or `no test count`.
#[must_use]
pub fn describe(c: &Counts) -> String {
    if c.is_empty() {
        return "no test count".into();
    }
    c.iter()
        .map(|(f, n)| format!("{f} {n}"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// Runs `argv` in `root` with exactly `env`, output to `log`. A run still
/// going after `limit` is killed and counts as failed.
///
/// # Errors
/// The command cannot start or its output cannot be read.
pub fn run(
    root: &Path,
    argv: &[String],
    env: &HashMap<String, String>,
    log: &Path,
    limit: Duration,
) -> Result<Run, String> {
    let (program, rest) = argv.split_first().ok_or("no check command")?;
    let out = File::create(log).map_err(super::io(log))?;
    let err = out.try_clone().map_err(super::io(log))?;
    let start = Instant::now();
    let mut child = Command::new(program)
        .args(rest)
        .current_dir(root)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err)
        .spawn()
        .map_err(super::cannot(program))?;
    let (status, timed_out) = loop {
        if start.elapsed() >= limit {
            let _ = child.kill();
            break (child.wait().map_err(|e| e.to_string())?, true);
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break (status, false);
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let text = std::fs::read(log).map_err(super::io(log))?;
    Ok(Run {
        ok: status.success() && !timed_out,
        counts: counts(&String::from_utf8_lossy(&text)),
        took: start.elapsed(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_each_runner_format_apart() {
        let out = "test result: ok. 3 passed; 0 failed; finished in 0.01s\n\
                   test result: ok. 2 passed; 0 failed\n\
                   # pass 4\n\u{2139} pass 1\n\
                   Tests:       7 passed, 7 total\n\
                   \x20     Tests  6 passed (6)\n\
                   ===== 1 failed, 5 passed in 0.12s =====\n\
                   8 passed in 0.3s\n\
                   12 passed but no timing\n\
                   test result: FAILED. no count\n";
        let c = counts(out);
        assert_eq!(
            describe(&c),
            "jest 7, libtest 5, node 5, pytest 13, vitest 6"
        );
        assert_eq!(describe(&Counts::new()), "no test count");
    }

    #[test]
    fn a_fall_in_one_format_is_not_hidden_by_another() {
        let base = counts("test result: ok. 3 passed;\n# pass 2\n");
        assert!(holds(
            &base,
            &counts("test result: ok. 3 passed;\n# pass 2\n")
        ));
        assert!(holds(
            &base,
            &counts("test result: ok. 4 passed;\n# pass 2\n")
        ));
        assert!(!holds(
            &base,
            &counts("test result: ok. 2 passed;\n# pass 9\n")
        ));
        assert!(!holds(&base, &counts("test result: ok. 9 passed;\n")));
        assert_eq!(number_before("x passed", " passed"), None);
    }

    #[test]
    fn a_check_past_its_limit_is_killed_and_fails() {
        let d = tempfile::tempdir().expect("temp dir");
        let env: HashMap<String, String> = std::env::vars().collect();
        let log = d.path().join("log");
        let git = ["git".to_string(), "--version".to_string()];
        let r = run(d.path(), &git, &env, &log, Duration::ZERO).expect("run");
        // git --version passes unless it is killed.
        assert!(!r.ok);
        let r = run(d.path(), &git, &env, &log, Duration::from_secs(60)).expect("run");
        assert!(r.ok && r.took < Duration::from_secs(60));
        let missing = run(
            d.path(),
            &["no-such-program-x".to_string()],
            &env,
            &log,
            Duration::ZERO,
        );
        assert!(
            missing
                .unwrap_err()
                .starts_with("cannot run no-such-program-x: ")
        );
    }
}
