# Contributing to jobbox

## Run the tests first

```console
$ python3 tests/run.py     no dependency — the Python you already have
$ pytest tests/            if you have it
```

Both runners execute the same files. `tests/run.py` exists because jobbox
means to be publishable: requiring `pytest` to *check* a single-file tool
would contradict that.

Some tests start a real `task-spooler` daemon on their own socket, so
`tsp` must be installed. They never touch a live queue — if `tsp` is
missing they say so and skip.

## What belongs in a test

The suite deliberately does not test `task-spooler`. It covers the places
that can be wrong **in silence** — where a defect produces a plausible
answer instead of an error:

- reading `tsp`'s table, whose column count changes with the job state;
- consuming signals, where a double read repeats an announcement and a
  lost one is simply never mentioned;
- merging into a settings file that belongs to other tools;
- the real `TS_ONFINISH` chain.

If a change cannot be wrong quietly, it probably does not need a test.

**Reference output is captured from a real `tsp`, never written from
memory.** A format reconstructed by hand tests the author's memory.

## Style

- **English everywhere** — identifiers, comments, docstrings, commit
  messages, documentation.
- **Comments say *why*, not *what*.** The code already says what it does;
  what it cannot say is which alternative was rejected and what went
  wrong the last time. A comment that repeats the line below it is noise;
  one that names a measured failure is the most valuable line in the
  file.
- **One file, and no dependencies.** `jobbox.py` is the tool, and it
  imports nothing outside the standard library — `python3 -S jobbox.py`
  must keep working. Both constraints are what make it copyable, and
  they keep the scope honest.

## Scope

jobbox is a thin wrapper. `task-spooler` holds the queue, keeps the exit
codes and writes the logs.

**If a change requires reimplementing something `tsp` already does, it is
probably the wrong direction.** Job dependencies, retries and scheduling
are deliberately absent.

### What was already ruled out

Proposing one of these is fine; proposing it without knowing it was
turned down is what wastes everyone's time. If you disagree with a call,
say so rather than working around it.

- **systemd `--user` units.** They give the queue, the logs and survival
  for free. Diverting a service manager for a throwaway script makes
  something that lives ten minutes pay for a unit model.
- **tmux, or any dashboard.** Watching things run is precisely what this
  tool exists to stop doing. A `tail` is enough.
- **Writing the queue ourselves.** `tsp` already does the ordering, one
  output file per job, the exit code, and adjustable parallelism.
- **A heartbeat emitted by the scripts.** It would only work for scripts
  we write, and whoever forgets it looks dead. Log freshness rewards the
  one who says where it is at without demanding anything of the others.
- **One queue per client.** Ordering and parallelism are a machine-level
  resource; N daemons would let N clients start N heavy jobs at once,
  which is what a queue prevents. Only the *consumption* of endings is
  split, because that is the part that actually conflicts.
- **A hook that sends long commands to the background by itself.** Built
  as far as a dry run, then dropped on the evidence. Replaying 136
  measured calls, no rule at any threshold or fingerprint granularity
  recovered more than 0.7 of the 28 minutes spent waiting on long
  commands — because four of the five long shapes had been seen exactly
  once. A retrospective rule cannot catch a command it has never seen,
  and long commands are almost always new ones. The measurement that
  showed this is still there (`jobbox timings --detail`); the guard is
  not.
- **No cap on parallelism by default.** Defensible, and it lost
  narrowly: most queued work is I/O-bound and half the cores throttles
  it for nothing — but the caller here is usually an agent, and an
  unbounded queue driven by one does not survive a loop that files fifty
  jobs. `JOBBOX_SLOTS=none` is one word away.

Everything else worth knowing lives next to the code that does it. This
project had a separate design document for a while; it was deleted
because it kept being a second copy of something, and it was wrong four
times in a single day — a harness limit nobody had tested, when hooks
take effect, an argument the code had already reversed, and its own
version number.

## Changes worth a CHANGELOG entry

Anything a user, integrator or operator would notice. Internal cleanups
do not belong there — the commit log covers them.

Version bumps follow SemVer: a new flag or command is at least MINOR, a
pure bug fix is PATCH, and a renamed field or removed flag is MAJOR.
