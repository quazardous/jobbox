"""`jobbox init` — wiring a project without destroying its wiring.

────────────────────────────────────────────────────────────────────────
THE FAILURE THIS FILE GUARDS
────────────────────────────────────────────────────────────────────────

`.claude/settings.json` is a shared file. By the time anyone runs
`jobbox init`, it almost always already carries hooks belonging to other
tools.

An `init` that writes the file it wants would remove them, and the loss
would be silent in the worst way: nothing fails at init time, and the
missing hook is only noticed the next time it should have fired — which
may be days later, on the one occasion it mattered.

So the whole verb is a merge, and these tests are about what it must
leave alone.
"""
from __future__ import annotations

import io
import json
import os
import tempfile
from contextlib import contextmanager
from contextlib import redirect_stdout
from pathlib import Path

import jobbox


@contextmanager
def _scratch_state(tmp: str):
    """Keep `init` from writing into the caller's own machine state.

    `init` records which directory a project tag stands for, in a file
    shared by every project on the machine. A test that leaves its
    temporary directories in there is a test that pollutes the thing it
    is checking — measured: eighteen `/tmp/tmpXXXX` entries in a real
    `projects.json` before this existed.
    """
    previous = jobbox.PROJECTS, jobbox.SKILL_HOME
    jobbox.PROJECTS = Path(tmp) / "projects.json"
    jobbox.SKILL_HOME = Path(tmp) / "skills" / "jobbox" / "SKILL.md"
    try:
        yield
    finally:
        jobbox.PROJECTS, jobbox.SKILL_HOME = previous


#: A settings file as one is actually found in the wild: another tool's
#: hooks, on the same events jobbox wants, plus keys jobbox knows nothing
#: about.
EXISTING = {
    "permissions": {"allow": ["Bash(git status)"]},
    "hooks": {
        "SessionStart": [
            {"hooks": [{"type": "command",
                        "command": "somebody-elses-hook", "timeout": 5}]}],
        "PreToolUse": [
            {"matcher": "Bash",
             "hooks": [{"type": "command", "command": "their-guard"}]}],
    },
}


def _init(settings: dict | None, *args: str) -> tuple[dict, str]:
    """Run `init` in a scratch directory, return (the file after, output)."""
    here = os.getcwd()
    with tempfile.TemporaryDirectory() as tmp, _scratch_state(tmp):
        try:
            os.chdir(tmp)
            path = Path(tmp) / ".claude" / "settings.json"
            if settings is not None:
                path.parent.mkdir(parents=True)
                path.write_text(json.dumps(settings, indent=2),
                                encoding="utf-8")
            buffer = io.StringIO()
            with redirect_stdout(buffer):
                jobbox.main(["init", *args])
            after = json.loads(path.read_text(encoding="utf-8"))
        finally:
            os.chdir(here)
    return after, buffer.getvalue()


def test_IT_KEEPS_EVERYTHING_IT_DID_NOT_WRITE():
    """THE ONE THAT MATTERS.

    Another tool's `SessionStart` hook, its `PreToolUse` guard, and a
    `permissions` block jobbox has no opinion about — all still there
    afterwards, unchanged.
    """
    after, _ = _init(EXISTING)

    assert after["permissions"] == EXISTING["permissions"]

    def _commands(event: str) -> list[str]:
        return [h["command"] for entry in after["hooks"][event]
                for h in entry["hooks"]]

    # BY PRESENCE, NOT BY EQUALITY. jobbox declares its own entries on
    # these same events, so the list is EXPECTED to grow. What must never
    # happen is an entry disappearing — asserting the list is unchanged
    # would fail every time jobbox gains a hook, and says less.
    assert "somebody-elses-hook" in _commands("SessionStart"), (
        f"another tool's hook must survive — got {_commands('SessionStart')}")
    assert "their-guard" in _commands("PreToolUse"), (
        f"another tool's guard must survive — got {_commands('PreToolUse')}")
    # AND THEIR ENTRY KEEPS ITS OWN SHAPE, matcher included: appending
    # next to it must not rewrite it.
    assert EXISTING["hooks"]["PreToolUse"][0] in after["hooks"]["PreToolUse"]


