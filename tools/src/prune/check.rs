// SPDX-License-Identifier: Apache-2.0

//! Runs the user's check and reads its libtest passed-test counts.

use std::collections::{BTreeMap, HashMap};
use std::fs::File;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Passed tests per runner format (libtest, `cargo test`'s, is the only one
/// read whilst Rust is the only finder).
pub type Counts = BTreeMap<&'static str, u64>;

/// One run of the check.
#[derive(Debug)]
pub struct Run {
    pub ok: bool,
    pub counts: Counts,
    pub took: Duration,
    /// The last 20 lines of its output.
    pub tail: String,
}

/// The digits that end `text` before `word`, as in `12 passed`.
fn number_before(text: &str, word: &str) -> Option<u64> {
    let head = &text[..text.find(word)?];
    let digits = head.len() - head.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    head[head.len() - digits..].parse().ok()
}

/// The passed count a libtest summary line reports, if any.
fn count_line(line: &str) -> Option<(&'static str, u64)> {
    let rest = line.trim().strip_prefix("test result: ")?;
    Some(("libtest", number_before(rest, " passed")?))
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

/// Kills process `pid` and every process it started: on Unix its process
/// group (the check leads its own), on Windows its process tree.
fn kill_tree(pid: u32) {
    #[cfg(unix)]
    let _ = Command::new("kill")
        .args(["-s", "KILL", "--", &format!("-{pid}")])
        .output();
    #[cfg(windows)]
    let _ = Command::new("taskkill")
        .args(["/T", "/F", "/PID", &pid.to_string()])
        .output();
}

/// Runs `argv` in `root` with exactly `env`, output to `log`. A run still
/// going after `limit` is killed, with every process it started, and counts
/// as failed.
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
    let mut cmd = Command::new(program);
    cmd.args(rest)
        .current_dir(root)
        .env_clear()
        .envs(env)
        .stdin(Stdio::null())
        .stdout(out)
        .stderr(err);
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut cmd, 0);
    let mut child = cmd.spawn().map_err(super::cannot(program))?;
    let (status, timed_out) = loop {
        if start.elapsed() >= limit {
            kill_tree(child.id());
            let _ = child.kill();
            break (child.wait().map_err(|e| e.to_string())?, true);
        }
        if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
            break (status, false);
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    // What the check left running would change files after prune looked: on
    // Unix its process group goes too. (Windows would need a job object.)
    #[cfg(unix)]
    kill_tree(child.id());
    let text = std::fs::read(log).map_err(super::io(log))?;
    let text = String::from_utf8_lossy(&text);
    let lines: Vec<&str> = text.lines().collect();
    Ok(Run {
        ok: status.success() && !timed_out,
        counts: counts(&text),
        took: start.elapsed(),
        tail: lines[lines.len().saturating_sub(20)..].join("\n"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_libtest_summaries_and_reads_nothing_else() {
        let out = "test result: ok. 3 passed; 0 failed; finished in 0.01s\n\
                   \x20  test result: ok. 2 passed; 0 failed\n\
                   # pass 4\n\
                   ===== 5 passed in 0.12s =====\n\
                   test result: FAILED. no count\n";
        assert_eq!(describe(&counts(out)), "libtest 5");
        assert_eq!(describe(&Counts::new()), "no test count");
    }

    #[test]
    fn a_count_may_not_fall_or_go_missing() {
        let base = counts("test result: ok. 3 passed;\n");
        assert!(holds(&base, &counts("test result: ok. 3 passed;\n")));
        assert!(holds(&base, &counts("test result: ok. 4 passed;\n")));
        assert!(!holds(&base, &counts("test result: ok. 2 passed;\n")));
        assert!(!holds(&base, &counts("no summary\n")));
        assert_eq!(number_before("x passed", " passed"), None);
    }

    #[cfg(unix)]
    #[test]
    fn a_killed_check_takes_the_processes_it_started_with_it() {
        let d = tempfile::tempdir().expect("temp dir");
        let env: HashMap<String, String> = std::env::vars().collect();
        let script = "(sleep 2; touch late) & sleep 30";
        let sh = ["sh", "-c", script].map(str::to_string);
        let limit = Duration::from_millis(500);
        let r = run(d.path(), &sh, &env, &d.path().join("log"), limit).expect("run");
        assert!(!r.ok && r.took < Duration::from_secs(2));
        std::thread::sleep(Duration::from_secs(3));
        assert!(!d.path().join("late").exists());
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
