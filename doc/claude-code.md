# jobbox with Claude Code

```console
$ jobbox init
  wrote  SessionStart -> jobbox claude-hook agent text
  wrote  UserPromptSubmit -> jobbox claude-hook agent text
  wrote  Stop -> jobbox claude-hook user stop
  wrote  PreToolUse -> jobbox observe
  wrote  PostToolUse -> jobbox observe
  wrote  env.JOBBOX_PROJECT = myproject-1de7
  wrote  env.JOBBOX_PROJECT_PATH = /home/you/myproject
  wrote  skill -> ~/.claude/skills/jobbox/SKILL.md
  in     /home/you/myproject/.claude/settings.json

  A new session is what reliably arms these. They have also been seen
  taking effect immediately — do not count on it.
```

**Five entries, not three.** The last two time your shell commands —
that is [timings.md](timings.md), and it is separable: removing those two
lines leaves the notification working.

## What gets told to whom

```
a job ends
  └─ tsp calls jobbox, which files one ending per audience

SessionStart        the model, when a session opens
UserPromptSubmit    the model, on its next turn
Stop                you, when the session stops
```

Hooks are read when a session starts. They have also been seen taking
effect immediately in a live one, and then seen not to — so a new session
is the only thing to rely on.

**Two audiences because two readers.** That the model has read a result
does not mean you have seen it — you do not read at the same moment or
through the same channel. Each gets its own copy, and each copy is
delivered exactly once.

**Only `Stop` can reach the model.** The other two hooks print into the
turn's context; `Stop`'s output goes to a debug log unless it *blocks*.
So a failure holds the session open and points at the log, and a success
does not — blocking on every ending would make the session unstoppable.

**And only a failure THIS session caused.** Every ending is still
announced, because the person wants all of them; but blocking does more
than announce — it stops a session and tells the model to go and fix
something. Doing that for a job another session queued sends an agent to
read a log from a project it is not working on.

```
jobbox: one job finished — build-the-front.
jobbox: build-the-front (exit=3). Failed job logs: /tmp/ts-out.x.
        Read them, say what broke, and fix it if it is within reach.
```

`JOBBOX_BLOCK_ON` is not a setting here; the rule is fixed: failures
interrupt, successes are announced.

## What `init` writes, and what it will not touch

Three hook entries, an `env` block naming the project, and a skill in
`~/.claude/skills/jobbox/`.

**It merges.** `.claude/settings.json` almost always carries hooks
belonging to other tools by the time anyone runs this. An `init` that
wrote the file it wanted would remove them, and the loss would be silent
in the worst way: nothing fails at init time, and the missing hook is
noticed the next time it should have fired.

**Re-running it is safe**, and is the point — it recognises its own
entries by verb, so a declaration that gains a flag replaces the old one
instead of piling up beside it. `--force` rewrites them even when
unchanged.

**An unreadable settings file is refused, not replaced.** Unparseable
JSON means somebody is mid-edit; overwriting is exactly the case where
overwriting destroys the most.

**The skill is never overwritten** without `--force`: it is written
outside the project, into something you may have edited.

## The skill

`init` installs one, globally, describing *when* a queue is the right
move — the judgement that does not fit in `--help`. Roughly: past a
minute of expected work, and if you do not need the result for the very
next command.

It also names the alternative honestly. Claude Code can detach a command
itself with `run_in_background`, and wake the model when it ends, inside
the same turn — no external tool can do that, because the loop belongs
to the harness. jobbox is for what must **outlive the session**.

## Not tied to Claude

The two verbs above are the only ones that know this harness.
Everything else returns facts — see
[other-harnesses.md](other-harnesses.md).