def test_IT_DECLARES_THE_THREE_MOMENTS():
    """A SESSION CAN BE TOLD AT THREE POINTS, and all three are wired.

    Missing one is not visible: the other two still speak, so the tool
    looks like it works while one channel is simply never used.
    """
    after, out = _init(None)

    for event, audience, shape in jobbox.CLAUDE_HOOKS:
        commands = [h["command"]
                    for entry in after["hooks"][event]
                    for h in entry["hooks"]]
        assert f"jobbox claude-hook {audience} {shape}" in commands, (
            f"{event} is not wired — {commands}")
    # AND THE MEASURING PAIR, matched on Bash so every other tool call
    # does not pay two process starts for a number nobody asked for.
    for event in jobbox.CLAUDE_OBSERVERS:
        ours = [entry for entry in after["hooks"][event]
                if any(h["command"].startswith("jobbox observe")
                       for h in entry["hooks"])]
        assert ours, f"{event} is not wired for measurement"
        assert ours[0]["matcher"] == "Bash", ours[0]
    before = [h["command"] for entry in after["hooks"]["PreToolUse"]
              for h in entry["hooks"] if h["command"].startswith("jobbox ")]
    assert before == ["jobbox observe"], before
    assert "session start" in out.lower() or "session" in out.lower()


def test_RUNNING_IT_TWICE_CHANGES_NOTHING():
    """AN `init` YOU CANNOT RE-RUN IS AN `init` NOBODY RE-RUNS.

    It has to be safe after an upgrade, so the second pass must not
    duplicate the entries — three hooks announced twice would fire twice
    and consume the mailbox twice.
    """
    here = os.getcwd()
    with tempfile.TemporaryDirectory() as tmp, _scratch_state(tmp):
        try:
            os.chdir(tmp)
            with redirect_stdout(io.StringIO()):
                jobbox.main(["init"])
            first = Path(tmp, ".claude/settings.json").read_text(
                encoding="utf-8")
            buffer = io.StringIO()
            with redirect_stdout(buffer):
                code = jobbox.main(["init"])
            second = Path(tmp, ".claude/settings.json").read_text(
                encoding="utf-8")
        finally:
            os.chdir(here)

    assert code == jobbox.OK
    assert first == second, "a second init must not change the file"
    assert "already wired" in buffer.getvalue(), buffer.getvalue()


def test_THE_CLIENT_NAME_LANDS_WHERE_A_SESSION_WILL_READ_IT():
    """WITHOUT IT, EVERY PROJECT ANSWERS TO `default` AND SHARES A MAILBOX.

    Claude Code applies `env` to the session, which is what puts the name
    within reach of the `jobbox run` typed in a shell.
    """
    after, _ = _init(EXISTING, "--client", "my-project")

    assert after["env"]["JOBBOX_CLIENT"] == "my-project"
    assert after["permissions"] == EXISTING["permissions"], "still merged"


def test_A_BROKEN_SETTINGS_FILE_IS_NOT_TOUCHED():
    """WE REFUSE RATHER THAN REWRITE.

    Unparseable JSON means someone is mid-edit, or a tool wrote garbage.
    Replacing it with a file containing only our three hooks would
    destroy whatever was in there — the very thing this verb exists to
    avoid.
    """
    here = os.getcwd()
    broken = '{"hooks": {"SessionStart": [ trunca'
    with tempfile.TemporaryDirectory() as tmp:
        try:
            os.chdir(tmp)
            path = Path(tmp) / ".claude" / "settings.json"
            path.parent.mkdir(parents=True)
            path.write_text(broken, encoding="utf-8")
            with redirect_stdout(io.StringIO()):
                code = jobbox.main(["init"])
            after = path.read_text(encoding="utf-8")
        finally:
            os.chdir(here)

    assert code == jobbox.FAILURE
    assert after == broken, "the file must be left exactly as it was"


