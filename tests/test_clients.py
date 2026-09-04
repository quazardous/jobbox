"""Several clients on one queue — and one mailbox each.

────────────────────────────────────────────────────────────────────────
THE DEFECT THIS FILE EXISTS FOR
────────────────────────────────────────────────────────────────────────

`signals` reads AND erases. That is a good property: it is what makes
each job announced exactly once, with no state kept on the side.

With a single mailbox it is also a theft. Two sessions running at the
same time share one file, so whichever looks first takes the other's
endings — and the loss is invisible, because what is missing is a job
that finished and there is nothing left to see.

The queue itself stays shared on purpose: ordering and parallelism are a
machine-level resource. What is split is the consumption.
"""
from __future__ import annotations

import io
import json
import os
import tempfile
import time
from contextlib import redirect_stdout
from pathlib import Path

import jobbox


def _as(client: str | None):
    """Set `JOBBOX_CLIENT` for the duration of a block."""
    previous = os.environ.get("JOBBOX_CLIENT")

    class _Scope:
        def __enter__(self):
            if client is None:
                os.environ.pop("JOBBOX_CLIENT", None)
            else:
                os.environ["JOBBOX_CLIENT"] = client

        def __exit__(self, *_):
            if previous is None:
                os.environ.pop("JOBBOX_CLIENT", None)
            else:
                os.environ["JOBBOX_CLIENT"] = previous
    return _Scope()


def _drain(client: str, audience: str = "agent") -> str:
    buffer = io.StringIO()
    with _as(client), redirect_stdout(buffer):
        jobbox.main(["signals", audience, "--json"])
    return buffer.getvalue()


def test_ONE_CLIENT_DOES_NOT_STEAL_ANOTHERS_ENDING():
    """THE WHOLE POINT, AND IT USED TO FAIL.

    Two sessions, one job each. The first to look must come back with
    its own, and leave the other's untouched — otherwise a finished job
    is announced to nobody.
    """
    previous = jobbox.SIGNALS
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.SIGNALS = Path(tmp) / "signals"
        for client, job in (("session-a", "1"), ("session-b", "2")):
            box = jobbox._mailbox(client, "agent")
            box.parent.mkdir(parents=True, exist_ok=True)
            box.write_text(json.dumps({"id": job, "code": "0",
                                       "client": client}) + "\n",
                           encoding="utf-8")
        try:
            first = _drain("session-a")
            second = _drain("session-b")
        finally:
            jobbox.SIGNALS = previous

    assert '"id": "1"' in first and '"id": "2"' not in first, first
    assert '"id": "2"' in second, (
        "the second client's ending must survive the first one's read — "
        f"got {second!r}")


def test_OUTSIDE_ANY_SESSION_IT_STILL_WORKS():
    """NO CONFIGURATION, AND NO HARNESS EITHER.

    Run from a plain terminal — no `JOBBOX_CLIENT`, no session id —
    jobbox must behave exactly as it did before clients existed: one
    shared name, everything still delivered. A feature that breaks the
    plain case is not a feature.
    """
    previous_signals = jobbox.SIGNALS
    previous_session = os.environ.pop(jobbox._SESSION_ENV, None)
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.SIGNALS = Path(tmp) / "signals"
        box = jobbox._mailbox(jobbox.UNCLAIMED, "agent")
        box.parent.mkdir(parents=True, exist_ok=True)
        box.write_text(json.dumps({"id": "9", "code": "0"}) + "\n",
                       encoding="utf-8")
        try:
            got = _drain(None)
        finally:
            jobbox.SIGNALS = previous_signals
            if previous_session is not None:
                os.environ[jobbox._SESSION_ENV] = previous_session

    assert '"id": "9"' in got, got


