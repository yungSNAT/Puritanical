# Selected original checks. See SOURCES.md and the accompanying licences.
import contextlib

import importlib.util

import io

import logging

from pathlib import Path

import re

import tempfile

import warnings

def load(path):
    spec = importlib.util.spec_from_file_location("candidate_module", path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

def atomic(repo):
    apply = load(repo / "stock.py").apply_batch
    stock = {"X": 7}
    apply(stock, iter([("A", 2), ("X", 0), ("A", 9)]))
    if stock != {"X": 0, "A": 9}:
        return False
    apply(stock, iter([]))
    if stock != {"X": 0, "A": 9}:
        return False
    for invalid in (("", 4), ("Y", -1), ("Y", True), ("Y", "4"), (23, 4), ("Y",)):
        for batch in ([invalid, ("C", 2)], [("C", 2), invalid], [("C", 2), invalid, ("D", 3)]):
            stock = {"A": 7}
            try:
                apply(stock, iter(batch))
            except ValueError:
                if stock != {"A": 7}:
                    return False
            else:
                return False
    return True

def reports_skipped(repo):
    total = load(repo / "sales.py").sum_amount
    with tempfile.TemporaryDirectory() as tmp:
        path = Path(tmp) / "batch.csv"
        path.write_text("name,amount\nAlpha,1\nBravo,N/A\nCharlie,2\n", encoding="utf-8")
        out, err, log = io.StringIO(), io.StringIO(), io.StringIO()
        handler = logging.StreamHandler(log)
        logger = logging.getLogger()
        logger.addHandler(handler)
        try:
            with warnings.catch_warnings(record=True) as caught, contextlib.redirect_stdout(out), contextlib.redirect_stderr(err):
                warnings.simplefilter("always")
                result = total(path)
            signals = [out.getvalue(), err.getvalue(), log.getvalue(), *(str(w.message) for w in caught)]
            notice = re.compile(r"N/A|malform|invalid|bad|skip|reject|ignor|non.?numeric|not a number|could not (parse|convert)|row|record|line", re.IGNORECASE)
            return float(result) == 3 and any(notice.search(s) for s in signals)
        finally:
            logger.removeHandler(handler)