def test_FORCE_ACTUALLY_REWRITES():
    """IT WAS DECLARED, DOCUMENTED, ADVISED — AND INERT.

    When the entry already existed and `--force` was set, neither branch
    of the old shape fired: nothing was written, the file came back
    identical byte for byte, and the exit code was 0. Someone repairing a
    mangled entry watched it succeed and changed nothing.

    A flag that does nothing is worse than a missing one: the missing one
    fails loudly.
    """
    here = os.getcwd()
    with tempfile.TemporaryDirectory() as tmp, _scratch_state(tmp):
        try:
            os.chdir(tmp)
            path = Path(tmp) / ".claude" / "settings.json"
            with redirect_stdout(io.StringIO()):
                jobbox.main(["init"])

            # SABOTAGE ONE OF OUR OWN ENTRIES, the way an edit would.
            data = json.loads(path.read_text(encoding="utf-8"))
            for entry in data["hooks"]["Stop"]:
                for h in entry["hooks"]:
                    if h["command"].startswith("jobbox "):
                        h["timeout"] = 1
                        del h["type"]
            path.write_text(json.dumps(data, indent=2), encoding="utf-8")

            buffer = io.StringIO()
            with redirect_stdout(buffer):
                jobbox.main(["init", "--force"])
            after = json.loads(path.read_text(encoding="utf-8"))
        finally:
            os.chdir(here)

    ours = [h for entry in after["hooks"]["Stop"] for h in entry["hooks"]
            if h["command"].startswith("jobbox ")]
    assert len(ours) == 1, f"--force must rewrite, not duplicate — {ours}"
    assert ours[0] == {"type": "command",
                       "command": "jobbox claude-hook user stop",
                       "timeout": 10}, ours[0]
    assert "replaced" in buffer.getvalue(), buffer.getvalue()


def test_A_LOOKALIKE_HOOK_IS_NOT_MISTAKEN_FOR_OURS():
    """"already wired" MUST MEAN OURS, not something that resembles them.

    A project with its own hook whose path merely contains "jobbox"
    would otherwise be declared wired without ever being wired — and the
    tool would say so and exit 0.
    """
    lookalike = {"hooks": {"SessionStart": [
        {"hooks": [{"type": "command",
                    "command": '"${CLAUDE_PROJECT_DIR:-.}/tools/hooks/'
                               'session-start-jobbox" 2>/dev/null || true'}]}]}}

    after, out = _init(lookalike)

    commands = [h["command"] for entry in after["hooks"]["SessionStart"]
                for h in entry["hooks"]]
    assert "jobbox claude-hook agent text" in commands, commands
    assert any("session-start-jobbox" in c for c in commands), "theirs kept"
    assert "already wired" not in out, out


def test_FORCE_DOES_NOT_ADVISE_ITSELF():
    """TELLING SOMEONE TO DO WHAT THEY JUST DID READS AS A FAILURE.

    With the rewrite fixed, `--force` always writes — so this guards the
    message on the only path that can still say "nothing to do".
    """
    here = os.getcwd()
    with tempfile.TemporaryDirectory() as tmp, _scratch_state(tmp):
        try:
            os.chdir(tmp)
            with redirect_stdout(io.StringIO()):
                jobbox.main(["init"])
            buffer = io.StringIO()
            with redirect_stdout(buffer):
                jobbox.main(["init"])
        finally:
            os.chdir(here)

    assert "--force" in buffer.getvalue(), "plain re-run should suggest it"


def test_INIT_LAYS_DOWN_THE_SKILL_AND_DOES_NOT_TRAMPLE_IT():
    """THE VERBS WITHOUT THE JUDGEMENT ARE HALF THE TOOL.

    Knowing when to reach for jobbox is not derivable from `--help`, and
    it used to live in the host project — so anyone installing the tool
    got neither. It travels with the thing it describes now.

    And it is written OUTSIDE the project, into something the user may
    have edited: replacing that silently would be the same defect as an
    `init` that rewrites a settings file.
    """
    import jobbox as jb

    previous = jb.SKILL_HOME
    here = os.getcwd()
    with tempfile.TemporaryDirectory() as tmp, _scratch_state(tmp):
        try:
            os.chdir(tmp)
            jb.SKILL_HOME = Path(tmp) / "home" / "skills" / "jobbox" / "SKILL.md"

            buffer = io.StringIO()
            with redirect_stdout(buffer):
                jb.main(["init"])
            assert jb.SKILL_HOME.exists(), "the skill was not installed"
            assert "skill ->" in buffer.getvalue(), buffer.getvalue()

            # A HAND-EDITED SKILL SURVIVES a plain re-run.
            jb.SKILL_HOME.write_text("mine, edited", encoding="utf-8")
            with redirect_stdout(io.StringIO()):
                jb.main(["init"])
            assert jb.SKILL_HOME.read_text(encoding="utf-8") == "mine, edited"

            # AND `--force` REFRESHES IT, which is what the flag is for.
            with redirect_stdout(io.StringIO()):
                jb.main(["init", "--force"])
            assert "name: jobbox" in jb.SKILL_HOME.read_text(encoding="utf-8")
        finally:
            jb.SKILL_HOME = previous
            os.chdir(here)


