"""jobbox — parsing what `tsp` prints.

────────────────────────────────────────────────────────────────────────
WHAT IS TESTED HERE, AND WHY ONLY THIS
────────────────────────────────────────────────────────────────────────

jobbox is a WRAPPER: `task-spooler` holds the queue, keeps the exit
codes and writes the logs. Testing that would amount to testing `tsp`.

What is ours, and what can be wrong SILENTLY, is reading its table. The
number of columns CHANGES with the state — a waiting job has neither
code nor duration, a running one neither, only a finished one carries
them. A naive split would take the first word of the command for an exit
code, and report "exit=0" on a script that failed.

────────────────────────────────────────────────────────────────────────
THE LINES ARE CAPTURED, NOT RECONSTRUCTED
────────────────────────────────────────────────────────────────────────

They come from a real `tsp` run, copied as-is — including the alignment,
which the length of the output path shifts. A format written from memory
would test my memory instead.
"""
from __future__ import annotations

import sys
from pathlib import Path

# THE MODULE IS NEXT DOOR, NOT INSTALLED. jobbox is a single publishable
# file; we want neither a package nor a `pip install -e` to run its
# tests.
sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

#: THE REAL OUTPUT of `tsp -l`, while one job runs and another waits.
#: Note that neither carries a code nor a duration.
WHILE_RUNNING = (
    "ID   State      Output               E-Level  Times(r/u/s)   "
    "Command [run=1/1]\n"
    "0    running    /tmp/ts-out.EO5neo                           "
    "[first-try]bash -c echo start; sleep 2; echo end\n"
    "1    queued     (file)                                       "
    "[second-try]bash -c echo other; sleep 1; exit 3\n"
)

#: THE SAME, BOTH FINISHED. Here the `E-Level` and `Times` columns
#: appear, which shifts everything after them.
AFTERWARDS = (
    "ID   State      Output               E-Level  Times(r/u/s)   "
    "Command [run=0/1]\n"
    "0    finished   /tmp/ts-out.EO5neo   0        2.00/0.00/0.00 "
    "[first-try]bash -c echo start; sleep 2; echo end\n"
    "1    finished   /tmp/ts-out.fgZ1TA   3        1.00/0.00/0.00 "
    "[second-try]bash -c echo other; sleep 1; exit 3\n"
)


def test_THE_HEADER_IS_NOT_A_JOB():
    """It starts with `ID`, not with a number.

    The dumbest guard in the file, and the one keeping a ghost job out of
    every listing.
    """
    from jobbox import parse

    assert len(parse(WHILE_RUNNING)) == 2


def test_A_RUNNING_JOB_HAS_NO_EXIT_CODE():
    """`None`, AND ABOVE ALL NOT `0`.

    This is THE defect this test exists to catch: on a running job the
    `E-Level` column is EMPTY, and splitting on whitespace would take the
    next word. `bash` would become a code, or worse, a `0` would invent
    itself — and `list` would show "exit=0" on a script that returned
    nothing at all.
    """
    from jobbox import parse

    running, waiting = parse(WHILE_RUNNING)

    assert running["state"] == "running"
    assert running["code"] is None, (
        f"a running job has no code — got {running['code']!r}")
    assert waiting["state"] == "queued"
    assert waiting["code"] is None


def test_A_WAITING_JOB_HAS_NO_LOG_YET():
    """`(file)` MEANS "not yet", and reads as `None`.

    Passing it through would make `tail` try a file named `(file)`, whose
    failure would talk about the file instead of saying the job is
    waiting its turn.
    """
    from jobbox import parse

    _, waiting = parse(WHILE_RUNNING)

    assert waiting["output"] is None


def test_A_FINISHED_JOB_RETURNS_ITS_CODE_AND_DURATION():
    """AND CODE 3 MUST NOT BECOME 0.

    This is the only piece of information saying whether the work
    succeeded. Misreading it would report green on a failure — the worst
    direction for an error.
    """
    from jobbox import parse

    passed, failed = parse(AFTERWARDS)

    assert passed["code"] == 0
    assert passed["duration"] == "2.00/0.00/0.00"
    assert failed["code"] == 3, f"the code must be 3 — got {failed['code']!r}"