def test_A_HOSTILE_CLIENT_NAME_CANNOT_ESCAPE_ITS_DIRECTORY():
    """A CLIENT NAME BECOMES A DIRECTORY, so it is not free text.

    `../..` would write outside the signal tree, and a `:` would break
    the label the client travels in. Both fall back rather than being
    sanitised silently — the caller is told.

    AN EMPTY VARIABLE IS NOT HOSTILE, it is unset, and it must fall
    through to the automatic naming like any other absence.
    """
    previous_session = os.environ.pop(jobbox._SESSION_ENV, None)
    try:
        for hostile in ("../escape", "a/b", "with:colon", "-leading"):
            with _as(hostile):
                assert jobbox._client() == jobbox.UNCLAIMED, hostile
        with _as(""):
            assert jobbox._client() == jobbox.UNCLAIMED, "empty == unset"
        with _as("session-a.1_x"):
            assert jobbox._client() == "session-a.1_x"

        os.environ[jobbox._SESSION_ENV] = "abcdef01-2345-6789-abcd-ef0123456789"
        with _as("../escape"):
            assert jobbox._client() == jobbox.UNCLAIMED, (
                "a hostile explicit name must NOT silently become the "
                "session name — the caller asked for something precise "
                "and got it wrong")
        with _as(""):
            assert jobbox._client() == "cc-abcdef01", "empty falls through"
    finally:
        os.environ.pop(jobbox._SESSION_ENV, None)
        if previous_session is not None:
            os.environ[jobbox._SESSION_ENV] = previous_session


def test_THE_LABEL_CARRIES_THE_CLIENT_THROUGH_THE_QUEUE():
    """`tsp` PASSES NO ENVIRONMENT TO `TS_ONFINISH`.

    The ending only receives `jobid errorlevel outputfile command`, so
    the client has to survive inside the label — which is the same reason
    the intent does. This checks `parse` reads it back, and that a bare
    label with no client is still an intent, not a client.
    """
    from jobbox import UNCLAIMED, parse

    labelled = ("0    finished   /tmp/ts-out.aaa   0        1.00/0.00/0.00 "
                "[session-a:build-front]npm run build\n")
    (job,) = parse(labelled)
    assert job["client"] == "session-a"
    assert job["intent"] == "build-front"
    assert job["command"] == "npm run build"

    # A JOB QUEUED BY HAND, `tsp -L build` — no client, and the whole
    # label is the intent.
    bare = ("1    finished   /tmp/ts-out.bbb   0        1.00/0.00/0.00 "
            "[build]make\n")
    (job,) = parse(bare)
    assert job["client"] == UNCLAIMED, job
    assert job["intent"] == "build", job

    # AND NO LABEL AT ALL still parses.
    naked = ("2    running    /tmp/ts-out.ccc                           "
             "make check\n")
    (job,) = parse(naked)
    assert job["client"] == UNCLAIMED
    assert job["intent"] == ""


def test_CLIENTS_SHOWS_WHAT_NOBODY_IS_COMING_BACK_FOR():
    """THE FAILURE PER-CLIENT MAILBOXES INTRODUCE.

    A session that ends leaves its endings behind, and an unread file
    looks exactly like an empty one. `clients` is what keeps that from
    being silent.
    """
    previous = jobbox.SIGNALS
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.SIGNALS = Path(tmp) / "signals"
        # THE AGENT'S BOX IS THE PER-CLIENT ONE, so it is the one that
        # can be orphaned. The human's is shared precisely so it cannot.
        box = jobbox._mailbox("gone-session", "agent")
        box.parent.mkdir(parents=True, exist_ok=True)
        box.write_text(json.dumps({"id": "4", "code": "1"}) + "\n",
                       encoding="utf-8")
        buffer = io.StringIO()
        try:
            with _as("session-here"), redirect_stdout(buffer):
                code = jobbox.main(["clients"])
        finally:
            jobbox.SIGNALS = previous

    out = buffer.getvalue()
    assert code == jobbox.OK
    # THE COLUMNS ARE `list`'s COLUMNS NOW: a project, a session, and how
    # many endings are waiting — not a name glued back together.
    assert "gone-session" in out, out
    assert "project" in out and "unread" in out, out
    assert "1" in out.split("gone-session")[1].split("\n")[0], out


