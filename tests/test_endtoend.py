"""The one thing the other tests cannot fake: a real `tsp`.

────────────────────────────────────────────────────────────────────────
WHY THIS FILE EXISTS
────────────────────────────────────────────────────────────────────────

`TS_ONFINISH` is the hinge of the whole notification chain, and until now
it was checked by hand. Its manual says the program is run "by the client
after the job", while `jobbox run` returns immediately — so there was a
legitimate doubt about whether it fires at all.

A doubt verified once, by hand, is a doubt that comes back. This runs the
real chain: a real daemon, a real job, `tsp` calling `jobbox-onfinish`
itself, and the signal read back afterwards.

────────────────────────────────────────────────────────────────────────
IT DOES NOT TOUCH THE LIVE QUEUE
────────────────────────────────────────────────────────────────────────

`JOBBOX_SOCKET` and `JOBBOX_DIR` point at a scratch directory, so this
gets its own daemon and its own signals. Without that isolation the test
would queue behind whatever the user is running, and would eat the
endings meant for their session — a test that steals a notification is
worse than no test.

The daemon is killed at the end, whatever happens.
"""
from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import tempfile
import time
from pathlib import Path

HERE = Path(__file__).resolve().parent
JOBBOX = HERE.parent / "jobbox.py"
sys.path.insert(0, str(HERE.parent))


def jobbox_uid(text: str) -> bool:
    import jobbox
    return bool(jobbox._UID.match(text))

#: HOW LONG WE WAIT FOR A `sleep 0`. Generous, but BOUNDED: a test that
#: can hang is a test that gets commented out.
PATIENCE = 20.0


def _run(env: dict, *args: str) -> subprocess.CompletedProcess:
    return subprocess.run([sys.executable, str(JOBBOX), *args],
                          env=env, capture_output=True, text=True)


def _stop(env: dict, socket: Path) -> None:
    """Take the daemon down, and LEAVE NOTHING.

    The first version asked `tsp -K` and unlinked the socket on the next
    line. A daemon whose socket is removed before it has exited survives,
    unreachable, forever — measured: two of them were still running hours
    later, with their sockets and `.error` files beside them.

    So the socket is the daemon's to remove, and we wait for it. Removing
    it ourselves is the last resort, and it is announced rather than done
    quietly: a test that leaks in silence is how this got here.
    """
    subprocess.run(["tsp", "-K"], env={**env, "TS_SOCKET": str(socket)},
                   capture_output=True, check=False)
    for _ in range(40):
        if not socket.exists():
            break
        time.sleep(0.05)
    else:
        print(f"  (daemon on {socket} did not stop — removing its socket)")
        socket.unlink(missing_ok=True)
    # `tsp` LEAVES THIS BESIDE THE SOCKET when a client goes away
    # mid-message. Harmless, and not ours to leave on somebody's machine.
    Path(f"{socket}.error").unlink(missing_ok=True)


