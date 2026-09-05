"""Measuring what actually takes time — before deciding to act on it.

────────────────────────────────────────────────────────────────────────
WHY MEASURE AT ALL
────────────────────────────────────────────────────────────────────────

The question behind this is whether something should force long commands
into the background. Nobody knows, because nobody has the numbers, and
the belief "this one is long" has been wrong in both directions.

Two properties matter more than the arithmetic, and both are tested here.

**It must not change what it measures.** No decision, no reminder, no
rewritten input — the command leaves exactly as typed. A measurement
that perturbs its subject answers a different question.

**It must not keep the command line.** A command can carry a secret, and
this table lives in a cache directory for weeks. Only the SHAPE is kept,
which is also all the grouping needs.
"""
from __future__ import annotations

import io
import json
import sys
import tempfile
import time
from contextlib import contextmanager, redirect_stdout
from pathlib import Path

import jobbox


@contextmanager
def _on_stdin(text: str):
    """The symmetric gesture to `redirect_stdout` — the standard library
    has no `redirect_stdin`, and adding a parameter to `main` just so a
    test can reach it would put the test's convenience in the product.
    """
    previous = sys.stdin
    sys.stdin = io.StringIO(text)
    try:
        yield
    finally:
        sys.stdin = previous


def _feed(events: list[dict]) -> tuple[list[dict], str]:
    """Play hook events through `observe`, return (the table, its output)."""
    previous_timings = jobbox.TIMINGS
    previous_pending, previous_observed = jobbox.PENDING, jobbox.OBSERVED
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.TIMINGS = Path(tmp) / "timings"
        jobbox.PENDING = jobbox.TIMINGS / "pending"
        jobbox.OBSERVED = jobbox.TIMINGS / "observed.jsonl"
        buffer = io.StringIO()
        try:
            for event in events:
                with _on_stdin(json.dumps(event)), redirect_stdout(buffer):
                    jobbox.main(["observe"])
            rows = []
            if jobbox.OBSERVED.exists():
                rows = [json.loads(l) for l
                        in jobbox.OBSERVED.read_text(encoding="utf-8")
                        .splitlines() if l.strip()]
        finally:
            jobbox.TIMINGS = previous_timings
            jobbox.PENDING, jobbox.OBSERVED = previous_pending, previous_observed
    return rows, buffer.getvalue()


def _pair(key: str, command: str, background: bool = False) -> list[dict]:
    return [
        {"hook_event_name": "PreToolUse", "tool_name": "Bash",
         "tool_use_id": key,
         "tool_input": {"command": command, "run_in_background": background}},
        {"hook_event_name": "PostToolUse", "tool_name": "Bash",
         "tool_use_id": key, "tool_input": {"command": command}},
    ]


def test_THE_COMMAND_LINE_IS_NEVER_STORED():
    """THE PROMISE THAT MATTERS MOST, and the one nobody would notice broken.

    A command can carry a secret inline — `TOKEN=… ./deploy` is ordinary
    — and this table sits in a cache directory for weeks. Only the shape
    is kept, and an assignment is DROPPED rather than truncated: a
    truncated secret is still a leaked prefix.
    """
    rows, _ = _feed(_pair("t1", "AWS_SECRET=hunter2 ./deploy.sh --prod"))

    (row,) = rows
    blob = json.dumps(row)
    assert "hunter2" not in blob and "AWS_SECRET" not in blob, blob
    assert row["shape"] == "./deploy.sh --prod", row


def test_A_SHELL_SCRIPT_STOPS_AT_THE_DASH_C():
    """WHAT FOLLOWS `-c` IS SOMEBODY ELSE'S SCRIPT.

    Keeping a few of its words says nothing about the shape and keeps a
    few words of whatever it contained.
    """
    from jobbox import fingerprint

    assert fingerprint('bash -c "curl -H Authorization:tok https://x"') == "bash -c"
    assert fingerprint("/bin/sh -c ./release") == "sh -c"
    assert fingerprint("make test-all") == "make test-all"


def test_IT_MEASURES_AND_SAYS_NOTHING():
    """A MEASUREMENT THAT SPEAKS CHANGES WHAT IT MEASURES.

    `observe` runs on every shell command. If it emitted a decision, a
    reminder or a rewritten input, it would be answering a question
    nobody has decided yet — and the numbers would describe the world it
    had already changed.
    """
    rows, output = _feed(_pair("t2", "make test"))

    assert output == "", f"observe must print nothing — got {output!r}"
    assert len(rows) == 1


def test_A_DETACHED_CALL_IS_COUNTED_APART():
    """TIME WAITED IS THE COST; TIME DETACHED IS NOT.

    Mixing them would make the total meaningless — a background build
    would look exactly like half an hour of someone sitting still.
    """
    rows, _ = _feed(_pair("t3", "npm run build", background=True)
                    + _pair("t4", "npm run build"))

    detached = [r for r in rows if r["background"]]
    waited = [r for r in rows if not r["background"]]
    assert len(detached) == 1 and len(waited) == 1, rows


