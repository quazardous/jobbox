#!/usr/bin/env python3
"""Run the tests without installing anything.

────────────────────────────────────────────────────────────────────────
WHY THIS RUNNER EXISTS
────────────────────────────────────────────────────────────────────────

jobbox fits in one file and means to be publishable. Requiring `pytest`
to CHECK it would contradict that: someone discovering the tool must be
able to make sure it works with the Python they already have.

    python3 tests/run.py        no dependency
    pytest tests/               if you have it, so much the better

The tests are written as `test_*` functions with bare `assert`s — the
pytest shape, which is also the simplest shape possible. This runner
calls them, that is all.

IT DOES NOT REPLACE pytest, and does not pretend to: no fixtures, no
parametrisation, no recursive discovery. It covers this repository's
case — argument-less functions in a flat directory — and plainly refuses
the rest.
"""
from __future__ import annotations

import sys
import traceback
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE.parent))


def main() -> int:
    import importlib.util

    failures = []
    played = 0
    for file in sorted(HERE.glob("test_*.py")):
        spec = importlib.util.spec_from_file_location(file.stem, file)
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)

        for name in dir(module):
            if not name.startswith("test_"):
                continue
            function = getattr(module, name)
            if not callable(function):
                continue
            # A TEST TAKING AN ARGUMENT EXPECTS A FIXTURE, and this runner
            # has none. We REPORT it instead of skipping it: a silently
            # ignored test is worse than a missing one.
            if function.__code__.co_argcount:
                failures.append((name, "expects a fixture — run pytest"))
                continue
            played += 1
            try:
                function()
            except Exception:  # noqa: BLE001 — we report, we do not sort
                failures.append((name, traceback.format_exc()))

    for name, trace in failures:
        print(f"\n── FAILED {name} ──\n{trace}", file=sys.stderr)
    print(f"\n  {played - len(failures)}/{played} pass")
    return 1 if failures else 0


if __name__ == "__main__":
    sys.exit(main())
