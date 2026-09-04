#!/usr/bin/env python3
"""jobbox — start a long command, and find it again later.

A thin wrapper around `task-spooler`, adding what it does not know:
the intent in plain words, stable paths, and LIVENESS.

    jobbox run <intent> -- <command>    queue it, print the id
    jobbox list                         waiting · running · finished
    jobbox status <id>                  state, exit code, duration, log
    jobbox tail <id> [-f]               the log
    jobbox kill <id>                    stop it
    jobbox health                       is the daemon there, who is mute

────────────────────────────────────────────────────────────────────────
WHY IT EXISTS
────────────────────────────────────────────────────────────────────────

An agent — or a human — waiting on a three-minute script does nothing
for three minutes. Each one is short; it is their sum that costs. The
gesture wanted is "start it, go do something else, come back and look".

`task-spooler` already does the essentials: an ordered queue, one output
file per job, the exit code kept, adjustable parallelism. What is
missing sits above that.

────────────────────────────────────────────────────────────────────────
WHAT IT ADDS, AND NOTHING MORE
────────────────────────────────────────────────────────────────────────

**The intent.** `tsp -l` returns six lines of `bash -c …` that cannot be
read back. A mandatory name costs three words at the moment you have
them in mind, and makes the list useful three hours later.

**Liveness.** `tsp` says whether a job RUNS, never whether it is MAKING
PROGRESS. A stuck script and a computing one are the same "running". So
we read the date of the last byte written to its log: a running job mute
for ten minutes gets named, without any script having to cooperate.

This is deliberately a DATE and not a heartbeat to emit. A heartbeat
would only work for scripts we write ourselves, and whoever forgets it
would look dead. Log freshness simply rewards the one who says where it
is at.

────────────────────────────────────────────────────────────────────────
TWO `tsp` TRAPS, MEASURED, AND THE CODE AVOIDS THEM
────────────────────────────────────────────────────────────────────────

**It SEGFAULTS** when it cannot create its socket: it prints "Probably,
the name is too long", then drops a core. A Unix socket is capped at
~108 characters by the kernel. So we set `TS_SOCKET` short and explicit,
instead of letting it pick a path that kills it.

**`TMPDIR` governs BOTH** — the socket *and* the output files.
Decoupling them is what lets us keep logs wherever we want without
making the socket path longer.

────────────────────────────────────────────────────────────────────────
IT DEPENDS ON NOTHING
────────────────────────────────────────────────────────────────────────

Python and `tsp` on the PATH. No dependency, no configuration, no
database, nothing from the project hosting it — it lives beside it.
"""
from __future__ import annotations

# ONLY WHAT THE MEASURING PATH BELOW ACTUALLY USES.
#
# `observe` runs twice per shell command, and every import it does not
# need is a tax on the whole session. Measured on this machine: these
# four cost 67 ms, and `re`, `shutil`, `subprocess` and `typing` cost 31
# more — for code that path never reaches. They are imported further
# down, after the fast path has already exited.
#
# The test that spawns this file is what keeps the split honest: moving
# an import below something that uses it fails there, loudly, instead of
# on somebody's first shell command.
import json
import os
import sys
import time
from pathlib import Path

# ── THE EXIT CONVENTION ─────────────────────────────────────────────────
#
# Four codes and two verbs. It is the same convention as the neighbouring
# project, and it is COPIED rather than imported: this tool must be able
# to be published on its own.
#
# A convention is not a shared fact. Two copies of a list of values drift
# dangerously; two copies of "2 means misuse" cannot contradict each
# other about data.

#: everything went well
OK = 0
#: the work failed
FAILURE = 1
#: the command was malformed — distinct from failure, so a caller can
#: retry differently
USAGE = 2
#: Ctrl-C
INTERRUPTED = 130

#: THE VERSION, AND `CHANGELOG.md` IS ITS SOURCE.
#:
#: Kept here because a published command must be able to say which one it
#: is — an installed copy is otherwise indistinguishable from any other,
#: and "which version do you have" becomes a conversation. A test asserts
#: this matches the changelog's newest release, because two places
#: holding one number is exactly how they come to disagree.
VERSION = "0.4.0"


#: WHERE THE LOGS LIVE.
#:
#: DECLARED, NEVER COMPUTED BY WALKING UP `..`. The previous version did
#: `__file__ / .. / .. / ..` to land back in the repository hosting it —
#: so it broke as soon as the file moved, and silently: the logs would
#: have gone elsewhere.
#:
#: `JOBBOX_DIR` first, because the caller is the one who knows where they
#: want their files. Otherwise `~/.cache/jobbox`, which is in the user's
#: home and not in a repository: this tool belongs to no project.
ROOT = Path(os.environ.get("JOBBOX_DIR")
            or Path.home() / ".cache" / "jobbox")

# ── MEASURING, AND THE QUESTION IT ALREADY ANSWERED ─────────────────────
#
# This was built to settle whether a guard should force long commands
# into the background. It did, and the answer was no — see "What was
# already ruled out" in CONTRIBUTING.md. Replaying 136 calls, no rule at
# any threshold or granularity recovered more than 0.7 of the 28 minutes
# spent on long commands, because four of the five long shapes had been
# seen exactly once. By the time a rule knows a command is slow, you have
# already waited through it and it does not come back.
#
# THE MEASUREMENT STAYS, the guard did not. It is what answered the
# question, it is what would answer it again if the work changed shape,
# and it costs 0.17 s per shell command to keep.
#
# It measures and does NOTHING ELSE: no decision, no reminder, no
# rewritten input. A measurement that changes what it measures answers a
# different question than the one asked.

#: A LEADING `cd <somewhere>` AND ITS SEPARATOR, which the harness adds.
#: Written without `re` on purpose: this runs before the fast path exits,
#: and `re` is one of the imports that path does not pay for.
def _WRAPPER_sub(text: str) -> str:
    if not text.startswith("cd "):
        return text
    rest = text[3:].lstrip()
    # the directory, then the separator that ends the prefix
    cut = len(rest)
    for sep in ("&&", ";", "\n"):
        found = rest.find(sep)
        if found != -1:
            cut = min(cut, found + len(sep))
    return rest[cut:].lstrip() if cut < len(rest) else text


#: SHELLS, so that `-c` can be recognised as "the rest is a script".
_SHELLS = frozenset(("sh", "bash", "zsh", "dash", "ksh", "fish"))

#: WHERE THE OBSERVED DURATIONS PILE UP.
TIMINGS = ROOT / "timings"

#: One file per in-flight call, named by `tool_use_id` — which the
#: harness reports IDENTICALLY on the before and after events. That is
#: the whole reason the pair can be correlated without keeping state
#: anywhere else.
PENDING = TIMINGS / "pending"

#: The append-only table the decision will be read from.
OBSERVED = TIMINGS / "observed.jsonl"


def fingerprint(command: str) -> str:
    """A COMMAND'S SHAPE, NOT THE COMMAND.

    **We never store the line as typed.** A command line can carry a
    secret — an inline `TOKEN=… ./deploy` is ordinary — and this table
    lives in a cache directory for weeks. Grouping is what the decision
    needs anyway: "which KINDS of command are long", not which exact
    string.

    Three tokens is the grain that separates what matters — `make test`
    from `make test-all`, `git status` from `git push` — while `bash -c`
    collapses everything behind it, which is correct: what follows is
    somebody else's script, and its shape tells us nothing.

    Assignments are dropped rather than truncated: `FOO=bar` is where a
    secret would be, and a truncated secret is still a leaked prefix.
    """
    # THE HARNESS PREFIXES COMMANDS WITH A `cd` INTO THE WORKING
    # DIRECTORY — measured on the first real capture, where every single
    # shape came back as `cd /home/… <token>`. The grouping was then done
    # on one token of actual signal, which is not grouping at all.
    #
    # So a leading `cd` and its argument are dropped, along with whatever
    # separator follows. What is being asked is which COMMAND is long,
    # and the directory it ran in is not part of that.
    command = _WRAPPER_sub(command.strip())
    tokens = [t for t in command.split() if "=" not in t.split("/")[0]]
    # A SHELL WITH `-c` STOPS THE READING THERE. What follows is somebody
    # else's script — it tells us nothing about the shape, and keeping a
    # few words of it is a few words of whatever it contained.
    if len(tokens) > 1 and Path(tokens[0]).name in _SHELLS and tokens[1] == "-c":
        return f"{Path(tokens[0]).name} -c"
    return " ".join(t[:24] for t in tokens[:3]) or "?"


#: BEYOND THIS, A STAMP HAS NO END COMING. Generous: a genuine command
#: can run for hours, and forgetting a live one would lose a real
#: measurement — the failure we are avoiding costs a file, the other
#: costs the data.
ORPHAN_AFTER = 24 * 3600


def _forget_orphans() -> None:
    """Drop stamps whose second half never arrived.

    A CALL WHOSE `PostToolUse` NEVER FIRES leaves its stamp behind, and
    nothing else ever removes it. Measured: hooks do not fire for every
    call, so this is not a rare case — it is a slow, unbounded pile in a
    cache directory, the kind that is noticed in six months.

    It never raises, like everything on this path.
    """
    cutoff = time.time() - ORPHAN_AFTER
    try:
        for stamp in PENDING.iterdir():
            try:
                if stamp.stat().st_mtime < cutoff:
                    stamp.unlink(missing_ok=True)
            except OSError:
                continue
    except OSError:
        pass