def test_THE_INTENT_DETACHES_FROM_THE_COMMAND():
    """IT IS WHAT MAKES `list` READABLE three hours later.

    `tsp` sticks it in brackets in front of the command. Not detaching it
    would leave lines of `[name]bash -c …` where the eye has to hunt for
    the name inside noise.
    """
    from jobbox import parse

    passed, failed = parse(AFTERWARDS)

    assert passed["intent"] == "first-try"
    assert failed["intent"] == "second-try"
    assert passed["command"].startswith("bash -c")
    assert "[" not in passed["command"], "the brackets are stripped"


def test_A_COMMAND_CARRYING_BRACKETS_DOES_NOT_FOOL_IT():
    """THE INTENT IS THE FIRST GROUP, not any of them.

    `grep '[0-9]'` is a perfectly ordinary command. A greedy expression
    would take everything up to the LAST bracket, and the intent would
    swallow half the command.
    """
    from jobbox import parse

    line = ("0    finished   /tmp/ts-out.aaa   0        1.00/0.00/0.00 "
            "[find-digits]grep [0-9] file\n")

    (job,) = parse(line)

    assert job["intent"] == "find-digits"
    assert job["command"] == "grep [0-9] file"


def test_A_NEGATIVE_CODE_IS_READ_TOO():
    """A KILLED JOB RETURNS A NEGATIVE CODE — the signal that stopped it.

    Refusing it would leave `code` at `None`, so "not finished yet" for a
    job that is well and truly dead. `list` would lie about its state.
    """
    from jobbox import parse

    line = ("7    finished   /tmp/ts-out.bbb   -15      0.50/0.00/0.00 "
            "[killed]sleep 999\n")

    (job,) = parse(line)

    assert job["code"] == -15


def test_AN_UNLABELLED_JOB_WHOSE_COMMAND_STARTS_WITH_A_NUMBER():
    """THE ONLY CASE WHERE THE STATE CHECK REALLY MATTERS.

    I thought I had covered it above, and sabotage proved me wrong:
    replacing `if state == "finished"` with `if True` made no test go
    red. The reason is that `rest` ALWAYS starts with `[intent]` — so the
    digit anchor is enough, and the state check is redundant… for a
    labelled job.

    `jobbox run` always labels. But `tsp` can be called by hand, without
    `-L`, and then `rest` is the bare command. A command starting with an
    integer — `7 files`, a numbered script — would have its first word
    read as an EXIT CODE, and a running job would pass for finished.

    The state check is what prevents it, and this test is what says so.
    """
    from jobbox import parse

    line = ("3    running    /tmp/ts-out.ccc                           "
            "7 files to process\n")

    (job,) = parse(line)

    assert job["code"] is None, (
        f"a RUNNING job has no code, even if its command starts with a "
        f"number — got {job['code']!r}")
    assert job["command"] == "7 files to process"
    assert job["intent"] == "", "no label, and that is legitimate"


def test_AN_EMPTY_OUTPUT_DOES_NOT_RAISE():
    """A daemon just born has nothing to list."""
    from jobbox import parse

    assert parse("") == []
    assert parse("ID   State      Output\n") == []


def test_THE_SOCKET_IS_SHORT_AND_OUTSIDE_THE_REPOSITORY():
    """BECAUSE `tsp` SEGFAULTS IF IT IS TOO LONG.

    Measured: it prints "Probably, the name is too long" then drops a
    core. A Unix socket is capped at ~108 characters, and the default
    path follows `TMPDIR` — which the caller chooses, so possibly long.

    This test guards the only thing keeping the tool from killing itself:
    the socket does NOT go under the repository, and its path stays
    short.
    """
    from jobbox import ROOT, SOCKET

    assert len(str(SOCKET)) < 100, (
        f"a Unix socket is capped at ~108 characters — {SOCKET}")
    assert not str(SOCKET).startswith(str(ROOT)), (
        "the socket must not follow `JOBBOX_DIR`, whose path is free — "
        "and therefore possibly too long")


def test_A_BAD_THRESHOLD_DOES_NOT_KILL_THE_TOOL():
    """`JOBBOX_MUTE_AFTER=abc` USED TO BE A TRACEBACK ON EVERY VERB.

    The value was read with a bare `float()` at import time, so a typo in
    a display threshold made `jobbox --help` itself fail. A threshold is
    not worth refusing to start over.
    """
    import os

    import jobbox

    before = os.environ.get("JOBBOX_MUTE_AFTER")
    try:
        os.environ["JOBBOX_MUTE_AFTER"] = "abc"
        assert jobbox._mute_after() == 600.0
        os.environ["JOBBOX_MUTE_AFTER"] = "30"
        assert jobbox._mute_after() == 30.0
    finally:
        if before is None:
            os.environ.pop("JOBBOX_MUTE_AFTER", None)
        else:
            os.environ["JOBBOX_MUTE_AFTER"] = before