def test_A_SESSION_NAMES_ITSELF_WITHOUT_ANY_CONFIGURATION():
    """A DEFAULT YOU MUST SWITCH ON IS A DEFAULT THAT STAYS OFF.

    The harness puts its session id in the environment of the shell it
    runs commands in — measured, not assumed; an earlier note in this
    repository claimed the opposite and left this open. So separation
    happens with nothing configured, and `JOBBOX_CLIENT` is only for
    pinning one fixed name on purpose.
    """
    previous = os.environ.get(jobbox._SESSION_ENV)
    try:
        os.environ[jobbox._SESSION_ENV] = "92183ccf-9dde-432b-8877-000000000000"
        with _as(None):
            assert jobbox._client() == "cc-92183ccf"
        # AN EXPLICIT NAME STILL WINS — that is what pinning means.
        with _as("ci-runner"):
            assert jobbox._client() == "ci-runner"
        # AND WITH NEITHER, the tool behaves as it always did.
        del os.environ[jobbox._SESSION_ENV]
        with _as(None):
            assert jobbox._client() == jobbox.UNCLAIMED
    finally:
        if previous is None:
            os.environ.pop(jobbox._SESSION_ENV, None)
        else:
            os.environ[jobbox._SESSION_ENV] = previous


def test_THE_HUMANS_MAILBOX_IS_NOT_SPLIT():
    """THE HUMAN IS ONE PERSON, and per-session mailboxes would lose them.

    A job launched by a session that has since closed must still be
    announced. Splitting the human's mailbox too would have INTRODUCED
    that loss while fixing the agent's — so the split follows the reader,
    not the file.
    """
    previous = jobbox.SIGNALS
    try:
        jobbox.SIGNALS = Path("/nowhere")
        agent_a = jobbox._mailbox("session-a", "agent")
        agent_b = jobbox._mailbox("session-b", "agent")
        user_a = jobbox._mailbox("session-a", "user")
        user_b = jobbox._mailbox("session-b", "user")
    finally:
        jobbox.SIGNALS = previous

    assert agent_a != agent_b, "two agents must not share a mailbox"
    assert user_a == user_b, (
        "the human reads through whichever session is open — splitting "
        "their mailbox loses the endings of sessions that closed")


def test_EMPTY_MAILBOXES_ARE_FORGOTTEN_WITHOUT_BEING_ASKED():
    """THREE CONDITIONS, AND EACH ONE PREVENTS A LOSS.

    Naming sessions automatically means one mailbox per session, so the
    empty remains pile up and bury the one that still holds something.
    They are now removed with no flag — which makes the guards the whole
    contract:

    **Holding something** is never removed, at any age: it is the only
    evidence that a job finished and nobody was told.

    **Ours** is never removed — churn at best, a race with our own next
    write at worst.

    **Recent** is never removed, and that one is not tidiness. `onfinish`
    creates a client's directory and then opens the file inside it;
    removing it between the two loses that ending, silently.
    """
    import os

    previous = jobbox.SIGNALS
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.SIGNALS = Path(tmp) / "signals"
        for name, lines in (("gone-empty", ""),
                            ("gone-holding", '{"id": "4", "code": "1"}\n'),
                            ("just-born", ""),
                            ("me-here", "")):
            box = jobbox._mailbox(name, "agent")
            box.parent.mkdir(parents=True, exist_ok=True)
            box.write_text(lines, encoding="utf-8")
        # EVERYTHING BUT `just-born` IS OLD ENOUGH to be considered.
        stale = time.time() - jobbox.FORGET_EMPTY_AFTER - 60
        for name in ("gone-empty", "gone-holding", "me-here"):
            folder = jobbox.SIGNALS / name
            os.utime(folder / "agent.jsonl", (stale, stale))
            os.utime(folder, (stale, stale))
        try:
            forgotten = jobbox._forget_empty_mailboxes("me-here")
            left = sorted(p.name for p in jobbox.SIGNALS.iterdir()
                          if p.is_dir())
        finally:
            jobbox.SIGNALS = previous

    assert forgotten == 1, forgotten
    assert left == ["gone-holding", "just-born", "me-here"], (
        f"only the old, empty, foreign one goes — got {left}")