def observe(stream, argv: list[str] | None = None) -> int:
    """TIME ONE TOOL CALL. Called by a hook, never by hand.

    Reads the harness's event on standard input and does the only two
    things it can do: stamp a start, or close one out.

    IT PRINTS NOTHING AND NEVER FAILS. This runs on every single tool
    call; a hook that errors is a hook that gets deleted, and one that
    speaks is a banner nobody reads. Its entire output is a line in a
    file.

    THE HARNESS REPORTS NO DURATION — measured, not assumed — so the
    elapsed time is ours to keep. `tool_use_id` is the same value on both
    events, which is what lets the pair meet without a lookup table
    keyed on anything guessable.
    """
    try:
        event = json.load(stream)
    except Exception:  # noqa: BLE001 — see the docstring
        return OK
    try:
        if event.get("tool_name") != "Bash":
            return OK
        key = str(event.get("tool_use_id") or "")
        if not key or "/" in key:
            return OK
        moment = PENDING / key

        if event.get("hook_event_name") == "PreToolUse":
            PENDING.mkdir(parents=True, exist_ok=True)
            _forget_orphans()
            tool_input = event.get("tool_input") or {}
            shape = fingerprint(str(tool_input.get("command") or ""))
            moment.write_text(json.dumps({
                "started": time.time(),
                "shape": shape,
                # ALREADY BACKGROUNDED OR NOT — the single distinction the
                # decision rests on. Time spent waiting is the cost; time
                # spent detached is not.
                "background": bool(tool_input.get("run_in_background")),
                "agent": str(event.get("agent_type") or ""),
                "session": str(event.get("session_id") or "")[:8],
            }, ensure_ascii=False), encoding="utf-8")
            return OK

        started = json.loads(moment.read_text(encoding="utf-8"))
        moment.unlink(missing_ok=True)
        TIMINGS.mkdir(parents=True, exist_ok=True)
        with OBSERVED.open("a", encoding="utf-8") as f:
            f.write(json.dumps({
                "at": started["started"],
                "seconds": round(time.time() - started["started"], 2),
                "shape": started["shape"],
                "background": started["background"],
                "agent": started.get("agent", ""),
                # WHOSE CALL THIS WAS. The table is machine-wide, and
                # without this there is no way to tell whether a reading
                # covers everyone or one session — a whole distribution
                # was published before anyone noticed it might be a
                # single session's.
                "session": started.get("session", ""),
            }, ensure_ascii=False) + "\n")
    except Exception:  # noqa: BLE001
        pass
    return OK


# ── THE MEASURING PATH EXITS BEFORE THE REST OF THE FILE ────────────────
#
# `observe` fires on EVERY shell command, twice, to append one line to a
# file. Everything below — argument parsing, the queue, the harness — is
# work it never needs, and this instrument exists to give time back, not
# to take it.
#
# This shortcut used to exist to dodge `click`, which cost 67 ms on its
# own. That dependency is gone; the shortcut stayed, because skipping
# imports you do not need is worth doing on its own terms.
#
# `main` takes the same path for the same reason, so there is one
# implementation and not two.
if __name__ == "__main__" and sys.argv[1:2] == ["observe"]:
    sys.exit(observe(sys.stdin, sys.argv[2:]))


# ── FROM HERE ON, THE FULL TOOL ─────────────────────────────────────────
#
# Reached only when the command is not `observe`, so these imports are
# paid by the person who typed a verb — never by a hook.
import argparse
import hashlib
import re
import secrets
import shutil
import subprocess
from typing import Any




def say(message: str) -> None:
    """A DIAGNOSTIC — on the error stream, always.

    Everything that is not the RESULT goes through here. The name is
    short on purpose: a convention written twenty times must cost less
    than `print(..., file=sys.stderr)`.
    """
    print(message, file=sys.stderr)


def emit(value: Any) -> None:
    """THE RESULT — on standard output, and nothing else goes there."""
    print(value)


class Refusal(Exception):
    """A REFUSAL THE CALLER CAN READ — not a traceback.

    Raised where the tool declines before doing anything: a missing
    `tsp`, a socket long enough to kill it, a queue asked to hold less
    than one job. `main` turns it into a sentence and the misuse code.
    """


def apply_verbosity(quiet: int = 0, verbose: int = 0) -> None:
    """Here, verbosity only sets a display threshold for later.

    It is accepted so the interface matches the neighbouring tools; this
    module has no narrative to silence yet.
    """
    return None


#: THE SOCKET, SHORT AND PER USER.
#:
#: Capped at ~108 characters by the kernel, and `tsp` SEGFAULTS beyond
#: that instead of refusing cleanly — measured. So it cannot live under
#: `ROOT`, whose path is free and may be long: that is the whole point of
#: putting it aside.
#:
#: `JOBBOX_SOCKET` OVERRIDES IT, and that is what makes an isolated
#: instance possible — a second queue, or an end-to-end test that does
#: not disturb the live one. Without it there is exactly one queue per
#: user, and no way to exercise the `TS_ONFINISH` chain without firing
#: real notifications at whoever is working.
SOCKET = Path(os.environ.get("JOBBOX_SOCKET")
              or f"/tmp/jobbox-{os.getuid()}.sock")  # noqa: S108

#: THE KERNEL'S CAP ON A UNIX SOCKET PATH, minus a margin. Beyond it
#: `tsp` segfaults rather than refusing, so we refuse for it.
SOCKET_MAX = 100


def _mute_after() -> float:
    """BEYOND THIS, A RUNNING JOB IS SUSPECT.

    Not a verdict: the threshold from which `health` NAMES it, so that
    someone goes and looks.

    READ THROUGH A FUNCTION, AND GUARDED. A bare `float(os.environ[...])`
    at import time turned `JOBBOX_MUTE_AFTER=abc` into a traceback on
    every single verb, `--help` included — the tool refused to start over
    a display threshold. A bad value now says so and falls back.
    """
    raw = os.environ.get("JOBBOX_MUTE_AFTER")
    if not raw:
        return 600.0
    try:
        return float(raw)
    except ValueError:
        say(f"  JOBBOX_MUTE_AFTER={raw!r} is not a number — using 600s")
        return 600.0


#: WHERE JOB ENDINGS PILE UP, WAITING TO BE READ.
#:
#: One file per CLIENT and per AUDIENCE — `signals/<client>/<audience>`.
#:
#: Per audience, because that the model has read does not mean the human
#: has seen; they do not read at the same moment nor through the same
#: channel.
#:
#: Per client, because READING ERASES. Two sessions sharing one file
#: means the first one to look takes the other's endings, and the theft
#: is invisible: what is missing is a job that finished, and there is
#: nothing left to notice.
SIGNALS = ROOT / "signals"

#: THE AUDIENCES, DECLARED ONCE. Adding a channel happens here.
AUDIENCES = ("agent", "user")

#: THE AUDIENCES THAT ARE **NOT** SPLIT PER CLIENT.
#:
#: THE HUMAN IS ONE PERSON. They want every ending, whichever session
#: started it, and they read through whatever session happens to be open.
#: Splitting their mailbox too would mean a job launched by a session
#: that has since closed is announced to nobody — a loss that per-client
#: mailboxes would have INTRODUCED while fixing the agent's.
#:
#: The agent is the opposite: it only cares about what it launched, and
#: one agent consuming another's endings is the actual defect.
#:
#: So the split follows the reader, not the file. Both are still consumed
#: exactly once.
SHARED_AUDIENCES = ("user",)

def _sane(name: str) -> str:
    """A project name reduced to what a directory and a label can hold.

    Anything outside the allowed set becomes a dash rather than being
    dropped: `my project` and `myproject` are different projects, and
    silently merging their mailboxes would merge their notifications.
    """
    kept = "".join(c if (c.isalnum() or c in "._-") else "-" for c in name)
    kept = kept.strip("-.")[:32]
    return kept if kept[:1].isalnum() else ""


#: A CLIENT NAME IS A DIRECTORY AND HALF A LABEL, so it is restricted:
#: no `/` that would escape the directory, no `:` that would break the
#: label apart.
_CLIENT_OK = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,63}$")

#: WHOSE ENDINGS THESE ARE when nobody said. A job started through `tsp`
#: by hand has no client, and it still has to end somewhere nameable.
UNCLAIMED = "default"


#: THE HARNESS ALREADY NAMES ITS SESSIONS, and it puts the name in the
#: environment of the shell it runs commands in — measured, not assumed.
#: An earlier note in this repository claimed the opposite and left
#: per-session identity as an open question; one `env` call settled it.
#:
#: We keep a short prefix and the leading hex so `clients` stays readable:
#: a full UUID per line turns that listing into a wall.
_SESSION_ENV = "CLAUDE_CODE_SESSION_ID"


def _client() -> str:
    """WHO IS ASKING — one queue for everyone, one mailbox each.

    THE QUEUE STAYS SHARED, DELIBERATELY. Ordering and parallelism are a
    MACHINE-level resource: that is the whole reason `task-spooler`
    exists. Giving each client its own daemon would let N clients start N
    heavy jobs at once, which is the problem the queue was there to
    prevent.

    What must not be shared is the CONSUMPTION of endings. `signals`
    reads and erases in one gesture — a property worth keeping, since it
    is what makes each job announced exactly once — but with a single
    mailbox it also means the first client to look blinds every other.

    `JOBBOX_CLIENT` names a session explicitly, and wins when set — it is
    how a CI runner or a shared worker pins one fixed mailbox on purpose.

    Otherwise the name is built from the PROJECT and the SESSION, and it
    needs both. The session id alone — `cc-92183ccf` — is unique and says
    nothing: on a shared queue you cannot tell whose work a job is. The
    project alone would put two windows on one project back in the same
    mailbox, which is the theft this whole design removed.

    `JOBBOX_PROJECT` is written once by `init`, into the settings file,
    and never computed from the working directory. Deriving it at call
    time would rename the client whenever a command ran from a
    subdirectory — splitting the mailbox mid-session and stranding
    everything already in it.
    """
    raw = os.environ.get("JOBBOX_CLIENT")
    if not raw:
        session = (os.environ.get(_SESSION_ENV) or "")[:8]
        project = _sane(os.environ.get("JOBBOX_PROJECT") or "")
        if project and session:
            raw = f"{project}-{session}"
        elif session:
            raw = f"cc-{session}"
        else:
            raw = project or UNCLAIMED
    if not _CLIENT_OK.match(raw):
        say(f"  JOBBOX_CLIENT={raw!r} is not a plain name "
            f"(letters, digits, `.`, `_`, `-`) — using {UNCLAIMED!r}")
        return UNCLAIMED
    return raw


