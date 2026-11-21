"""Rebuild registered prompts and replay saved answers offline on Linux."""
import hashlib
import json
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import sys
import tempfile

HERE = Path(__file__).resolve().parent
CHECKERS = ("retired_checks.py", "ponytail_checks.py", "batch_checks.py")
READY = b"benchmark runtime ready\n"


def load(name):
    return json.loads((HERE / name).read_text(encoding="utf-8"))


def digest(text):
    return hashlib.sha256(text.encode()).hexdigest()


def prompt(case, rules):
    return ("Use these active coding instructions.\n\n" + rules
            + "\n\nWork only from the synthetic repository below, without tools. Return exactly one JSON object "
            'with a "files" object mapping relative paths to the full text of each new or changed file. '
            "Omit unchanged files. Include tests if you normally would for this change.\n\n"
            + case["prompt"] + "\n\nRepository files (JSON):\n"
            + json.dumps(case["seed"], ensure_ascii=False, sort_keys=True))


def judge(task, answer):
    case = load("fixtures.json")[task]
    if case["checker"] == "node" and not shutil.which("node"):
        raise RuntimeError("Node.js is required for this fixture")
    sys.stderr.buffer.write(READY)
    sys.stderr.buffer.flush()
    try:
        files = json.loads(answer)["files"]
        if not isinstance(files, dict):
            raise ValueError()
        for name, value in files.items():
            path = PurePosixPath(name)
            if (not path.parts or path.is_absolute() or ".." in path.parts or "\\" in name
                    or ":" in name or "\0" in name or not isinstance(value, str)):
                raise ValueError()
    except (TypeError, KeyError, ValueError):
        return {"pass": False, "skip_notice": False if task == "csv-sum" else None}
    with tempfile.TemporaryDirectory() as tmp:
        repo = Path(tmp)
        notice = False if task == "csv-sum" else None
        try:
            for rel, content in {**case["seed"], **files}.items():
                dest = repo / rel
                dest.parent.mkdir(parents=True, exist_ok=True)
                dest.write_text(content, encoding="utf-8")
            if case["checker"] == "node":
                proc = subprocess.run(["node", "-e", case["test"]], cwd=repo,
                                      stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=20)
                passed = proc.returncode == 0
            else:
                from importlib import import_module
                checker = import_module(case["checker"])
                result = getattr(checker, case["function"])(repo)
                passed = result if isinstance(result, bool) else result["correct"] and result["safe"]
                if task == "csv-sum" and passed:
                    from batch_checks import reports_skipped
                    try:
                        notice = reports_skipped(repo)
                    except Exception:
                        notice = False
        except (Exception, SystemExit):
            # A submitted implementation may raise any application exception.
            passed = False
    return {"pass": bool(passed), "skip_notice": notice}


def score(task, answer):
    cmd = ["bwrap", "--unshare-all", "--die-with-parent", "--new-session", "--cap-drop", "ALL",
           "--clearenv", "--setenv", "PATH", "/usr/bin", "--setenv", "PYTHONDONTWRITEBYTECODE", "1"]
    for lib in ("/usr", "/bin", "/lib", "/lib64"):
        if Path(lib).exists():
            cmd.extend(["--ro-bind", lib, lib])
    cmd.extend(["--dev", "/dev", "--proc", "/proc", "--size", "67108864", "--tmpfs", "/tmp",
                "--chdir", "/tmp", "--dir", "/bench"])
    for name in ("replay.py", "fixtures.json", *CHECKERS):
        cmd.extend(["--ro-bind", str(HERE / name), "/bench/" + name])
    cmd.extend(["/usr/bin/python3", "/bench/replay.py", "--grade", task])
    failed = {"pass": False, "skip_notice": False if task == "csv-sum" else None}
    try:
        out = subprocess.run(cmd, input=answer.encode(), capture_output=True, timeout=75)
    except subprocess.TimeoutExpired as error:
        if READY not in (error.stderr or b""):
            raise RuntimeError("Grader did not start") from error
        return failed
    if READY not in out.stderr:
        raise RuntimeError(f"Grader setup failed: {out.stderr.decode(errors='replace')[:500]}")
    if out.returncode or not out.stdout.strip():
        return failed
    try:
        result = json.loads(out.stdout.splitlines()[-1])
    except (ValueError, UnicodeError):
        return failed
    if not isinstance(result, dict) or not isinstance(result.get("pass"), bool):
        return failed
    return result


def verify():
    design = load("registration.json")
    for name, want in design["hashes"].items():
        if digest((HERE / name).read_text(encoding="utf-8")) != want:
            raise ValueError("Frozen input differs: " + name)
    rules, cases, rows = load("rules.json"), load("fixtures.json"), load("receipts.json")
    order = design["order"]
    keys = {(q["task"], q["arm"], q["repeat"]) for q in order}
    expected = {(t, a, r) for t in cases for a in rules for r in (1, 2)}
    if len(rows) != len(order) or len(order) != len(expected) or keys != expected:
        raise ValueError("Missing or duplicate scheduled cell")
    totals = {a: dict(passed=0, scored=0, input=0, cached=0, output=0) for a in rules}
    for spec, row in zip(order, rows):
        task, arm = spec["task"], spec["arm"]
        if any(row[k] != v for k, v in spec.items()) or digest(prompt(cases[task], rules[arm])) != spec["prompt_sha256"]:
            raise ValueError(f"Prompt or schedule mismatch: {spec}")
        if row["status"] != "completed":
            if row["status"] != "infrastructure" or row["answer"] is not None or row["verdict"] is not None:
                raise ValueError("Invalid missing response")
            continue
        if digest(row["answer"]) != row["answer_sha256"] or score(task, row["answer"]) != row["verdict"]:
            raise ValueError(f"Answer or verdict mismatch: {spec}")
        t, u = totals[arm], row["usage"]
        t["passed"] += row["verdict"]["pass"]
        t["scored"] += 1
        t["input"] += u["input_tokens"]
        t["cached"] += u["cached_input_tokens"]
        t["output"] += u["output_tokens"]
    print(json.dumps(totals, indent=2))


if __name__ == "__main__":
    if len(sys.argv) == 3 and sys.argv[1] == "--grade" and HERE == Path("/bench"):
        print(json.dumps(judge(sys.argv[2], sys.stdin.read())))
    elif len(sys.argv) == 4 and sys.argv[1] == "--prompt":
        sys.stdout.write(prompt(load("fixtures.json")[sys.argv[2]], load("rules.json")[sys.argv[3]]))
    elif len(sys.argv) == 1:
        verify()
    else:
        sys.exit("Usage: replay.py [--prompt TASK ARM]")
