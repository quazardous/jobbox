# jobbox

Start a long command, and find it again later.

```
jobbox run <intent> -- <command>    queue it, print the id
jobbox list                         waiting · running · finished
jobbox status <id>                  state, exit code, duration, log
jobbox tail <id> [-f]               the log
jobbox kill <id>                    stop it
jobbox health                       is the daemon there, who is mute
jobbox clients                      who has a mailbox, and what waits in it
jobbox slots [n]                    how many jobs may run at once
jobbox timings                      what actually takes time, measured
```

```console
$ jobbox run build-the-front -- npm run build
0
$ jobbox list
     0  running   build-the-front
$ jobbox status 0
  intent     build-the-front
  state      finished
  exit       0
  times      41.20/38.11/2.44
  log        ~/.cache/jobbox/ts-out.ysNV5u
```

> **Taking this over?** The *why* lives next to the code that does it,
> and `CONTRIBUTING.md` lists what was already ruled out.

## What it is, and what it is not

A thin wrapper around [`task-spooler`](https://vicerveza.homeunix.net/~viric/soft/ts/),
which already does the essentials: an ordered queue, one output file per
job, the exit code kept, adjustable parallelism.

**jobbox adds three things, and nothing more.**

### The intent

`tsp -l` returns lines of `bash -c …` that cannot be read back. A
mandatory name costs three words at the moment you have them in mind, and
makes the list useful three hours later.

### Liveness

`tsp` says whether a job **runs**, never whether it **makes progress**. A
stuck script and a computing one are the same `running`.

So jobbox reads the date of the last byte written to the log:

```
running + fresh log           it is working
running + nothing for a while MUTE?   ← what `health` names
not running                   done, and the code says how
```

**It is a date, not a heartbeat to emit.** A heartbeat would only work
for scripts you write yourself, and whoever forgets it would look dead.
Log freshness simply rewards the one who says where it is at — and asks
nothing of the others.

`health` returns `0` even on a mute job: it may be a script computing
without saying anything, and returning a failure would make `health` an
alarm you switch off.

### The notification

`tsp` accepts `TS_ONFINISH`: a program run when a job ends, with
`jobid errorlevel outputfile command`. jobbox hooks a shim there that
drops one signal per audience.

```
jobbox signals agent           consume and return what has finished
jobbox signals user --json     one JSON line per job, for an integrator
```

**Reading and erasing are one gesture.** Nothing speaks again until the
next ending: the backoff is structural, there is no date of last look to
keep, no list of already-announced jobs, no fingerprint to compare. Each
job ends exactly once — so nothing can be announced twice, nor missed.

**Two audiences, two mailboxes**, because whoever automates and whoever
watches read neither at the same moment nor through the same channel.

## Several sessions at once

The queue is shared on purpose: ordering and parallelism are a
**machine-level** resource, and that is the whole reason `task-spooler`
exists. Giving every session its own daemon would let N sessions start N
heavy jobs at once — the problem the queue was there to prevent.

What must not be shared is the **consumption** of endings, since reading
erases them. **Sessions name themselves**, so that separation costs no
configuration at all — a default you have to switch on is a default that
stays off.

```console
$ jobbox run migrate -- ./migrate.sh      # in one session
$ jobbox run reindex -- ./reindex.sh      # in another
$ jobbox list
     0  running   migrate
     1  queued    reindex                   (cc-bbbbbbbb)
```

Each session drains only its own endings. `JOBBOX_CLIENT` pins one fixed
name instead — right for a CI runner or a shared worker, wrong for a
person with two windows open. Outside any session, and with nothing set,
jobbox behaves exactly as it did before clients existed.

The client travels inside the job's label, which is the only thing that
reaches `TS_ONFINISH` — `tsp` passes it no environment.

### The human's mailbox is not split

**The human is one person.** They want every ending, whichever session
started it, and they read through whatever session happens to be open.
Splitting their mailbox too would mean a job launched by a session that
has since closed is announced to nobody — a loss that per-client
mailboxes would have *introduced* while fixing the agent's.

So the split follows the reader, not the file. Both are still consumed
exactly once.

### Seeing what nobody came back for

```console
$ jobbox clients
  (user, shared)               2
  cc-aaaaaaaa                  agent=1
  cc-bbbbbbbb                  empty          ← you
```

Splitting mailboxes opens a quieter failure: a session that ends leaves
its endings behind, and an unread file looks exactly like an empty one.
Naming sessions automatically makes that worse, since every session
creates one.

The empty remains are **forgotten without being asked** — from `clients`
and behind every job that ends. Three conditions, and each prevents a
loss rather than being tidiness: never one that still holds an ending
(the only evidence a job finished unannounced), never your own, and
never one touched in the last hour — `onfinish` creates a directory and
then writes inside it, and removing it in between would lose that
ending.

### How wide the queue is

```console
$ jobbox health
  daemon     alive, 4 job(s) known
  slots      1/1 busy, 2 waiting
             one slot — jobs run strictly one after another
```

A queue jobbox opens itself starts at **half the cores**. `JOBBOX_SLOTS`
changes that — a number, or `none` for no cap at all.

The width is applied **only at the daemon's birth**, so a deliberate
`jobbox slots 2` is never quietly undone by the next command. And it is
machine-wide: every session on the account shares it, which is the point
of a queue rather than N daemons.

## What actually takes time

Whether anything *should* force long commands into the background is an
open question, and the belief "this one is long" has been wrong in both
directions. So `jobbox init` also wires a pair of hooks that time shell
commands, and `jobbox timings` reads the table back:

```console
$ jobbox timings
  312 call(s) measured, 18 already detached
  22.4 min spent waiting in the foreground

     total  calls   median  shape
      680s     17    39.2s  make test
      300s      1   300.0s  docker compose build
```

`jobbox timings --detail` adds the reading itself — how the waiting is
spread, what a guard at each threshold would buy and interrupt, and the
per-session split. It names an outlier carrying most of the total, and
says when a distribution comes from a single session: both are ways a
table decides something it should not.

**Ranked by total time waited, not by the slowest single call.** The trap
is not the ten-minute build — nobody runs that in the foreground twice.
It is the forty-second command run thirty times: each one too short to
stop for, and their sum is the half hour.

**It measures and does nothing else** — no reminder, no refusal, no
rewritten command. A measurement that changes what it measures answers a
different question.

**It never stores the command line.** A command can carry a secret
inline, and this table sits in a cache directory for weeks; only the
shape is kept, an assignment is dropped rather than truncated, and
everything after a shell's `-c` is discarded.

It costs about **0.17 s per shell command** — two process starts,
measured on one machine against a control, not in isolation. The
measuring path exits before `click` is imported and before the imports
only the full tool needs, which together is 46% of what it cost at
first. `jobbox timings --reset` forgets
everything, and removing the two `observe` entries from
`.claude/settings.json` stops it.

## Install

```console
$ sudo dnf install task-spooler                  # or apt, brew, pkg…
$ ./install.sh                                   # into ~/.local, no root
$ jobbox health
```

`install.sh --symlink` points the install at this checkout instead, so
edits are live. `install.sh --uninstall` removes it and keeps your logs;
add `--purge` to delete those too. `jobbox --version` says which copy is
on your PATH.

One file, Python 3.11+, and `tsp` on the `PATH`. **No dependencies** —
it runs under `python3 -S`, with site-packages switched off entirely.

| variable | default | what it sets |
|---|---|---|
| `JOBBOX_DIR` | `~/.cache/jobbox` | where the logs go |
| `JOBBOX_SOCKET` | `/tmp/jobbox-<uid>.sock` | which queue to talk to |
| `JOBBOX_CLIENT` | the session's own id | pins the name your mailbox answers to |
| `JOBBOX_SLOTS` | half the cores | how wide a NEW queue opens (`none` for no cap) |
| `JOBBOX_MUTE_AFTER` | `600` | seconds before a running job is called mute |

## Wiring it into a project

jobbox knows no harness in its core — `signals` returns facts, and
shaping them belongs to whoever integrates. But leaving it there cost the
tool its usability: it was publishable and nobody could wire it without
writing the bridge themselves.

So two verbs, named for what they know, carry all of it:

```console
$ jobbox init
  wrote  SessionStart -> jobbox claude-hook agent text
  wrote  UserPromptSubmit -> jobbox claude-hook agent text
  wrote  Stop -> jobbox claude-hook user stop
```

`--client` is available and is the exception: it pins one name for every
session in the project. Without it each session names itself, which is
what you want unless the project *is* a single shared worker.

`init` also installs a **skill** into `~/.claude/skills/jobbox/` — when
to reach for a queue at all, which is not derivable from `--help`. It is
never overwritten unless you pass `--force`, so an edited copy is yours
to keep.

`init` **merges** into `.claude/settings.json` — it never removes hooks
belonging to other tools, and running it again after an upgrade is safe.
**Open a new session to arm them.** They have been seen taking effect
immediately in a live one, and then seen not to — so a new session is
the only thing to rely on.

For any other harness, build on `jobbox signals <audience> --json`, which
returns one object per ending:

```json
{"id": "7", "code": "0", "log": "/tmp/ts-out.x", "client": "cc-aaaaaaaa",
 "intent": "build-the-front", "command": "npm run build",
 "finished_at": 1756900000.0}
```

## Two `tsp` traps, measured

They are in the code, with their reason — but they are worth knowing to
anyone building on `tsp`.

**It segfaults** when it cannot create its socket. It prints
`Probably, the name is too long`, then drops a core. A Unix socket is
capped at ~108 characters by the kernel. jobbox therefore sets a short,
explicit `TS_SOCKET`, and refuses an over-long one with a sentence rather
than letting `tsp` die on it.

**`TMPDIR` governs both** — the socket *and* the output files.
Decoupling them is what lets you keep logs wherever you want without
making the socket path longer.

## The tests

```console
$ python3 tests/run.py     no dependency
$ pytest tests/            if you have it
```

They cover the places that can be wrong **in silence**: reading `tsp`'s
table (the number of columns changes with the state — a naive split would
report `exit=0` on a script that failed), consuming signals (read twice =
a repeated announcement; erased without being returned = a lost ending),
merging into a settings file that belongs to other tools, and the real
`TS_ONFINISH` chain against a live daemon on its own socket.

The reference lines are **captured from a real `tsp`**, not reconstructed
from memory.

## What it does not do

- **It does not outlive its daemon.** The queue lives with the `tsp`
  daemon; if it dies, what was waiting is lost. For development commands
  that is acceptable — it needs to be known, not hidden. `jobbox health`
  says when the daemon it is talking to was started by the check itself,
  which is the closest thing to noticing.
- **It does not replace a scheduler.** No dependencies between jobs, no
  retries, no calendar.

## License

MIT — see [LICENSE](LICENSE).