#: WHERE A PROJECT NAME IS TRACED BACK TO ITS DIRECTORY.
#:
#: Machine-wide, like the queue: `list` shows other sessions' jobs, and
#: their paths are not in this session's environment.
PROJECTS = ROOT / "projects.json"


def project_tag(path: Path) -> str:
    """A project's name, made unique by where it lives.

    TWO DIRECTORIES CAN SHARE A NAME. `~/work/jobbox` and
    `~/forks/jobbox` are different projects, and letting them answer to
    one name would put their jobs in one mailbox — the same theft as a
    shared queue, one level down.

    Four hex characters of the path's digest separate them. Short on
    purpose: it sits in every listing, and `--project-path` is there for
    the moment it stops being readable.
    """
    digest = hashlib.sha256(str(path).encode("utf-8")).hexdigest()[:4]
    named = _sane(path.name) or "project"
    return f"{named}-{digest}"


def remember_project(tag: str, path: str) -> None:
    """Record which directory a project tag stands for. Never raises."""
    if not tag or not path:
        return
    try:
        known = {}
        if PROJECTS.exists():
            known = json.loads(PROJECTS.read_text(encoding="utf-8"))
        if known.get(tag) == path:
            return
        known[tag] = path
        PROJECTS.parent.mkdir(parents=True, exist_ok=True)
        PROJECTS.write_text(json.dumps(known, indent=1, ensure_ascii=False),
                            encoding="utf-8")
    except (OSError, ValueError):
        pass


def project_paths() -> dict[str, str]:
    try:
        return json.loads(PROJECTS.read_text(encoding="utf-8"))
    except (OSError, ValueError):
        return {}


#: THE SESSION HALF OF A CLIENT NAME: exactly the eight hex characters
#: `_client` takes from the harness's session id.
_SESSION_HALF = re.compile(r"^[0-9a-f]{8}$")


def split_client(client: str) -> tuple[str, str]:
    """A client name as (project, session) — for reading, not for keys.

    A client IS `project-session` by construction, so this is
    presentation rather than inference. It cannot lose anything: when the
    tail is not a session id — a name pinned with `--client`, or a job
    queued outside jobbox — the whole thing is the project and the
    session is empty.
    """
    project, _, tail = client.rpartition("-")
    if project and _SESSION_HALF.match(tail):
        return project, tail
    return client, ""


def _mailbox(client: str, audience: str) -> Path:
    """Where endings wait — per client, or shared, depending on the reader.

    A shared audience sits at the top of the tree and a per-client one
    under its client's directory, so the layout on disk says which is
    which without anybody having to look it up.
    """
    if audience in SHARED_AUDIENCES:
        return SIGNALS / f"{audience}.jsonl"
    return SIGNALS / client / f"{audience}.jsonl"


#: BEFORE A MAILBOX IS OLD ENOUGH TO FORGET.
#:
#: Not a tidiness setting — a RACE GUARD. `onfinish` creates a client's
#: directory and then opens the file inside it; removing it between the
#: two loses that ending, silently, which is the exact failure this whole
#: design exists to prevent. An hour puts any live writer far outside the
#: window.
FORGET_EMPTY_AFTER = 3600


def _forget_empty_mailboxes(me: str) -> int:
    """Remove the empty leavings of sessions that are gone.

    THREE CONDITIONS, AND EACH ONE PREVENTS A LOSS.

    **Empty.** A mailbox holding an ending is the only evidence that a
    job finished and nobody was told. It is never removed, at any age.

    **Not ours.** Removing our own would be churn at best and a race with
    our own next write at worst.

    **Not recent.** See `FORGET_EMPTY_AFTER` — this is what keeps the
    removal from stepping on a writer mid-gesture.

    It never raises: it runs from the ending of somebody's job, and
    failing there would soil a job that went fine.
    """
    if not SIGNALS.is_dir():
        return 0
    cutoff = time.time() - FORGET_EMPTY_AFTER
    forgotten = 0
    try:
        folders = list(SIGNALS.iterdir())
    except OSError:
        return 0
    for folder in folders:
        try:
            if not folder.is_dir() or folder.name == me:
                continue
            if folder.stat().st_mtime > cutoff:
                continue
            boxes = list(folder.iterdir())
            if any(b.stat().st_size for b in boxes):
                continue                    # it still holds something
            for box in boxes:
                box.unlink(missing_ok=True)
            folder.rmdir()
            forgotten += 1
        except OSError:
            continue
    return forgotten


def _take(client: str, audience: str) -> list[dict[str, Any]]:
    """EMPTY ONE MAILBOX — read and erase, in a single gesture.

    THE ONLY PLACE THAT CONSUMES. Both `signals` and `claude-hook` need
    it, and two copies of "read then erase" would drift at the first edit
    of one — silently, because what drifts is a job ending that stops
    arriving, and a missing announcement looks exactly like a quiet day.

    IT CLAIMS BY RENAMING. Reading then unlinking left a window: an
    `onfinish` appending between the two had its line deleted before
    anyone saw it. `rename` is atomic — after it, a later ending opens the
    path afresh and lands in a new file the deletion never touches.

    IT NEVER RAISES: it is called from a hook on every turn.
    """
    path = _mailbox(client, audience)
    if not path.exists():
        return []
    claimed = path.with_name(f"{audience}.jsonl.taken-{os.getpid()}")
    try:
        path.rename(claimed)
    except OSError:
        return []                      # gone, or someone got it first
    try:
        raw = claimed.read_text(encoding="utf-8")
    except OSError:
        return []
    finally:
        claimed.unlink(missing_ok=True)

    taken = []
    for line in raw.splitlines():
        if not line.strip():
            continue
        try:
            taken.append(json.loads(line))
        except ValueError:
            # A TRUNCATED LINE COSTS ONLY ITSELF. The neighbouring jobs
            # did finish, and their ending is what we came for.
            continue
    return taken

#: THE PROGRAM `tsp` RUNS WHEN A JOB ENDS.
#:
#: `TS_ONFINISH` receives `jobid errorlevel outputfile command`. It is
#: started through execlp, so the path must be absolute.
ONFINISH = Path(__file__).resolve().parent / "jobbox-onfinish"


def _env() -> dict[str, str]:
    """`tsp`'s environment, with its two paths decoupled.

    `TS_ONFINISH` IS WHAT MAKES NOTIFICATION POSSIBLE. Without it,
    knowing a job has finished would mean RE-READING the queue and
    comparing — so keeping state on the side, and keeping it right. Here
    it is `tsp` that comes and tells us, once, at the right moment.
    """
    ROOT.mkdir(parents=True, exist_ok=True)
    return {**os.environ, "TS_SOCKET": str(SOCKET), "TMPDIR": str(ROOT),
            "TS_ONFINISH": str(ONFINISH)}