def test_HEALTH_SAYS_WHEN_ANOTHER_MAILBOX_IS_HOLDING_SOMETHING():
    """THE ANSWER TO A QUESTION WE CANNOT SETTLE.

    jobbox does not decide who a client is — the harness does. If it ever
    gives a sub-agent, or a resumed session, a different name from the
    one that comes looking, that mailbox is never drained and a finished
    job is announced to nobody.

    We cannot tell an abandoned mailbox from a merely idle one, and
    draining someone else's on a guess is the exact theft this design
    removed. So we make it VISIBLE instead — from `health`, which is
    where someone already goes when they suspect something, and never
    from a hook that would say it every turn.

    This is what keeps that whole class of question from blocking
    anything: whatever the harness does, a stranded ending is one command
    away from being seen.
    """
    previous = jobbox.SIGNALS
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.SIGNALS = Path(tmp) / "signals"
        for name, lines in (("sub-agent", '{"id": "1"}\n{"id": "2"}\n'),
                            ("me-here", '{"id": "3"}\n'),
                            ("idle-empty", "")):
            box = jobbox._mailbox(name, "agent")
            box.parent.mkdir(parents=True, exist_ok=True)
            box.write_text(lines, encoding="utf-8")
        try:
            held = jobbox._stranded("me-here")
        finally:
            jobbox.SIGNALS = previous

    assert held == [("sub-agent", 2)], (
        f"only other mailboxes that HOLD something — not ours, not the "
        f"empty ones — got {held}")


def test_HEALTH_ITSELF_SAYS_IT_WHEN_NOTHING_ELSE_IS_WRONG():
    """THROUGH THE VERB, NOT THE HELPER — that is the whole point.

    The first version of this call sat past `health`'s early return for
    "no mute job", so it only spoke when a job was ALREADY stuck: never
    in the ordinary case it exists for. The helper's own test passed
    throughout. Running the command is what caught it.
    """
    previous = jobbox.SIGNALS
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.SIGNALS = Path(tmp) / "signals"
        box = jobbox._mailbox("cc-abandoned", "agent")
        box.parent.mkdir(parents=True, exist_ok=True)
        box.write_text('{"id": "1", "code": "0"}\n', encoding="utf-8")
        buffer = io.StringIO()
        try:
            with _as("me-here"), redirect_stdout(buffer):
                jobbox.main(["health"])
        finally:
            jobbox.SIGNALS = previous

    out = buffer.getvalue()
    assert "no mute job" in out, "the ordinary case — nothing is stuck"
    assert "UNREAD" in out and "cc-abandoned" in out, (
        f"health must say it when nothing else is wrong — got {out!r}")


def test_A_CLIENT_NAME_CARRIES_THE_PROJECT_AND_THE_SESSION():
    """IT NEEDS BOTH, AND FOR OPPOSITE REASONS.

    The session id alone is unique and says nothing: on a shared queue
    you cannot tell whose work a job is. The project alone would put two
    windows on one project back in the same mailbox — the theft this
    whole design removed.
    """
    previous = {k: os.environ.get(k) for k in
                ("JOBBOX_CLIENT", "JOBBOX_PROJECT", jobbox._SESSION_ENV)}
    try:
        for k in previous:
            os.environ.pop(k, None)
        os.environ[jobbox._SESSION_ENV] = "92183ccf-9dde-441d-8877-000000000000"

        assert jobbox._client() == "cc-92183ccf", "no project yet"
        os.environ["JOBBOX_PROJECT"] = "BookShepherd"
        assert jobbox._client() == "BookShepherd-92183ccf"
        # AN EXPLICIT PIN STILL WINS over both.
        os.environ["JOBBOX_CLIENT"] = "ci-runner"
        assert jobbox._client() == "ci-runner"
    finally:
        for k, v in previous.items():
            os.environ.pop(k, None)
            if v is not None:
                os.environ[k] = v