def test_A_LONE_HALF_LEAVES_NO_ROW():
    """AN UNPAIRED EVENT IS NOT A ZERO-SECOND CALL.

    A session killed mid-command leaves a start with no end. Writing a
    row anyway would invent a duration; the pending stamp simply stays
    unclaimed, and stays out of the table.
    """
    only_start = _pair("t5", "sleep 300")[:1]
    only_end = _pair("t6", "sleep 300")[1:]

    assert _feed(only_start)[0] == []
    assert _feed(only_end)[0] == []


def test_ONLY_SHELL_COMMANDS_ARE_MEASURED():
    """THE QUESTION IS ABOUT SHELL COMMANDS.

    Every other tool call would pay two process starts for a number
    nobody asked for.
    """
    reading = [{"hook_event_name": "PreToolUse", "tool_name": "Read",
                "tool_use_id": "t7", "tool_input": {"file_path": "/x"}},
               {"hook_event_name": "PostToolUse", "tool_name": "Read",
                "tool_use_id": "t7", "tool_input": {"file_path": "/x"}}]

    assert _feed(reading)[0] == []


def test_GARBAGE_ON_STDIN_DOES_NOT_FAIL():
    """A HOOK THAT ERRORS IS A HOOK THAT GETS DELETED.

    This runs on every shell command, so its worst case has to be
    silence, not a broken turn.
    """
    previous = jobbox.OBSERVED
    try:
        jobbox.OBSERVED = Path("/nowhere/observed.jsonl")
        buffer = io.StringIO()
        with _on_stdin("not json at all"), redirect_stdout(buffer):
            code = jobbox.main(["observe"])
    finally:
        jobbox.OBSERVED = previous

    assert code == jobbox.OK
    assert buffer.getvalue() == ""


