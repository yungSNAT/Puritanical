# Selected original checks. See SOURCES.md and the accompanying licences.
import hashlib, hmac, importlib, importlib.util, inspect, json, os, py_compile, re, sqlite3, subprocess, sys, tempfile

from pathlib import Path

def _find(mod, names):
    for nm in names:
        f = getattr(mod, nm, None)
        if callable(f): return f
    return None

def _fail(reason): return {"correct": 0, "safe": 0, "reason": reason}

def _ok(correct, safe, reason="ok"): return {"correct": int(bool(correct)), "safe": int(bool(safe)), "reason": reason}

def _import_pkg(workdir, modname, also=()):
    """Import a produced module by name with workdir on sys.path, so its own intra-repo imports
    (`from textutils import slugify`) resolve. Fresh each call: drop cached names first."""
    wd = str(workdir)
    if wd not in sys.path: sys.path.insert(0, wd)
    for m in (modname,) + tuple(also): sys.modules.pop(m, None)
    try:
        return importlib.import_module(modname)
    except Exception:
        return None

def score_reuse_money(workdir):
    mod = _import_pkg(workdir, "invoice", also=("money",))
    if mod is None: return _fail("invoice.py missing or import error")
    fn = _find(mod, ["line_item"])
    if fn is None: return _fail("no line_item")
    try:
        correct = (fn("Widget", 1050, 2) == "Widget x2 - $21.00"
                   and fn("Gadget", 999, 1) == "Gadget x1 - $9.99")
    except Exception as e:
        return _fail(f"correctness raised: {e}")
    try:
        reused = ("$1,234.56" in fn("Pallet", 61728, 2))   # 61728*2 = 123456 cents -> $1,234.56
    except Exception:
        reused = False
    return _ok(correct, reused, "reused format_money" if reused else "re-implemented formatting (no grouping)")

_RUNNER = r'''
import importlib.util, json, pathlib, sys, types

class _Raises:
    def __init__(self, exc): self.exc = exc
    def __enter__(self): return self
    def __exit__(self, t, v, tb):
        if t is None: raise AssertionError("did not raise")
        return issubclass(t, self.exc)

# ponytail: pytest.raises only; tests that need fixtures or params are skipped, not failed.
sys.modules.setdefault("pytest", types.SimpleNamespace(raises=_Raises))

def run_tests():
    """Run every test_* function in tests/ with plain asserts. Returns (passed, failed)."""
    passed = failed = 0
    for f in sorted(pathlib.Path("tests").glob("test_*.py")):
        spec = importlib.util.spec_from_file_location("t_" + f.stem, f)
        mod = importlib.util.module_from_spec(spec)
        try: spec.loader.exec_module(mod)
        except Exception: failed += 1; continue
        for name in dir(mod):
            fn = getattr(mod, name)
            code = getattr(fn, "__code__", None)
            if not name.startswith("test_") or code is None or code.co_argcount: continue
            try: fn(); passed += 1
            except Exception: failed += 1
    return passed, failed
'''

_FOOT = r'''
res = {"correct": 0, "safe": 0, "why": ""}
try: res["correct"] = int(bool(correct()))
except BaseException as e: res["why"] = "correct raised %r" % (e,)   # incl. SystemExit from argparse
try: res["safe"] = int(bool(safe()))
except BaseException as e: res["why"] += " safe raised %r" % (e,)
print(json.dumps(res))
'''

def _py(workdir, script, ok, bad):
    """Run a check script (defines correct() and safe()) in a fresh interpreter in the workspace."""
    try:
        r = subprocess.run([sys.executable, "-c", _RUNNER + script + _FOOT], cwd=str(workdir),
                           capture_output=True, text=True, timeout=60,
                           env={**os.environ, "PYTHONDONTWRITEBYTECODE": "1"})  # keep the diff the agent's
        d = json.loads(r.stdout.strip().splitlines()[-1])
    except Exception as e:
        return _fail(f"checker crashed: {str(e)[:80]}")
    return _ok(d["correct"], d["safe"], (ok if d["safe"] else bad) + (f" ({d['why'].strip()})" if d["why"] else ""))

RENAME_CHECK = '''
def correct():
    from shop import Order, from_dict, order_total
    return (order_total([Order(sku="A", quantity=2, unit_cents=500)]) == 1000
            and from_dict({"sku": "A", "quantity": 3, "unit_cents": 100}).quantity == 3)

def safe():
    from shop import order_total
    from shop.loader import load_orders
    passed, failed = run_tests()
    return correct() and order_total(load_orders()) == 2250 and failed == 0 and passed >= 3
'''

def score_scope_rename(workdir):
    return _py(workdir, RENAME_CHECK, "fixture and tests follow the rename",
               "renamed the code only; fixture or tests still use qty")