def test_A_PROJECT_NAME_IS_REDUCED_NOT_DROPPED():
    """`my project` AND `myproject` ARE DIFFERENT PROJECTS.

    Removing the offending characters instead of replacing them would
    merge their mailboxes, and merging mailboxes is merging
    notifications.
    """
    from jobbox import _sane

    assert _sane("BookShepherd") == "BookShepherd"
    assert _sane("mon projet !") == "mon-projet"
    assert _sane("my project") != _sane("myproject")
    # A NAME THAT CANNOT START A DIRECTORY IS NO NAME AT ALL.
    assert _sane("...") == "" and _sane("") == ""
    assert len(_sane("x" * 100)) <= 32


def test_A_CLIENT_NAME_READS_AS_TWO_COLUMNS():
    """PRESENTATION, NOT INFERENCE: a client IS `project-session`.

    And it cannot lose anything. A name pinned with `--client` has no
    session half, and neither does a job queued outside jobbox — both
    keep their whole name in the project column rather than being cut
    somewhere arbitrary.
    """
    from jobbox import split_client

    assert split_client("BookShepherd-92183ccf") == ("BookShepherd", "92183ccf")
    assert split_client("mon-projet-92183ccf") == ("mon-projet", "92183ccf")
    # `cc` IS NOT A PROJECT — it is the prefix worn before `init` names
    # one, and putting it in a project column answers wrongly rather
    # than not at all.
    assert split_client("cc-92183ccf") == ("", "92183ccf")
    # NO SESSION HALF — kept whole rather than cut at the last dash.
    assert split_client("ci-runner") == ("ci-runner", "")
    assert split_client("default") == ("default", "")
    assert split_client("deploy-nightly") == ("deploy-nightly", "")


def test_TWO_DIRECTORIES_SHARING_A_NAME_ARE_TWO_PROJECTS():
    """OTHERWISE THEIR JOBS SHARE A MAILBOX.

    `~/work/jobbox` and `~/forks/jobbox` are different projects. Letting
    them answer to one name is the same theft as a shared queue, one
    level down — and it would be invisible, because both names look
    right.
    """
    from jobbox import project_tag

    work = project_tag(Path("/home/x/work/jobbox"))
    fork = project_tag(Path("/home/x/forks/jobbox"))

    assert work != fork, (work, fork)
    assert work.startswith("jobbox-") and fork.startswith("jobbox-")
    # STABLE: the same directory must not drift between two calls, or a
    # session would change mailbox under itself.
    assert project_tag(Path("/home/x/work/jobbox")) == work
    # AND A DIRECTORY WHOSE NAME SURVIVES NOTHING still gets a project.
    assert project_tag(Path("/home/x/...")).startswith("project-")


def test_THE_PATH_IS_RECOVERABLE_FROM_THE_TAG():
    """A SHORT TAG IS UNREADABLE THE DAY IT MATTERS.

    It is built short because it sits in every listing — and the day two
    projects share a name is exactly the day you need to know which is
    which. The mapping is the way back, and it is machine-wide because
    `list` shows other sessions' jobs.
    """
    previous = jobbox.PROJECTS
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.PROJECTS = Path(tmp) / "projects.json"
        try:
            jobbox.remember_project("jobbox-1de7", "/home/x/work/jobbox")
            jobbox.remember_project("jobbox-4732", "/home/x/forks/jobbox")
            jobbox.remember_project("", "/ignored")
            known = jobbox.project_paths()
        finally:
            jobbox.PROJECTS = previous

    assert known == {"jobbox-1de7": "/home/x/work/jobbox",
                     "jobbox-4732": "/home/x/forks/jobbox"}


def test_AN_UNREADABLE_MAPPING_COSTS_ONLY_ITSELF():
    """IT IS READ WHILE SOMEBODY IS LISTING A QUEUE.

    A corrupt file must cost the path column, not the listing.
    """
    previous = jobbox.PROJECTS
    with tempfile.TemporaryDirectory() as tmp:
        jobbox.PROJECTS = Path(tmp) / "projects.json"
        jobbox.PROJECTS.write_text("{ truncated", encoding="utf-8")
        try:
            assert jobbox.project_paths() == {}
            jobbox.remember_project("a-1111", "/somewhere")   # must not raise
        finally:
            jobbox.PROJECTS = previous