def test_THE_TABLE_RANKS_BY_TOTAL_WAITED_NOT_BY_THE_SLOWEST():
    """THE FORTY-SECOND COMMAND RUN THIRTY TIMES.

    That is the trap the whole question is about: each call too short to
    stop for, and their sum is the half-hour. Ranking by the worst single
    call would put a one-off build on top and hide it completely.
    """
    previous = jobbox.OBSERVED
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.OBSERVED = Path(tmp) / "observed.jsonl"
        rows = [{"at": 1.0, "seconds": 300.0, "shape": "big one-off",
                 "background": False}]
        rows += [{"at": 1.0, "seconds": 40.0, "shape": "make test",
                  "background": False} for _ in range(30)]
        jobbox.OBSERVED.write_text(
            "".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
        buffer = io.StringIO()
        try:
            with redirect_stdout(buffer):
                jobbox.main(["timings"])
        finally:
            jobbox.OBSERVED = previous

    out = buffer.getvalue()
    listed = [l for l in out.splitlines() if "make test" in l or "big" in l]
    assert "make test" in listed[0], (
        f"the repeated command must rank first — got {listed}")


def test_THE_FAST_PATH_RUNS_THE_FILE_ITSELF():
    """THE ONLY WAY TO TEST WHAT ACTUALLY RUNS.

    `observe` exits before `click` is imported, because paying that
    import twice per shell command taxes the whole session. Every other
    test here reaches it through `main()` — which is the CLICK path, and
    a different path.

    The first version of the split proved it: the fast path referenced a
    constant defined below the import and died with a traceback on every
    single call. All 43 tests stayed green throughout, because not one of
    them ran the file.

    So this one spawns it, the way a hook does.
    """
    import os
    import subprocess

    with tempfile.TemporaryDirectory() as tmp:
        env = {**os.environ, "JOBBOX_DIR": tmp}
        here = Path(__file__).resolve().parent.parent / "jobbox.py"

        def hook(payload: dict) -> subprocess.CompletedProcess:
            return subprocess.run([sys.executable, str(here), "observe"],
                                  input=json.dumps(payload), text=True,
                                  capture_output=True, env=env)

        start, end = _pair("fast", "make test-all")
        for event in (start, end):
            done = hook(event)
            assert done.returncode == 0, done.stderr
            assert done.stdout == "", done.stdout
            assert done.stderr == "", (
                f"the fast path must not print a traceback — {done.stderr}")

        observed = Path(tmp) / "timings" / "observed.jsonl"
        assert observed.exists(), "the fast path wrote nothing"
        (row,) = [json.loads(l) for l in
                  observed.read_text(encoding="utf-8").splitlines() if l.strip()]
        assert row["shape"] == "make test-all", row

        # AND IT MUST NOT HAVE IMPORTED click — that is the whole reason
        # it exists. Asking the interpreter is cheaper than trusting the
        # placement of a line.
        proof = subprocess.run(
            [sys.executable, "-c",
             "import sys; sys.argv=['jobbox.py','observe'];"
             f"exec(open({str(here)!r}).read());"],
            input=json.dumps(start), text=True, capture_output=True, env=env)
        assert "click" not in proof.stderr, proof.stderr


def test_THE_HARNESS_WRAPPER_IS_NOT_PART_OF_THE_SHAPE():
    """WITHOUT THIS, THE TABLE GROUPS ON ONE TOKEN OF SIGNAL.

    The harness prefixes commands with a `cd` into the working
    directory. The first real capture came back with EVERY shape reading
    `cd /home/… <token>` — so the grouping the whole verb exists for was
    being done on whatever landed third.

    Found by arming the measurement and looking at what it collected,
    which no unit test would have shown: the wrapper is added outside
    jobbox, so nothing in the test data had it until real data did.
    """
    from jobbox import fingerprint

    assert fingerprint("cd /home/user/project && make test") == "make test"
    assert fingerprint("cd /a/b\nmake test-all") == "make test-all"
    # A REAL `cd` IS STILL A COMMAND — dropping it would erase the only
    # thing that call did.
    assert fingerprint("cd /tmp") == "cd /tmp"


def test_A_ROW_SAYS_WHICH_SESSION_MADE_THE_CALL():
    """A DISTRIBUTION FROM ONE SESSION IS NOT THE SAME CLAIM AS FROM SEVERAL.

    The table is machine-wide. A whole reading of it was published —
    bands, thresholds, a recommendation — before anyone noticed it might
    all be one session's calls, and nothing in the row could settle it.
    """
    events = _pair("s1", "make test")
    events[0]["session_id"] = "d4a69872-c959-440e-97c2-adb01be98ba0"

    rows, _ = _feed(events)

    (row,) = rows
    assert row["session"] == "d4a69872", row


def test_A_STAMP_WHOSE_END_NEVER_CAME_IS_EVENTUALLY_FORGOTTEN():
    """NOTHING ELSE REMOVES THEM, and they are not rare.

    Hooks do not fire for every call — measured — so a `PostToolUse`
    that never arrives leaves its stamp behind for good. One file is
    nothing; an unbounded pile in a cache directory is the kind of thing
    noticed in six months.

    The cutoff is deliberately generous: a real command can run for
    hours, and forgetting a live stamp loses a measurement, which is the
    more expensive of the two mistakes.
    """
    import os

    previous = jobbox.PENDING
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.PENDING = Path(tmp) / "pending"
        jobbox.PENDING.mkdir(parents=True)
        old, fresh = jobbox.PENDING / "ancient", jobbox.PENDING / "recent"
        old.write_text("{}", encoding="utf-8")
        fresh.write_text("{}", encoding="utf-8")
        stale = time.time() - jobbox.ORPHAN_AFTER - 60
        os.utime(old, (stale, stale))
        try:
            jobbox._forget_orphans()
            left = sorted(p.name for p in jobbox.PENDING.iterdir())
        finally:
            jobbox.PENDING = previous

    assert left == ["recent"], (
        f"only the stamp with no end coming — got {left}")




def test_THE_DETAILED_READING_RUNS_AT_ALL():
    """IT DID NOT, FOR SIX COMMITS.

    Removing the guard took two constants with it — they sat inside the
    block that was cut — and `timings --detail` has raised `NameError` on
    every call since. Nothing noticed: the suite exercised `_table` and
    the bands' arithmetic, never the verb that puts them together.

    Found by re-reading the documentation and running an example from it.
    That is a slow way to find a crash, and this test is the fast one.
    """
    previous = jobbox.OBSERVED
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.OBSERVED = Path(tmp) / "observed.jsonl"
        rows = [{"at": 1.0, "seconds": s, "shape": f"cmd-{i % 3}",
                 "background": False, "session": "aaaa1111"}
                for i, s in enumerate((0.5, 3.0, 7.0, 20.0, 90.0, 400.0))]
        jobbox.OBSERVED.write_text(
            "".join(json.dumps(r) + "\n" for r in rows), encoding="utf-8")
        buffer = io.StringIO()
        try:
            with redirect_stdout(buffer):
                code = jobbox.main(["timings", "--detail"])
        finally:
            jobbox.OBSERVED = previous

    out = buffer.getvalue()
    assert code == jobbox.OK, out
    assert "how the waiting is spread" in out, out
    assert "what a guard would buy" in out, out
    # EVERY BAND AND EVERY CUTOFF, since a missing constant is exactly
    # what broke it.
    assert "> 60s" in out and "at  60s" in out, out
    # AND THE OUTLIER IS NAMED: one call of 400s out of 520 total.
    assert "CAREFUL" in out, out


def test_THE_DECLARED_TIMEOUT_IS_RECORDED():
    """A DECLARED SIGNAL, NOT A GUESSED ONE.

    Asking for ten minutes is saying you expect several. The
    history-based rule failed because long commands are almost always
    new shapes; a declared timeout needs no history at all.

    It is the branch of that question nobody tested before closing it,
    and nothing can test it until the number is in the table.
    """
    events = _pair("declared", "cd /x && make release")
    events[0]["tool_input"]["timeout"] = 600000

    rows, _ = _feed(events)

    (row,) = rows
    assert row["asked_ms"] == 600000, row
    # AND A CALL THAT DECLARED NOTHING RECORDS A ZERO, not a missing key
    # somebody has to guard against when reading the table.
    plain, _ = _feed(_pair("plain", "cd /x && ls"))
    assert plain[0]["asked_ms"] == 0, plain