def test_THE_QUEUE_WIDTH_IS_READ_FROM_THE_HEADER():
    """ALREADY ON SCREEN — no second round trip to `tsp -S`.

    It matters because the queue holds ONE slot by default, so jobs run
    strictly one after another. Invisible while one person used it; with
    several clients sharing the queue it is the first question anyone
    asks — "why has my job not started".
    """
    from jobbox import slots

    assert slots(WHILE_RUNNING) == (1, 1)
    assert slots(AFTERWARDS) == (0, 1)
    assert slots("ID   State      Output   Command [run=2/4]\n") == (2, 4)
    # NO HEADER, NO GUESS. Returning (0, 1) here would report a width we
    # never read.
    assert slots("") is None


def test_THE_VERSION_MATCHES_THE_CHANGELOG():
    """TWO PLACES HOLDING ONE NUMBER IS HOW THEY COME TO DISAGREE.

    `jobbox --version` exists so an installed copy can say which one it
    is. That is worth nothing if it drifts from the release notes, and
    nothing about a stale constant fails on its own — it just answers
    confidently and wrongly.
    """
    import re

    from jobbox import VERSION

    changelog = (Path(__file__).resolve().parent.parent / "CHANGELOG.md")
    newest = re.search(r"^## \[(\d+\.\d+\.\d+)\]",
                       changelog.read_text(encoding="utf-8"), re.M)

    assert newest, "no released version found in CHANGELOG.md"
    assert VERSION == newest.group(1), (
        f"jobbox says {VERSION}, the changelog's newest release is "
        f"{newest.group(1)}")


def test_THE_DEFAULT_WIDTH_IS_HALF_THE_CORES_AND_OVERRIDABLE():
    """A QUEUE IS THERE TO BOUND WHAT RUNS AT ONCE.

    One slot serialised everything — never discussed, and only visible
    once several clients shared the queue. Unbounded would let N clients
    start N heavy jobs and defeat the point of queueing. Half leaves the
    machine usable while the work runs.

    A bad override says so and falls back: refusing to start over a
    parallelism setting would be the same defect as the muteness
    threshold used to have.
    """
    import os

    from jobbox import wanted_slots

    before = os.environ.get("JOBBOX_SLOTS")
    try:
        from jobbox import UNCAPPED
        for value, expected in (("4", 4), ("1", 1), ("auto", None),
                                ("", None), ("nonsense", None), ("-3", None),
                                # `none`, `off` and `0` all mean no cap —
                                # the position that a queue which makes
                                # anyone wait defeats the point.
                                ("none", UNCAPPED), ("off", UNCAPPED),
                                ("0", UNCAPPED)):
            os.environ["JOBBOX_SLOTS"] = value
            got = wanted_slots()
            if expected is None:
                assert got == max(1, (os.cpu_count() or 2) // 2), (value, got)
            else:
                assert got == expected, (value, got)
    finally:
        if before is None:
            os.environ.pop("JOBBOX_SLOTS", None)
        else:
            os.environ["JOBBOX_SLOTS"] = before


def test_A_LISTING_HAS_HEADINGS_AND_DROPS_EMPTY_COLUMNS():
    """A COLUMN NOBODY FILLED IS WORSE THAN NO COLUMN.

    The muteness column is empty almost always. Printing its heading
    anyway teaches the eye to skip the place where the warning will one
    day appear — and that warning is the only reason the column exists.

    The client column is the same shape: on a single-session machine it
    would repeat one word forever.
    """
    import io
    from contextlib import redirect_stdout

    import jobbox

    buffer = io.StringIO()
    with redirect_stdout(buffer):
        jobbox._table(("id", "state", "intent", "exit", "", "client"),
                      [("0", "finished", "build", "0", "", ""),
                       ("10", "running", "sweep", "", "", "")])
    lines = buffer.getvalue().splitlines()

    assert lines[0].split() == ["id", "state", "intent", "exit"], lines[0]
    assert "client" not in lines[0], "nobody filled it"
    # AND THE IDS LINE UP, because they are numbers.
    assert lines[1].index("finished") == lines[2].index("running"), lines
