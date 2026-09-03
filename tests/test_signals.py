"""Consuming job endings — read AND erase, exactly once.

This is the part that can be wrong SILENTLY. A signal read twice repeats
an announcement; a signal erased without being returned loses it for
good, and nobody will know a job finished. Both failures are mute.

The triggering itself — `TS_ONFINISH` — is not tested here: it needs a
real `tsp` daemon, so it belongs to an end-to-end try, not to this file.
"""
from __future__ import annotations

import io
import json
import tempfile
from contextlib import redirect_stdout
from pathlib import Path

import jobbox


def _with_signals(lines: list[dict], audience: str = "agent") -> tuple[str, bool]:
    """Lay signals down, consume, return (what came out, does the file remain).

    We move `SIGNALS` rather than `ROOT`: it is the only thing the verb
    touches, and a test writing into the user's real cache would destroy
    their pending signals.
    """
    previous = jobbox.SIGNALS
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.SIGNALS = Path(tmp) / "signals"
        # THROUGH `_mailbox`, NOT BY HAND. The layout is
        # `signals/<client>/<audience>` since clients exist; a test that
        # rebuilds the path itself stops testing the real one.
        path = jobbox._mailbox(jobbox._client(), audience)
        path.parent.mkdir(parents=True)
        path.write_text(
            "".join(json.dumps(l, ensure_ascii=False) + "\n" for l in lines),
            encoding="utf-8")
        buffer = io.StringIO()
        try:
            with redirect_stdout(buffer):
                jobbox.main(["signals", audience, "--json"])
        finally:
            remains = path.exists()
            jobbox.SIGNALS = previous
    return buffer.getvalue(), remains


ONE_JOB = {"id": "7", "code": "0", "log": "/tmp/j", "intent": "build",
           "client": "default",
           "command": "make", "finished_at": 1.0}


def test_A_SIGNAL_IS_RETURNED_THEN_ERASED():
    """THE WHOLE GESTURE: what comes out is the fact, and the file is gone."""
    output, remains = _with_signals([ONE_JOB])

    assert '"id": "7"' in output, output
    assert not remains, "the signal must be consumed, not merely read"


def test_SEVERAL_JOBS_ALL_COME_OUT():
    """A QUEUE CAN FINISH THREE BETWEEN TWO LOOKS.

    This is what separates jobbox from the test box, which has one
    verdict per pass. Returning only the last would lose the others.
    """
    lines = [{**ONE_JOB, "id": str(i)} for i in (1, 2, 3)]

    output, _ = _with_signals(lines)

    returned = [json.loads(l)["id"] for l in output.splitlines() if l.strip()]
    assert returned == ["1", "2", "3"], returned


def test_ONE_AUDIENCE_DOES_NOT_CONSUME_THE_OTHER():
    """TWO FILES BECAUSE TWO AUDIENCES.

    That the model has read does not mean the human has seen. If one read
    carried away both, the second audience would stay silent forever —
    and the defect would be invisible, since there is nothing to see.
    """
    previous = jobbox.SIGNALS
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.SIGNALS = Path(tmp) / "signals"
        line = json.dumps(ONE_JOB) + "\n"
        me = jobbox._client()
        for audience in jobbox.AUDIENCES:
            box = jobbox._mailbox(me, audience)
            box.parent.mkdir(parents=True, exist_ok=True)
            box.write_text(line, encoding="utf-8")
        try:
            with redirect_stdout(io.StringIO()):
                jobbox.main(["signals", "agent", "--json"])
            left = [a for a in jobbox.AUDIENCES
                    if jobbox._mailbox(me, a).exists()]
        finally:
            jobbox.SIGNALS = previous

    assert left == ["user"], left


def test_THE_ABSENCE_OF_A_SIGNAL_SAYS_NOTHING():
    """SILENCE IS THE NORMAL CASE, and it must above all not raise.

    This verb is called on EVERY turn by a hook. If it failed when the
    queue returned nothing — that is, almost always — the hook would be
    removed within the day.
    """
    previous = jobbox.SIGNALS
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.SIGNALS = Path(tmp) / "signals"   # never created
        buffer = io.StringIO()
        try:
            with redirect_stdout(buffer):
                code = jobbox.main(["signals", "agent", "--json"])
        finally:
            jobbox.SIGNALS = previous

    assert code == jobbox.OK, code
    assert buffer.getvalue().strip() == "", buffer.getvalue()


def test_AN_UNREADABLE_LINE_DOES_NOT_CARRY_THE_OTHERS_AWAY():
    """A DAMAGED SIGNAL IS AN INCIDENT, NOT A TOTAL LOSS.

    An interrupted write leaves a truncated line. It must cost only
    itself — the neighbouring jobs did finish, and their ending is what
    we came for.
    """
    previous = jobbox.SIGNALS
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.SIGNALS = Path(tmp) / "signals"
        box = jobbox._mailbox(jobbox._client(), "agent")
        box.parent.mkdir(parents=True)
        box.write_text(
            '{"id": "1", "code": "0"}\n{"id": "2", trunca\n'
            '{"id": "3", "code": "0"}\n', encoding="utf-8")
        buffer = io.StringIO()
        try:
            with redirect_stdout(buffer):
                jobbox.main(["signals", "agent"])
        finally:
            jobbox.SIGNALS = previous

    output = buffer.getvalue()
    assert "job 1" in output and "job 3" in output, output


def test_A_SIGNAL_ARRIVING_DURING_THE_READ_IS_NOT_SWALLOWED():
    """THE WINDOW THAT `rename` CLOSES.

    Reading then unlinking left a gap: an ending appended between the two
    was deleted before anyone rendered it. Claiming the file by renaming
    it first means a later ending opens the path afresh and lands in a
    NEW file, which the deletion never touches.

    Here we simulate the concurrent ending by writing to the path again
    while the verb runs — through `emit`, which is called once the file
    has already been claimed.
    """
    previous_signals, previous_emit = jobbox.SIGNALS, jobbox.emit
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.SIGNALS = Path(tmp) / "signals"
        path = jobbox._mailbox(jobbox._client(), "agent")
        path.parent.mkdir(parents=True)
        path.write_text(json.dumps(ONE_JOB) + "\n", encoding="utf-8")

        latecomer = json.dumps({**ONE_JOB, "id": "99"}) + "\n"

        def emit_then_a_job_ends(value):
            # EXACTLY WHAT `onfinish` DOES: open the path, append, close.
            with path.open("a", encoding="utf-8") as f:
                f.write(latecomer)

        try:
            jobbox.emit = emit_then_a_job_ends
            jobbox.main(["signals", "agent", "--json"])
            survived = path.exists() and "99" in path.read_text(encoding="utf-8")
        finally:
            jobbox.SIGNALS, jobbox.emit = previous_signals, previous_emit

    assert survived, (
        "an ending arriving during the read must survive it — otherwise "
        "a finished job is lost, and silently")