def wanted_slots() -> int:
    """How wide a queue we open, when we are the ones opening it.

    HALF THE CORES, because a queue is there to bound what runs at once.
    One slot serialised everything, which was never discussed and only
    became visible once several clients shared the queue; unbounded would
    let N clients start N heavy jobs and defeat the point of queueing at
    all. Half leaves the machine usable while the work runs.

    `JOBBOX_SLOTS` overrides it: a number, `auto` for the default, or
    `none` for no cap at all — which is a defensible position, since
    jobbox exists so nobody waits and a cap is a way of waiting. It is
    not the default only because an unbounded queue cannot survive a loop
    that queues fifty jobs, and half the cores is already more than a
    session ever reaches.

    A bad value says so and falls back rather than refusing to start.
    """
    raw = (os.environ.get("JOBBOX_SLOTS") or "").strip().lower()
    if raw in ("none", "off", "0"):
        return UNCAPPED
    if raw and raw != "auto":
        try:
            asked = int(raw)
        except ValueError:
            say(f"  JOBBOX_SLOTS={raw!r} is not a number — using auto")
        else:
            if asked >= 1:
                return asked
            say(f"  JOBBOX_SLOTS={raw!r} — use `none` for no cap")
    return max(1, (os.cpu_count() or 2) // 2)


#: NO CAP, EXPRESSED AS A NUMBER because that is all `tsp` accepts. Big
#: enough that nothing reaches it, small enough to stay a number.
UNCAPPED = 1000

#: SET AT MOST ONCE PER PROCESS, and only for a daemon we started.
_WIDTH_CONSIDERED = False


def _tsp(*args: str, check: bool = False) -> subprocess.CompletedProcess:
    """Talk to `tsp` — and size a queue we are about to bring into being.

    THE WIDTH IS SET ONLY AT THE DAEMON'S BIRTH, never afterwards. Any
    `tsp` call starts the server if it is absent, so "was there a socket
    before this call" is exactly "are we the ones creating it".

    Doing it any later would fight the person who set `jobbox slots 2` on
    purpose: their choice would be silently undone by the next command.
    A default is for a queue nobody has an opinion about yet.
    """
    global _WIDTH_CONSIDERED
    ours = not _WIDTH_CONSIDERED and not SOCKET.exists()
    _WIDTH_CONSIDERED = True
    if ours and args[:1] != ("-S",):
        # BEFORE the real call, not after. Sizing afterwards left the
        # caller holding output captured from the one-slot daemon that
        # existed a moment earlier — `health` announced six slots and
        # reported one on the next line. `tsp -S` on an absent socket
        # starts the daemon and sizes it in the same breath.
        subprocess.run(["tsp", "-S", str(wanted_slots())], env=_env(),
                       capture_output=True, text=True, check=False)
    return subprocess.run(["tsp", *args], env=_env(), check=check,
                          capture_output=True, text=True)


#: `<id> <state> <output> [<code> <times>] <command>`
#:
#: THE NUMBER OF COLUMNS CHANGES WITH THE STATE, which is why we do not
#: split naively: a `queued` job has neither code nor times, a `running`
#: one neither, only a `finished` one carries them. A naive `split()`
#: would take the start of the command for an exit code.
_LINE = re.compile(r"^(?P<id>\d+)\s+(?P<state>\S+)\s+(?P<output>\S+)\s+(?P<rest>.*)$")
#: THE HEADER CARRIES THE PARALLELISM — `Command [run=1/2]`. Reading it
#: costs nothing, where asking `tsp -S` would be a second round trip for
#: something already on screen.
_SLOTS = re.compile(r"\[run=(?P<busy>\d+)/(?P<slots>\d+)\]")


def slots(output: str) -> tuple[int, int] | None:
    """How many jobs run at once, and how many may — or `None`.

    WHY IT IS WORTH SAYING AT ALL. The queue holds one slot by default,
    so jobs run strictly one after another. That was invisible while one
    person used it; with several clients sharing the queue, the first
    question anyone asks is "why has my job not started", and the answer
    is almost always that someone else's is holding the only slot.
    """
    m = _SLOTS.search(output)
    return (int(m["busy"]), int(m["slots"])) if m else None


#: A NAME THIS TOOL MINTS, because `tsp`'s numbers are not stable.
#:
#: The daemon numbers jobs from zero and starts over when it dies, so an
#: id kept from yesterday can name a different job today — and would hand
#: back the wrong log without a word. That is the silent-wrong-answer
#: shape this whole file is written against.
#:
#: A minted id does NOT make the queue survive its daemon; nothing can.
#: It makes a stale reference FAIL, which is the only useful difference:
#: `status j7f3a91c` on a queue that has been recreated says it does not
#: know that job, instead of confidently showing another one.
#:
#: It begins with a letter so it can never be mistaken for one of `tsp`'s
#: numbers — both are accepted wherever a job is named.
_UID = re.compile(r"^j[0-9a-f]{7}$")


def mint() -> str:
    """A short name no other job will carry."""
    return "j" + secrets.token_hex(4)[:7]


#: What `-L` set, as `-l` renders it: `[client:uid:intent]the command`.
#:
#: THE LABEL IS THE ONLY THING THAT REACHES `onfinish`. `tsp` hands it
#: `jobid errorlevel outputfile command` and nothing else — no
#: environment, no label — so whatever the ending needs to know has to
#: travel through the queue itself. That is why the client is written
#: into the label rather than kept in a table on the side: a table
#: indexed by job id would drift the moment `tsp -C` reuses one.
#:
#: The prefix is OPTIONAL, because `tsp -L` can be called by hand: a bare
#: `[build]` is an intent with no client, and lands in `UNCLAIMED`.
_TAG = re.compile(r"^\[(?:(?P<client>[A-Za-z0-9][A-Za-z0-9._-]*):)?"
                  r"(?:(?P<uid>j[0-9a-f]{7}):)?"
                  r"(?P<intent>[^\]]*)\](?P<command>.*)$")


def parse(output: str) -> list[dict[str, Any]]:
    """The lines of `tsp -l`, as records.

    PROVEN ON REAL LINES, captured by running `tsp` — not on a format
    reconstructed from memory. This is the only part of this module that
    can be wrong SILENTLY: a misread column would return a plausible exit
    code for a job that failed.
    """
    jobs = []
    for line in output.splitlines():
        m = _LINE.match(line)
        if not m:
            continue                       # the header, and blank lines
        state, rest = m["state"], m["rest"]
        code: int | None = None
        duration: str | None = None
        if state == "finished":
            # ONLY A FINISHED JOB CARRIES ITS CODE. Both fields are stuck
            # against the command, so we detach them by shape — an
            # integer, then three floats separated by slashes.
            done = re.match(r"^(-?\d+)\s+(\S+)\s+(?P<rest>.*)$", rest)
            if done:
                code = int(done.group(1))
                duration = done.group(2)
                rest = done["rest"]
        tagged = _TAG.match(rest)
        jobs.append({
            "id": int(m["id"]),
            "state": state,
            "output": None if m["output"] == "(file)" else m["output"],
            "code": code,
            "duration": duration,
            "client": (tagged["client"] or UNCLAIMED) if tagged else UNCLAIMED,
            # EMPTY FOR A JOB THIS TOOL DID NOT QUEUE — `tsp -L` by hand,
            # or one from before ids were minted. Such a job can still be
            # named by its number, and only by its number.
            "uid": (tagged["uid"] or "") if tagged else "",
            "intent": tagged["intent"] if tagged else "",
            "command": tagged["command"] if tagged else rest,
        })
    return jobs


def _jobs() -> list[dict[str, Any]]:
    res = _tsp("-l")
    return parse(res.stdout)


def _one(ref: str | int) -> dict[str, Any] | None:
    """Find a job by its minted id, or by `tsp`'s number.

    BOTH ARE ACCEPTED, and they cannot be confused: a minted id starts
    with a letter. The number is what `tsp -l` shows to anyone looking at
    the queue directly, so refusing it would make this tool disagree with
    the thing underneath it.
    """
    ref = str(ref).strip()
    jobs = _jobs()
    if _UID.match(ref):
        return next((j for j in jobs if j["uid"] == ref), None)
    if ref.isdigit():
        return next((j for j in jobs if j["id"] == int(ref)), None)
    return None


def _silence(job: dict[str, Any]) -> float | None:
    """How many seconds since this job wrote anything, or `None`.

    LIVENESS IS READ FROM FILE FRESHNESS, not from a heartbeat the script
    would have to emit. A heartbeat would only have worked for our own
    scripts, and whoever forgets it would look dead.

    This rewards exactly what a well-made script does: the one that says
    where it is at gets precise liveness, for free. The one that stays
    silent gets "I do not know", which is the honest answer.
    """
    path = job.get("output")
    if not path or job["state"] != "running":
        return None
    try:
        return time.time() - Path(path).stat().st_mtime
    except OSError:
        return None


def _require_tsp() -> None:
    """Refuse early and NAME the package, rather than failing further on."""
    if shutil.which("tsp") is None:
        raise Refusal(
            "task-spooler is not installed. It is the one holding the "
            "queue; jobbox is only its wrapper. On Fedora: "
            "`sudo dnf install task-spooler`.")
    # A SEGFAULT TURNED INTO A SENTENCE. `tsp` does not refuse an
    # over-long socket path: it prints "Probably, the name is too long"
    # and drops a core. Since `JOBBOX_SOCKET` lets a caller choose that
    # path, the check has to live where the caller can read it.
    if len(str(SOCKET)) > SOCKET_MAX:
        raise Refusal(
            f"the socket path is {len(str(SOCKET))} characters long; a "
            f"Unix socket is capped at ~108 by the kernel and `tsp` "
            f"segfaults instead of refusing. Shorten JOBBOX_SOCKET: "
            f"{SOCKET}")


def _run(intent: str, command: tuple[str, ...]) -> int:
    """THE INTENT IS MANDATORY, and that is deliberate.

    A queue of six `bash -c …` lines cannot be read back. The name is
    what makes `list` useful three hours later, and requiring it costs
    three words at the moment you have them in mind.
    """
    _require_tsp()
    # THE MAPPING FILLS ITSELF from any session that queues a job, so it
    # is not lost with a settings file and does not need `init` to have
    # been re-run.
    remember_project(_sane(os.environ.get("JOBBOX_PROJECT") or ""),
                     os.environ.get("JOBBOX_PROJECT_PATH") or "")
    uid = mint()
    res = _tsp("-L", f"{_client()}:{uid}:{intent}", *command)
    if res.returncode != 0:
        say(f"  {res.stderr.strip() or 'tsp refused the command'}")
        return FAILURE
    # THE MINTED ID IS WHAT WE PRINT, because it is the one that stays
    # meaningful. `status` takes `tsp`'s number too, for anyone reading
    # the queue directly.
    emit(uid)
    return OK


def _list(mine: bool, full_path: bool = False) -> int:
    """THE QUEUE IS SHOWN WHOLE BY DEFAULT, and that is deliberate.

    It is ONE queue for the whole machine. Hiding other sessions' jobs
    would make a full queue look empty, and someone would wonder why
    their own job never starts while six of somebody else's run.

    `--mine` narrows to this session. `--all` is the default written out
    — it adds nothing, and it exists because a default worth relying on
    should be typeable.

    THERE IS NO NOTION OF A PROJECT HERE, and that is not an oversight:
    the queue is a machine-level resource and a client is a SESSION, not
    a directory. Two sessions open on the same project are two clients.
    """
    _require_tsp()
    me = _client()
    jobs = [j for j in _jobs() if not mine or j["client"] == me]
    if not jobs:
        say("  the queue is empty" if not mine
            else f"  nothing queued by {me!r}")
        return OK
    threshold = _mute_after()

    rows = []
    for j in jobs:
        mute = _silence(j)
        rows.append((
            # THE MINTED ID IS THE ONE TO TYPE. A job queued outside
            # jobbox has none, and falls back to the number — which is
            # all it ever had.
            j["uid"] or str(j["id"]),
            j["state"],
            j["intent"],
            "" if j["code"] is None else str(j["code"]),
            # MUTENESS IS ONLY SAID WHEN IT MATTERS. On every line it
            # would be a column people stop reading — and it is precisely
            # the one that must be seen the day it speaks.
            f"MUTE {int(mute)}s"
            if mute is not None and mute > threshold else "",
            # ALWAYS SHOWN, even when it is yours and repeats down the
            # column. Hiding it made blank mean "mine" — a convention the
            # reader has to hold, against a heading that explains itself.
            # A job queued outside jobbox shows the mailbox it lands in.
            *split_client(j["client"]),
        ))
    if full_path:
        # THE TAG IS BUILT TO BE SHORT, which makes it unreadable the day
        # two projects share a name — exactly the day it matters. This is
        # the way back.
        known = project_paths()
        rows = [r[:5] + (known.get(r[5], r[5]),) + r[6:] for r in rows]
    _table(("id", "state", "intent", "exit", "", "project", "session"), rows)
    return OK


def _table(headings: tuple[str, ...], rows: list[tuple[str, ...]]) -> None:
    """Columns wide enough for what is in them, and a heading over each.

    WRITTEN HERE RATHER THAN PULLED IN. jobbox has no dependencies, and a
    table library would be a whole one for twelve lines of arithmetic —
    the same trade that `click` lost.

    A COLUMN NOBODY FILLED IS NOT PRINTED. The muteness column is empty
    almost always, and an empty column with a heading is worse than none:
    it teaches the eye to skip the place where the warning will appear.
    """
    # A COLUMN IS KEPT ONLY IF SOMETHING FILLS IT — having a heading is
    # not enough. The first version tested the heading, which is the
    # opposite of what the paragraph above promises, and printed an empty
    # `client` column on a single-session machine.
    kept = [i for i in range(len(headings)) if any(r[i] for r in rows)]
    if not kept:
        return
    width = [max(len(headings[i]), *(len(r[i]) for r in rows)) for i in kept]
    # THE ID IS RIGHT-ALIGNED because it is a number and they line up;
    # everything else reads from the left.
    def line(cells):
        out = []
        for n, i in enumerate(kept):
            out.append(cells[i].rjust(width[n]) if headings[i] == "id"
                       else cells[i].ljust(width[n]))
        return "  " + "  ".join(out).rstrip()
    emit(line(headings))
    for row in rows:
        emit(line(row))


def _status(ref: str) -> int:
    _require_tsp()
    j = _one(ref)
    if j is None:
        say(f"  job {ref} unknown to the queue")
        return FAILURE
    emit(f"  id         {j['uid'] or '(queued outside jobbox)'}")
    emit(f"  queue id   {j['id']}   — tsp's number, reused after a restart")
    emit(f"  intent     {j['intent']}")
    emit(f"  state      {j['state']}")
    emit(f"  command    {j['command']}")
    if j["code"] is not None:
        emit(f"  exit       {j['code']}")
    if j["duration"]:
        emit(f"  times      {j['duration']}")
    if j["output"]:
        emit(f"  log        {j['output']}")
    mute = _silence(j)
    if mute is not None:
        emit(f"  last wrote {int(mute)}s ago")
    # THE JOB'S EXIT CODE BECOMES OURS. A script calling `jobbox status`
    # can therefore decide without parsing any text.
    return OK if not j["code"] else FAILURE


def _tail(ref: str, follow: bool, lines: int) -> int:
    """WE DELEGATE TO `tail`, AND THAT IS THE POINT.

    "just tool the tail of the log". `tsp`'s output is an ordinary FILE —
    so `tail`, `grep`, `less` all work on it, and there is neither a
    format to parse nor a wrapper to maintain. We hand the path to the
    tool that knows how to read it.
    """
    _require_tsp()
    j = _one(ref)
    if j is None:
        say(f"  job {ref} unknown to the queue")
        return FAILURE
    if not j["output"]:
        say(f"  job {ref} has no log yet — it is waiting its turn "
            f"in the queue")
        return OK
    args = ["tail", "-n", str(lines), *(["-f"] if follow else []), j["output"]]
    return subprocess.run(args, check=False).returncode


def _kill(ref: str) -> int:
    _require_tsp()
    # `tsp` ONLY KNOWS ITS OWN NUMBER, so a minted id is resolved first.
    j = _one(ref)
    if j is None:
        say(f"  job {ref} unknown to the queue")
        return FAILURE
    res = _tsp("-k", str(j["id"]))
    if res.returncode != 0:
        say(f"  {res.stderr.strip() or 'tsp refused'}")
        return FAILURE
    # WE SAY IT, BECAUSE THE CODE WILL NOT. A killed job returns an
    # interrupt code that a later reader would take for a failure of the
    # script. The trace lives in the log, next to its output.
    say(f"  job {ref} stopped")
    return OK


def _onfinish(job_id: str, code: str, logfile: str,
              command: tuple[str, ...]) -> int:
    """`tsp` TELLS US A JOB HAS FINISHED — we note it, for each audience.

    Called by `TS_ONFINISH`, never by hand. It displays nothing: its
    output would land in the log of the job that just finished, where
    nobody reads it.

    IT NEVER RAISES. This program runs behind the back of someone waiting
    for their result; failing here would soil a job that itself went
    fine.

    THE INTENT IS NOT IN ITS ARGUMENTS. `tsp` passes the raw command,
    without the label — so we read it back from the queue, which still
    carries it at that instant.
    """
    try:
        intent, client, uid = "", UNCLAIMED, ""
        job = _one(job_id)
        if job:
            intent = job.get("intent") or ""
            uid = job.get("uid") or ""
            # WHOSE ENDING THIS IS, read back from the label — the only
            # place it could have survived the trip through `tsp`.
            client = job.get("client") or UNCLAIMED

        signal = {"id": uid or job_id, "queue_id": job_id,
                  "code": code, "log": logfile,
                  "client": client, "intent": intent,
                  "command": " ".join(command),
                  "finished_at": time.time()}
        line = json.dumps(signal, ensure_ascii=False) + "\n"

        for audience in AUDIENCES:
            box = _mailbox(client, audience)
            box.parent.mkdir(parents=True, exist_ok=True)
            with box.open("a", encoding="utf-8") as f:
                f.write(line)

        # TIDYING HAPPENS HERE, and nowhere a person is waiting. This
        # runs behind a job that has already finished, so a directory
        # scan costs nobody anything — and a job ending is exactly when
        # the tree has changed and is worth sweeping.
        _forget_empty_mailboxes(client)
    except Exception:  # noqa: BLE001 — see the docstring: we do not soil
        pass
    return OK


def _signals(audience: str, as_json: bool, client: str | None) -> int:
    """WHAT HAS FINISHED SINCE LAST TIME — and only once.

    ────────────────────────────────────────────────────────────────────
    THE SIGNAL IS CONSUMED
    ────────────────────────────────────────────────────────────────────

    We read AND erase, in a single gesture. Nothing speaks again until
    the next ending: the backoff is structural, there is no state to keep
    on the side — no date of last look, no list of already-announced
    jobs, no fingerprint to compare.

    ────────────────────────────────────────────────────────────────────
    WHY IT CLAIMS BY RENAMING
    ────────────────────────────────────────────────────────────────────

    Reading then unlinking left a window: an `onfinish` appending between
    the two had its line deleted before anyone rendered it, and losing an
    ending is silent by nature — there is nothing left to see.

    `rename` is atomic. After it, every later ending opens the path afresh
    and lands in a new file, untouched by the deletion that follows.

    ────────────────────────────────────────────────────────────────────
    WHY THIS VERB IS GENERIC
    ────────────────────────────────────────────────────────────────────

    jobbox knows no harness and does not want to. It returns facts; the
    shaping — a hook's JSON, a desktop notification, a message — belongs
    to whoever integrates it.
    """
    for s in _take(client or _client(), audience):
        if as_json:
            emit(json.dumps(s, ensure_ascii=False))
            continue
        state = "OK" if s.get("code") == "0" else f"FAILED (exit={s.get('code')})"
        emit(f"  job {s.get('id')}  {s.get('intent') or '?'}  {state}"
             f"  — {s.get('log')}")
    return OK


def _stranded(me: str) -> list[tuple[str, int]]:
    """Endings held in mailboxes that are not ours, newest count first.

    WHOSE PROBLEM THIS IS. Naming clients automatically means jobbox does
    not decide who exists — the harness does. If it ever gives a
    sub-agent, or a resumed session, a different name from the one that
    will come looking, that client's endings are held by a mailbox nobody
    returns to.

    WE DO NOT TRY TO GUESS WHICH ONES ARE ABANDONED. A mailbox belonging
    to a session that is merely idle looks exactly like one belonging to
    a session that is gone, and draining someone else's on a guess is the
    theft this whole design removed.

    So we only make it VISIBLE, and we do it from `health` — the verb
    people run when they suspect something, and the one place where
    saying it costs nobody anything. A hook saying it every turn would
    become the banner nobody reads.
    """
    if not SIGNALS.is_dir():
        return []
    held = []
    for folder in sorted(SIGNALS.iterdir()):
        if not folder.is_dir() or folder.name == me:
            continue
        count = 0
        for audience in AUDIENCES:
            if audience in SHARED_AUDIENCES:
                continue
            try:
                count += sum(1 for l in (folder / f"{audience}.jsonl")
                             .read_text(encoding="utf-8").splitlines()
                             if l.strip())
            except OSError:
                pass
        if count:
            held.append((folder.name, count))
    return held


def _report_stranded() -> None:
    """Say what another client is holding, and what to do about it."""
    held = _stranded(_client())
    if not held:
        return
    total = sum(n for _, n in held)
    emit(f"  UNREAD     {total} ending(s) in {len(held)} other mailbox(es)")
    for name, count in held:
        emit(f"             {name} holds {count}")
    emit("             `jobbox signals agent --client <name>` reads one")


def _slots_cmd(count: int | None) -> int:
    """READ, OR SET, THE WIDTH OF THE QUEUE.

    IT IS A MACHINE-WIDE SETTING shared by every client on this account,
    so this verb exists to make a change VISIBLE and deliberate.

    A NEW queue opens at half the cores — see `wanted_slots`. What is set
    here outlives that: the default is only ever applied when jobbox is
    the one bringing a daemon into being, precisely so a deliberate width
    is never quietly undone.
    """
    _require_tsp()
    if count is None:
        counted = slots(_tsp("-l").stdout)
        if counted is None:
            say("  could not read the queue width")
            return FAILURE
        busy, total = counted
        emit(f"  {busy}/{total} busy")
        return OK
    if count < 1:
        raise Refusal("a queue needs at least one slot")
    res = _tsp("-S", str(count))
    if res.returncode != 0:
        say(f"  {res.stderr.strip() or 'tsp refused'}")
        return FAILURE
    say(f"  the queue now runs up to {count} job(s) at once — this is "
        f"machine-wide, every client shares it")
    return OK


def _clients() -> int:
    """WHAT NOBODY IS COMING BACK FOR.

    Per-client mailboxes fix the theft between agents, but they open a
    quieter failure: a session that ends leaves its endings behind, and a
    file nobody reads is indistinguishable from a file with nothing in
    it. Splitting the mailboxes without a way to see them all would trade
    a loud bug for a silent one.

    NAMING SESSIONS AUTOMATICALLY MAKES THAT WORSE, not better: every
    session creates a mailbox, so the empty remains of closed sessions
    accumulate and bury the one that still holds something.

    THEY ARE FORGOTTEN WITHOUT BEING ASKED — here, and behind every job
    that ends. Only ones that are empty, not the caller's own, and not
    recently touched; the conditions are in `_forget_empty_mailboxes`,
    and each of them prevents a loss rather than being politeness.

    An entry that still holds something is NEVER removed. It is the one
    piece of evidence that a job finished and nobody was told.
    """
    me = _client()
    if not SIGNALS.is_dir():
        say("  no mailbox yet — nothing has finished")
        return OK

    def _count(box: Path) -> int:
        try:
            return sum(1 for l in box.read_text(encoding="utf-8").splitlines()
                       if l.strip())
        except OSError:
            return 0

    # THE SHARED ONES FIRST, because they belong to nobody and would
    # otherwise read as a client with a strange name.
    for audience in SHARED_AUDIENCES:
        count = _count(_mailbox(me, audience))
        emit(f"  {'(' + audience + ', shared)':<28} "
             f"{count if count else 'empty'}")

    forgotten, seen = _forget_empty_mailboxes(me), False
    for folder in sorted(SIGNALS.iterdir()):
        if not folder.is_dir():
            continue
        pending = [(a, _count(folder / f"{a}.jsonl")) for a in AUDIENCES
                   if a not in SHARED_AUDIENCES]
        seen = True
        detail = " ".join(f"{a}={n}" for a, n in pending if n) or "empty"
        mark = " ← you" if folder.name == me else ""
        emit(f"  {folder.name:<28} {detail}{mark}")
    if not seen:
        say("  no session mailbox — nothing of yours is waiting")
    if forgotten:
        say(f"  forgot {forgotten} empty mailbox(es) of finished sessions")
    return OK


def _health() -> int:
    """TWO QUESTIONS, AND THEY DO NOT HAVE THE SAME ANSWER.

    ────────────────────────────────────────────────────────────────────
    ASKING IS WHAT STARTS THE DAEMON — MEASURED
    ────────────────────────────────────────────────────────────────────

    `tsp -l` on a fresh socket does not fail: it STARTS the server and
    returns an empty list, exit 0. So "did the daemon answer" is a
    question this verb cannot answer by asking it — and a queue that died
    with its daemon would read as an ordinary empty queue.

    What IS observable is whether the socket existed BEFORE we asked. If
    it did not, the daemon we are now talking to is one we just created,
    and whatever was queued before is gone. That is the fact worth
    saying, and it is the one that used to be silent.

    A stale socket left by a hard kill would read as "was up" — the check
    is honest about what it sees, not about what happened.

    "Is a job stuck" is nowhere in `tsp`: that is our addition, and it
    comes from the freshness of the output file.
    """
    _require_tsp()
    was_up = SOCKET.exists()
    res = _tsp("-l")
    if res.returncode != 0:
        say(f"  task-spooler refused to answer (socket {SOCKET})")
        return FAILURE
    jobs = parse(res.stdout)
    if was_up:
        emit(f"  daemon     alive, {len(jobs)} job(s) known")
    else:
        emit("  daemon     STARTED BY THIS CHECK — it was not running")
        emit( "             anything queued before it died with it")
        emit(f"             opened at {wanted_slots()} slot(s) "
             f"(half the cores; JOBBOX_SLOTS overrides)")
    counted = slots(res.stdout)
    if counted:
        busy, total = counted
        waiting = sum(1 for j in jobs if j["state"] == "queued")
        emit(f"  slots      {busy}/{total} busy, {waiting} waiting")
        # THE ANSWER TO "WHY IS MY JOB NOT STARTING", said before anyone
        # has to ask it. One slot is the default and it is a legitimate
        # choice; being silent about it while several clients share the
        # queue is what turns it into a mystery.
        if waiting and total == 1:
            emit("             one slot — jobs run strictly one after "
                 "another (`jobbox slots 2` to widen)")
    threshold = _mute_after()
    stuck = [(j, m) for j in jobs
             if (m := _silence(j)) is not None and m > threshold]
    if not stuck:
        emit("  no mute job")
    for j, mute in stuck:
        # NAMED BY THE ID SOMEBODY WOULD TYPE. `health` said "STUCK? 3"
        # while every other verb spoke in minted ids — and 3 is the
        # number that stops meaning this job the moment the daemon dies.
        emit(f"  STUCK?     {j['uid'] or j['id']} {j['intent']} — nothing "
             f"written for {int(mute)}s")
    # AFTER THE MUTE JOBS AND OUTSIDE THEIR BRANCH. It sat past an early
    # return, so it only spoke when a job was ALREADY stuck — that is,
    # never in the case it exists for. The unit test passed because it
    # called the helper directly; running the verb is what caught it.
    _report_stranded()
    # A MUTE JOB IS NOT A SYSTEM FAILURE. We NAME it and return OK: it may
    # be a script computing without saying anything, and returning a
    # failure would make `health` an alarm people switch off.
    return OK


# ── WHERE THE HARNESS IS ALLOWED TO EXIST ───────────────────────────────
#
# Everything above knows no harness, and that is deliberate: `signals`
# returns facts, and shaping them into a hook's JSON, a desktop
# notification or a message belongs to whoever integrates.
#
# That principle held while a neighbouring project carried the bridge.
# It cost jobbox its usability: the tool was publishable and nobody could
# WIRE it without writing the bridge themselves.
#
# So the harness lives here, in two verbs that say so in their names, and
# nowhere else. `claude-hook` shapes; `init` declares. Adding a second
# harness means a second pair, not a change above this line.

#: THE THREE MOMENTS a Claude Code session can be told something, and the
#: audience each one serves. `Stop` is the only path that reaches the
#: model itself, so it is the one that carries the human's copy.
CLAUDE_HOOKS = (
    ("SessionStart", "agent", "text"),
    ("UserPromptSubmit", "agent", "text"),
    ("Stop", "user", "stop"),
)

#: THE DISCIPLINE, SHIPPED WITH THE TOOL.
#:
#: Knowing when to reach for jobbox is not derivable from its `--help`,
#: and it used to live in the host project — so anyone installing the
#: tool got the verbs and none of the judgement. Same shape as the hook
#: bridge, and the same fix: it travels with the thing it describes.
SKILL = Path(__file__).resolve().parent / "skills" / "jobbox" / "SKILL.md"

#: WHERE CLAUDE CODE LOOKS. Global rather than per project: the
#: discipline is about the tool, not about one repository.
SKILL_HOME = Path.home() / ".claude" / "skills" / "jobbox" / "SKILL.md"


def _install_skill(force: bool) -> str | None:
    """Lay the skill down, and say so — or stay quiet if it is already there.

    NEVER OVERWRITES WITHOUT BEING ASKED. This writes outside the project,
    into something the user may have edited; silently replacing their
    version would be the same defect as an `init` that rewrites a
    settings file.
    """
    if not SKILL.exists():
        return None                    # an install that did not ship it
    if SKILL_HOME.exists() and not force:
        return None
    try:
        SKILL_HOME.parent.mkdir(parents=True, exist_ok=True)
        SKILL_HOME.write_text(SKILL.read_text(encoding="utf-8"),
                              encoding="utf-8")
    except OSError as exc:
        say(f"  could not install the skill ({exc})")
        return None
    return str(SKILL_HOME)


#: THE PAIR THAT MEASURES. Both run `observe`, which tells them apart by
#: the event name it is handed — one verb, so there is one place that
#: knows the payload's shape.
CLAUDE_OBSERVERS = ("PreToolUse", "PostToolUse")


def _claude_hook(audience: str, shape: str) -> int:
    """THE ONLY PLACE THAT KNOWS WHAT `systemMessage` MEANS.

    It consumes the same mailbox `signals` does, and prints what Claude
    Code expects: plain lines for the two informative hooks, a JSON
    object for `Stop` — which is the only hook whose output reaches the
    model rather than a debug log.

    IT NEVER FAILS. A hook that errors is a hook that gets deleted, and
    this one runs on every single turn to say, almost always, nothing.
    """
    try:
        pending = _take(_client(), audience)
    except Exception:  # noqa: BLE001 — see the docstring
        return OK
    if not pending:
        return OK                      # silence is the normal case

    failed = [s for s in pending if s.get("code") != "0"]
    summary = " · ".join(
        (s.get("intent") or f"job {s.get('id')}")
        + ("" if s.get("code") == "0" else f" (exit={s.get('code')})")
        for s in pending)

    if shape == "stop":
        what = ("one job finished" if len(pending) == 1
                else f"{len(pending)} jobs finished")
        out: dict[str, Any] = {
            "systemMessage": f"jobbox: {what} — {summary}."
            + (f" {len(failed)} failed." if failed else "")}
        if failed:
            # BLOCKING IS THE ONLY WAY IN. A `Stop` hook's stdout goes to
            # a debug log; only `decision` reaches the model. We spend
            # that on failures alone — blocking on every ending would
            # make the session unstoppable.
            logs = " ".join(str(s.get("log")) for s in failed) or "—"
            out["decision"] = "block"
            out["reason"] = (
                f"jobbox: {summary}. Failed job logs: {logs}. "
                "Read them, say what broke, and fix it if it is within "
                "reach.")
        emit(json.dumps(out, ensure_ascii=False))
        return OK

    for s in pending:
        state = "OK" if s.get("code") == "0" else f"FAILED exit={s.get('code')}"
        emit(f"[jobbox] {s.get('intent') or s.get('id')} — {state} "
             f"— {s.get('log')}")
    if failed:
        emit("A background job failed. Look at its log before stacking "
             "anything else on top.")
    return OK


def _timings(top: int, reset: bool, detail: bool, session: str | None) -> int:
    """WHAT THE DECISION WILL BE READ FROM.

    SORTED BY TOTAL TIME WAITED, not by the slowest single call. The trap
    is not the ten-minute build — nobody runs that in the foreground
    twice. It is the forty-second command run thirty times: each one too
    short to stop for, and their sum is the half-hour.

    Ordering by the worst case would hide exactly that, and it is the
    thing the whole question was about.
    """
    if reset:
        for path in (OBSERVED,):
            path.unlink(missing_ok=True)
        if PENDING.is_dir():
            for stale in PENDING.iterdir():
                stale.unlink(missing_ok=True)
        say("  forgot every measurement")
        return OK
    if not OBSERVED.exists():
        say("  nothing measured yet — is `jobbox init` wired, and has a "
            "new session started since?")
        return OK

    rows = []
    for line in OBSERVED.read_text(encoding="utf-8").splitlines():
        if line.strip():
            try:
                rows.append(json.loads(line))
            except ValueError:
                continue
    if not rows:
        say("  nothing measured yet")
        return OK

    if session:
        rows = [r for r in rows if r.get("session") == session]
        if not rows:
            say(f"  nothing measured for session {session!r}")
            return OK
    waited = [r for r in rows if not r.get("background")]
    detached = len(rows) - len(waited)
    total = sum(r["seconds"] for r in waited)

    shapes: dict[str, list[float]] = {}
    for r in waited:
        shapes.setdefault(r["shape"], []).append(r["seconds"])

    emit(f"  {len(rows)} call(s) measured, {detached} already detached")
    emit(f"  {total / 60:.1f} min spent waiting in the foreground")
    # HOW MANY SESSIONS THIS COVERS, because a distribution drawn from
    # one is not the same claim as one drawn from several — and the table
    # gave no other way to tell.
    seen = sorted({r.get("session", "") for r in rows if r.get("session")})
    if seen:
        emit(f"  across {len(seen)} session(s): {' '.join(seen)}")
    # HOW MUCH THIS TABLE IS MISSING, rather than a warning that it is.
    # Hooks do not fire for every call; a stamp with no end never becomes
    # a row. Saying "these are lower bounds" without a size lets a reader
    # assume the gap is small — or that it is enormous.
    try:
        unpaired = sum(1 for _ in PENDING.iterdir())
    except OSError:
        unpaired = 0
    if unpaired:
        emit(f"  {unpaired} call(s) started and never closed — these totals "
             f"are lower bounds")
    emit("")
    emit(f"  {'total':>8} {'calls':>6} {'median':>8}  shape")
    for shape, times in sorted(shapes.items(),
                               key=lambda kv: -sum(kv[1]))[:top]:
        times.sort()
        median = times[len(times) // 2]
        emit(f"  {sum(times):>7.0f}s {len(times):>6} {median:>7.1f}s  {shape}")
    if detail:
        _detail(waited, total)
    return OK


#: THE BANDS A THRESHOLD WOULD BE DRAWN BETWEEN.
_BANDS = ((0, 2), (2, 5), (5, 15), (15, 60), (60, None))

#: THE CUTOFFS WORTH PRICING. What matters is not the number but how much
#: it buys for how many interruptions.
_CUTOFFS = (5, 10, 20, 30, 60)


def _detail(waited: list[dict[str, Any]], total: float) -> None:
    """THE READING, AS A VERB RATHER THAN A SCRIPT SOMEBODY RETYPES.

    This analysis was hand-written twice before it lived here, and the
    person who has to DECIDE could not run it without asking. A table
    only somebody else can read is a table that decides nothing.

    It also prints the per-session split, because a distribution drawn
    from one session is not the same claim as one drawn from several —
    and a reading that hides which case it is will be believed anyway.
    """
    emit("")
    emit("  how the waiting is spread")
    for low, high in _BANDS:
        band = [r for r in waited
                if low <= r["seconds"] and (high is None or r["seconds"] < high)]
        if not band:
            continue
        spent = sum(r["seconds"] for r in band)
        label = f"{low}-{high}s" if high else f"> {low}s"
        emit(f"  {label:>9} {len(band):>4} calls {spent:>7.0f}s "
             f"{spent / total * 100:>5.1f}%")

    emit("")
    emit("  what a guard would buy, and what it would interrupt")
    for cutoff in _CUTOFFS:
        beyond = [r for r in waited if r["seconds"] >= cutoff]
        if not beyond:
            continue
        spent = sum(r["seconds"] for r in beyond)
        emit(f"  at {cutoff:>3}s  {len(beyond):>4} calls "
             f"({len(beyond) / len(waited) * 100:>4.1f}%)  "
             f"recovers {spent / 60:>5.1f} min of {total / 60:.1f}")

    # THE OUTLIER, NAMED. One call carrying most of the total means the
    # table describes that call, not a habit — and a threshold drawn from
    # it would too.
    worst = max(waited, key=lambda r: r["seconds"])
    share = worst["seconds"] / total * 100
    if share > 30:
        emit("")
        emit(f"  CAREFUL: one call ({worst['seconds']:.0f}s, "
             f"{worst['shape'][:28]}) is {share:.0f}% of the total —")
        emit("           a threshold drawn from this describes it, not a habit")

    by_session: dict[str, list[float]] = {}
    for r in waited:
        by_session.setdefault(r.get("session") or "?", []).append(r["seconds"])
    if len(by_session) > 1:
        emit("")
        emit("  per session — a shared shape justifies one threshold, "
             "a split one does not")
        for who, times in sorted(by_session.items(),
                                 key=lambda kv: -sum(kv[1])):
            times.sort()
            emit(f"  {who:>10} {len(times):>4} calls "
                 f"{sum(times):>7.0f}s  median {times[len(times) // 2]:>5.1f}s")


def _init(force: bool, client: str | None, project: str | None) -> int:
    """WIRE THIS HARNESS, AND SAY WHAT CHANGED.

    NAMED FOR CLAUDE CODE IN ITS HELP, because that is the only harness
    it knows how to write settings for. A second one is a second verb
    beside this, not a flag inside it — and nothing above this line has
    to move for that to happen.


    IT MERGES, IT DOES NOT OVERWRITE. A project's `.claude/settings.json`
    almost always already carries hooks that have nothing to do with us;
    replacing the file would silently remove someone else's wiring, and
    the loss would only be noticed the next time that hook mattered.

    So each of our three entries is added only if an equivalent one is
    absent, everything else in the file is preserved byte-for-byte in
    meaning, and the command says which lines it wrote.

    Re-running it is safe: that is the whole point of an `init` you can
    call again after an upgrade.

    `--client` IS THE EXCEPTION, NOT THE SETUP. Sessions name themselves
    from the harness's own session id, so mailboxes are separated with no
    configuration at all. Pinning one name here makes every session in
    this project share a mailbox again — which is right for a CI runner
    or a single shared worker, and wrong for a person opening two
    windows.
    """
    settings = Path.cwd() / ".claude" / "settings.json"
    data: dict[str, Any] = {}
    if settings.exists():
        try:
            data = json.loads(settings.read_text(encoding="utf-8"))
        except ValueError as exc:
            say(f"  {settings} is not readable JSON ({exc}) — refusing to "
                f"touch it. Fix it, then run `jobbox init` again.")
            return FAILURE
    hooks = data.setdefault("hooks", {})

    declared = [(event, f"jobbox claude-hook {audience} {shape}", None)
                for event, audience, shape in CLAUDE_HOOKS]
    # THE MEASURING PAIR IS MATCHED ON `Bash` AND NOTHING ELSE. Every
    # tool call would pay two process starts for a number nobody asked
    # for; the question is about shell commands.
    declared += [(event, "jobbox observe", "Bash")
                 for event in CLAUDE_OBSERVERS]

    written = []
    for event, command, matcher in declared:
        entries = hooks.setdefault(event, [])
        # OURS BY VERB, not by the whole line and not by a substring.
        #
        # Exact equality was the first rule, and it broke the promise that
        # matters most here: an `init` you can re-run after an upgrade.
        # The day a declaration gained a flag, the old line stopped
        # matching and was left sitting beside the new one — two hooks
        # doing the same work, and the file saying so to nobody.
        #
        # A substring would be worse: a project whose own hook merely has
        # "jobbox" in its path is not us. The verb is the honest middle —
        # `jobbox observe` is unmistakably this tool invoked by name,
        # where a foreign hook is a path.
        verb = " ".join(command.split()[:2])
        mine = [entry for entry in entries if isinstance(entry, dict)
                and any(isinstance(h, dict)
                        and str(h.get("command") or "").startswith(verb)
                        for h in entry.get("hooks", []))]
        # UNCHANGED AND ALREADY THERE is the only case worth skipping.
        settled = any(h.get("command") == command
                      for entry in mine for h in entry.get("hooks", []))
        if mine and settled and not force:
            continue

        # `--force` REWRITES, and used to do nothing at all. When the
        # entry was already there and `force` was set, neither branch of
        # the old shape fired: the flag was declared, documented, advised
        # by the closing message — and inert. Worse than absent, because
        # someone repairing a mangled entry watched it "succeed".
        for entry in mine:
            entries.remove(entry)
        fresh: dict[str, Any] = {"hooks": [{"type": "command",
                                            "command": command,
                                            "timeout": 10}]}
        if matcher:
            fresh["matcher"] = matcher
        entries.append(fresh)
        written.append(f"{event} -> {command}"
                       + ("  (replaced)" if mine else ""))

    # THE PROJECT NAME, CAPTURED ONCE. Taken from this directory now, so
    # that it never changes later: a client renamed mid-session splits
    # its mailbox and strands whatever was already in it.
    env = data.setdefault("env", {})
    here = Path.cwd().resolve()
    named = (_sane(project) if project else "") or project_tag(here)
    if named and (env.get("JOBBOX_PROJECT") != named or force):
        env["JOBBOX_PROJECT"] = named
        written.append(f"env.JOBBOX_PROJECT = {named}")
    if env.get("JOBBOX_PROJECT_PATH") != str(here) or force:
        env["JOBBOX_PROJECT_PATH"] = str(here)
        written.append(f"env.JOBBOX_PROJECT_PATH = {here}")
    remember_project(named, str(here))

    if client:
        # CLAUDE CODE APPLIES THIS TO THE SESSION'S ENVIRONMENT, which is
        # what puts the name in reach of the `jobbox run` we type in a
        # shell — and what makes it override the per-session default.
        if env.get("JOBBOX_CLIENT") != client or force:
            env["JOBBOX_CLIENT"] = client
            written.append(f"env.JOBBOX_CLIENT = {client}")

    skill = _install_skill(force)
    if skill:
        written.append(f"skill -> {skill}")

    if not written:
        emit(f"  {settings}: already wired — nothing to do")
        # ONLY ADVISE WHAT HAS NOT BEEN TRIED. The old message suggested
        # `--force` even to someone who had just passed it, which reads
        # as "you did not do the thing you did".
        if not force:
            emit("  (re-run with --force to rewrite the entries)")
        return OK

    settings.parent.mkdir(parents=True, exist_ok=True)
    settings.write_text(json.dumps(data, indent=2, ensure_ascii=False) + "\n",
                        encoding="utf-8")
    for what in written:
        emit(f"  wrote  {what}")
    emit(f"  in     {settings}")
    # WHEN THEY TAKE EFFECT IS NOT SOMETHING WE KNOW.
    #
    # This repository asserted "only at session start", untested. Running
    # `init` inside a live session then showed the next four commands
    # being measured — so that was corrected to "straight away". The
    # fifth command was not measured, and nothing here explains why.
    #
    # Two claims, both made on thin evidence, both wrong to state as a
    # rule. What is said now is what was seen, and the one thing that
    # reliably works.
    say("")
    say("  A new session is what reliably arms these. They have also been "
        "seen taking effect immediately — do not count on it.")
    return OK


def _parser() -> argparse.ArgumentParser:
    """THE WHOLE INTERFACE, IN ONE PLACE.

    Declared here rather than scattered over the verbs it describes.
    Everything a caller can type is on this page, which is what makes it
    possible to see that two verbs disagree — the kind of thing that
    hides when each command carries its own decorators.
    """
    parser = argparse.ArgumentParser(
        prog="jobbox",
        description="Run a long command in the background, and find it again.")
    parser.add_argument("-V", "--version", action="version",
                        version=f"jobbox, version {VERSION}")
    parser.add_argument("-q", "--quiet", action="count", default=0,
                        help="Less.")
    parser.add_argument("-v", "--verbose", action="count", default=0,
                        help="More detail.")
    verbs = parser.add_subparsers(dest="verb", metavar="VERB")

    def verb(name, help_text, handler, hidden=False):
        sub = verbs.add_parser(name, help=argparse.SUPPRESS if hidden
                               else help_text, description=help_text)
        sub.set_defaults(handler=handler)
        return sub

    # RUN TAKES A RAW TAIL. Everything after the intent belongs to the
    # command being queued, options included — `--` separates them, and
    # argparse eats exactly one, which is the one the caller typed.
    run = verb("run", "Queue a command. `jobbox run <intent> -- …`",
               lambda ns: _run(ns.intent, tuple(
                   ns.command[1:] if ns.command[:1] == ["--"] else ns.command)))
    run.add_argument("intent")
    run.add_argument("command", nargs=argparse.REMAINDER)

    listing = verb("list", "What waits, what runs, what has finished.",
                   lambda ns: _list(ns.mine, ns.project_path))
    scope = listing.add_mutually_exclusive_group()
    scope.add_argument("--mine", action="store_true",
                       help="only this session's jobs (see JOBBOX_CLIENT)")
    listing.add_argument("--project-path", action="store_true",
                         help="show each project's directory, not its name")
    scope.add_argument("--all", action="store_true",
                       help="every job on the machine — the default, "
                            "spelled out")

    status = verb("status", "One job: its state, its code, its duration.",
                  lambda ns: _status(ns.job))
    status.add_argument("job", metavar="ID")

    tail = verb("tail", "A job's log. `-f` to follow it.",
                lambda ns: _tail(ns.job, ns.follow, ns.lines))
    tail.add_argument("job", metavar="ID")
    tail.add_argument("-f", "--follow", action="store_true",
                      help="follow the writing")
    tail.add_argument("-n", "--lines", type=int, default=40,
                      help="how many lines")

    kill = verb("kill", "Stop a running job.", lambda ns: _kill(ns.job))
    kill.add_argument("job", metavar="ID")

    onfinish = verb("onfinish", "Called by tsp when a job ends.",
                    lambda ns: _onfinish(ns.job_id, ns.code, ns.logfile,
                                         tuple(ns.command)), hidden=True)
    for name in ("job_id", "code", "logfile"):
        onfinish.add_argument(name)
    onfinish.add_argument("command", nargs=argparse.REMAINDER)

    signals = verb("signals", "Consume an audience's job endings.",
                   lambda ns: _signals(ns.audience, ns.as_json, ns.client))
    signals.add_argument("audience", choices=AUDIENCES)
    signals.add_argument("--json", dest="as_json", action="store_true",
                         help="one JSON line per job, for an integrator")
    signals.add_argument("--client", default=None,
                         help="read another client's mailbox")

    slots = verb("slots", "How many jobs may run at once. With a number, set it.",
                 lambda ns: _slots_cmd(ns.count))
    slots.add_argument("count", type=int, nargs="?")

    verb("clients", "Who has a mailbox, and what waits in it.",
         lambda ns: _clients())
    verb("health", "Is the daemon there, and who is stuck.",
         lambda ns: _health())

    hook = verb("claude-hook", "Shape pending endings for Claude Code.",
                lambda ns: _claude_hook(ns.audience, ns.shape))
    hook.add_argument("audience", choices=AUDIENCES)
    hook.add_argument("shape", choices=("text", "stop"))

    # DECLARED SO `--help` LISTS IT AND MISUSE IS REFUSED, but never
    # dispatched from here: `main` sends `observe` to the fast path above
    # before any of this is built.
    verb("observe", "Time one tool call, from a hook.",
         lambda ns: observe(sys.stdin, []), hidden=True)

    timings = verb("timings", "What actually takes time, measured.",
                   lambda ns: _timings(ns.top, ns.reset, ns.detail,
                                       ns.session))
    timings.add_argument("-n", "--top", type=int, default=12,
                         help="how many shapes to show")
    timings.add_argument("--reset", action="store_true",
                         help="forget everything measured")
    timings.add_argument("--detail", action="store_true",
                         help="bands, thresholds and per-session split")
    timings.add_argument("--session", default=None,
                         help="read one session's calls only")

    init = verb("init", "Wire jobbox into Claude Code, in this directory.",
                lambda ns: _init(ns.force, ns.client, ns.project))
    init.add_argument("--force", action="store_true",
                      help="rewrite the entries even if already declared")
    init.add_argument("--client", default=None,
                      help="PIN one fixed name for every session here")
    init.add_argument("--project", default=None,
                      help="the project name in each client (default: "
                           "this directory's name)")
    return parser


def main(argv: list[str] | None = None) -> int:
    """THE `main()`, AND IT TRANSLATES EVERY EXIT INTO A CODE.

    NOTHING BELOW MAY EXIT ON ITS OWN. `argparse` calls `sys.exit` for
    `--help`, `--version` and misuse; swallowing that here is what lets a
    caller in a script read a number instead of catching SystemExit.
    """
    args = sys.argv[1:] if argv is None else list(argv)

    # THE SAME SHORTCUT THE FILE TAKES AT IMPORT, for the same reason and
    # so there is one implementation: a hook must not pay for a parser it
    # does not use.
    if args[:1] == ["observe"]:
        return observe(sys.stdin, args[1:])

    try:
        chosen = _parser().parse_args(args)
    except SystemExit as exc:
        # `--help` and `--version` leave 0; misuse leaves 2, which is
        # already this tool's code for it.
        return OK if exc.code in (0, None) else USAGE
    if not getattr(chosen, "handler", None):
        _parser().print_help()
        return OK
    apply_verbosity(chosen.quiet, chosen.verbose)
    try:
        return chosen.handler(chosen)
    except Refusal as exc:
        say(f"  {exc}")
        return USAGE
    except KeyboardInterrupt:
        # NO MESSAGE: the user just hit Ctrl-C, they know.
        return INTERRUPTED
    except BrokenPipeError:
        # `jobbox tail … | head` CLOSES THE PIPE, and that is normal.
        return OK


if __name__ == "__main__":
    sys.exit(main())