def test_A_REAL_JOB_ENDING_REACHES_BOTH_AUDIENCES():
    """THE WHOLE CHAIN, WITHOUT A SINGLE STUB.

    `run` queues, `tsp` runs and calls `jobbox-onfinish` on its own,
    `onfinish` writes one signal per audience, `signals` reads them back
    and erases them.

    What is asserted is what would break silently: the exit code carried
    through (4, not 0), the intent recovered FROM THE QUEUE — `tsp` does
    not pass the label to `TS_ONFINISH`, so it has to be read back — and
    the log path pointing at a file that really holds the output.
    """
    if shutil.which("tsp") is None:
        print("  (no tsp on the PATH — end-to-end skipped)")
        return

    with tempfile.TemporaryDirectory() as tmp:
        # THE SOCKET STAYS OUT of the scratch directory and stays SHORT:
        # `tsp` segfaults on a long path, and a temporary directory is
        # exactly where a path gets long.
        socket = Path("/tmp") / f"jobbox-test-{os.getpid()}.sock"
        env = {**os.environ, "JOBBOX_DIR": tmp, "JOBBOX_SOCKET": str(socket),
               # THE CLIENT MAKES THE FULL ROUND TRIP HERE: written
               # into the label by `run`, read back from the queue by
               # `onfinish`, and used to pick the mailbox `signals`
               # then empties. Nothing else carries it.
               "JOBBOX_CLIENT": "e2e-client"}
        try:
            queued = _run(env, "run", "end-to-end",
                          "bash", "-c", "echo alive; exit 4")
            assert queued.returncode == 0, queued.stderr
            job_id = queued.stdout.strip()
            # `run` PRINTS THE MINTED ID, not tsp's number: it is the one
            # that stays meaningful, and the one every other verb takes.
            assert jobbox_uid(job_id), f"`run` must print a minted id — {job_id!r}"

            deadline = time.time() + PATIENCE
            signals: list[dict] = []
            while time.time() < deadline and not signals:
                time.sleep(0.2)
                seen = _run(env, "signals", "agent", "--json")
                signals = [json.loads(l) for l in seen.stdout.splitlines()
                           if l.strip().startswith("{")]

            assert signals, (
                f"no signal after {PATIENCE}s — TS_ONFINISH did not fire, "
                f"or onfinish wrote nowhere")
            (s,) = signals

            assert s["id"] == job_id, "the signal carries the minted id"
            assert str(s["queue_id"]).isdigit(), s
            assert s["code"] == "4", f"the exit code must survive — got {s!r}"
            assert s["client"] == "e2e-client", (
                f"the client must survive the trip through tsp — got {s!r}")
            assert s["intent"] == "end-to-end", (
                "the intent is not in TS_ONFINISH's arguments; it is read "
                f"back from the queue — got {s!r}")
            assert Path(s["log"]).read_text(encoding="utf-8").strip() == "alive"

            # THE OTHER AUDIENCE STILL HAS ITS COPY. Reading as `agent`
            # must not take what the human has not seen.
            human = _run(env, "signals", "user", "--json")
            assert '"end-to-end"' in human.stdout, human.stdout

            # AND NOTHING SPEAKS TWICE.
            again = _run(env, "signals", "agent", "--json")
            assert again.stdout.strip() == "", again.stdout
        finally:
            _stop(env, socket)


def test_HEALTH_SAYS_WHEN_ASKING_IS_WHAT_STARTED_THE_DAEMON():
    """A QUEUE THAT DIED WITH ITS DAEMON USED TO READ AS AN EMPTY QUEUE.

    `tsp -l` on a fresh socket does not fail: it starts the server and
    returns exit 0. So `health` could never report a dead daemon — it
    resurrected the thing it was asking about, and said "alive".

    What it can see is whether the socket existed BEFORE the question.
    This checks both sides of that: the first call reports the start, the
    second reports an ordinary living daemon.
    """
    if shutil.which("tsp") is None:
        print("  (no tsp on the PATH — end-to-end skipped)")
        return

    with tempfile.TemporaryDirectory() as tmp:
        socket = Path("/tmp") / f"jobbox-health-{os.getpid()}.sock"
        socket.unlink(missing_ok=True)
        env = {**os.environ, "JOBBOX_DIR": tmp, "JOBBOX_SOCKET": str(socket)}
        try:
            first = _run(env, "health")
            assert "STARTED BY THIS CHECK" in first.stdout, first.stdout
            assert first.returncode == 0, first.stderr

            second = _run(env, "health")
            assert "alive" in second.stdout, second.stdout
            assert "STARTED BY THIS CHECK" not in second.stdout, second.stdout
        finally:
            _stop(env, socket)


def test_THE_END_TO_END_TESTS_LEAVE_NOTHING_BEHIND():
    """A TEST THAT LEAKS IN SILENCE IS HOW THE LAST ONE GOT THERE.

    These start real daemons. Two of them were found still running hours
    after a run, each with a socket and an `.error` file beside it,
    because the teardown removed the socket before the daemon had exited.

    Nothing catches that except looking, so this looks.
    """
    if shutil.which("tsp") is None:
        print("  (no tsp on the PATH — end-to-end skipped)")
        return

    leftovers = sorted(str(p) for p in Path("/tmp").glob("jobbox-test-*")
                       ) + sorted(str(p) for p in Path("/tmp").glob("jobbox-health-*"))
    assert leftovers == [], f"the end-to-end tests left these behind: {leftovers}"