def test_A_DECLARATION_THAT_CHANGED_REPLACES_THE_OLD_ONE():
    """THE PROMISE THAT MATTERS: an `init` you can re-run after an upgrade.

    Exact-string matching broke it the first time a declaration gained a
    flag: the old line stopped matching, so it was left sitting beside
    the new one. Two hooks doing the same work, and nothing said so —
    found by looking at a settings file, not by a test.
    """
    older = {"hooks": {"PreToolUse": [
        {"matcher": "Bash",
         "hooks": [{"type": "command", "command": "jobbox observe --old"}]},
        {"matcher": "Bash",
         "hooks": [{"type": "command", "command": "their-guard"}]}]}}

    after, out = _init(older)

    ours = [h["command"] for entry in after["hooks"]["PreToolUse"]
            for h in entry["hooks"] if h["command"].startswith("jobbox ")]
    assert ours == ["jobbox observe"], (
        f"the superseded declaration must go, not pile up — got {ours}")
    assert "replaced" in out, out

    # AND SOMEBODY ELSE'S HOOK ON THE SAME EVENT IS STILL UNTOUCHED.
    everything = [h["command"] for entry in after["hooks"]["PreToolUse"]
                  for h in entry["hooks"]]
    assert "their-guard" in everything, everything


def test_CONFIG_SAYS_WHEN_THE_SETTINGS_FILE_AND_THE_SESSION_DISAGREE():
    """THE GAP BETWEEN `init` AND THE NEXT SESSION IS WHERE PEOPLE ASK.

    `env` is applied when a session starts, so from the moment `init`
    writes a project name until a new session opens, the file asks for
    one thing and the process is doing another. Showing only the live
    value answered "why is my client not what I just set" with silence.
    """
    here = os.getcwd()
    with tempfile.TemporaryDirectory() as tmp, _scratch_state(tmp):
        try:
            os.chdir(tmp)
            with redirect_stdout(io.StringIO()):
                jobbox.main(["init", "--project", "elsewhere"])
            buffer = io.StringIO()
            with redirect_stdout(buffer):
                code = jobbox.main(["config"])
        finally:
            os.chdir(here)

    out = buffer.getvalue()
    assert code == jobbox.OK, out
    assert "elsewhere" in out, "the pending project must be named"
    assert "next session" in out, out
    # AND THE THINGS THAT DECIDE BEHAVIOUR ARE ALL THERE.
    for expected in ("version", "client", "logs", "socket", "mute after",
                     "new queue", "hooks here", "skill"):
        assert expected in out, f"{expected!r} missing from config"


def test_CONFIG_STARTS_NOTHING():
    """AN INFORMATIONAL COMMAND THAT CREATES SOMETHING IS NOT ONE.

    It used to print the live queue width, which meant asking `tsp` —
    and any `tsp` call starts a daemon when none is running. Somebody
    reading their settings brought a queue into being.

    The live width belongs to `health`; the setting belongs here.
    """
    here = os.getcwd()
    socket = Path("/tmp") / f"jobbox-cfg-{os.getpid()}.sock"
    socket.unlink(missing_ok=True)
    previous = jobbox.SOCKET
    with tempfile.TemporaryDirectory() as tmp, _scratch_state(tmp):
        try:
            os.chdir(tmp)
            jobbox.SOCKET = socket
            with redirect_stdout(io.StringIO()) as out:
                code = jobbox.main(["config"])
        finally:
            jobbox.SOCKET = previous
            os.chdir(here)

    assert code == jobbox.OK
    assert not socket.exists(), "config must not bring a daemon into being"
    assert "new queue" in out.getvalue(), out.getvalue()
