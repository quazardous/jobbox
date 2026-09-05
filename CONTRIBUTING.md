# Contributing to JobBox

The project is **JobBox**; the command it installs is `jbx`.

## Run the tests first

```console
$ cargo test
```

They need nothing installed. `rtk` is deliberately kept off the `PATH`
inside the hook tests: what it rewrites is its business and its version's,
and a test that depended on it would fail the day it learns a new command.

## What belongs in a test

The suite covers the places that can be wrong **in silence** — where a
defect produces a plausible answer instead of an error:

- an exit code that comes back changed, or comes back at all after the
  process that could have reported it is gone;
- output replayed at the end instead of poured through as it is written,
  which looks identical once the command has finished;
- a hook that wraps its own output, burying the real command one level
  deeper on every pass;
- quoting a line into one shell word, where an apostrophe is enough to
  change what it means;
- consuming an ending, where a double read repeats an announcement and a
  lost one is simply never mentioned;
- merging into a settings file that belongs to other tools.

If a change cannot be wrong quietly, it probably does not need a test.

**Every test pins `JBX_DIR` and `JBX_CONFIG` into its own scratch, and
runs from its own directory.** Not tidiness: without the first two, the
`init` tests wrote the real `~/.config/jobbox/config.yaml` and dropped a
`.jbx.yaml` into the repository being tested. Without the third, a
setting on the machine changes what a test measures. A suite that edits
the machine it runs on is a suite nobody can trust.

**The tests go through the COMMAND, never through the function.** What
this tool is worth is what happens between processes: a child that
outlives its parent, a code written by one and read by another, a hook
answering a harness on standard output. A test calling the functions
directly would prove the pieces and miss every wire, and wires are all
there is here.

## Style

- **English everywhere** — identifiers, comments, doc comments, commit
  messages, documentation.
- **Comments say *why*, not *what*.** The code already says what it does;
  what it cannot say is which alternative was rejected and what went
  wrong the last time. A comment that repeats the line below it is noise;
  one that names a measured failure is the most valuable line in the file.
- **Measure, then write it down.** Nearly every decision recorded here
  came from a reading, and the reading is quoted where the decision is.
  A limit written from memory becomes a fact nobody rechecks.
- **Linux and Windows, both.** `cargo check --target
  x86_64-pc-windows-gnu` is part of finishing a change. Where a platform
  cannot answer a question — `/proc` does not exist on Windows — the
  answer is "I do not know", never a guess.

## Scope

JobBox wraps a line and lets go of it when it turns out to be long. It owns
the process work: detaching, supervising, keeping the exit code, the log,
the liveness, the cap on deliberate work.

Job dependencies, retries and scheduling are deliberately absent.

### What was already ruled out

Proposing one of these is fine; proposing it without knowing it was
turned down is what wastes everyone's time. If you disagree with a call,
say so rather than working around it.

- **PREDICTING which commands will be long.** Built as far as a dry run,
  then dropped on the evidence. Replaying 136 measured calls, no rule at
  any threshold or fingerprint granularity recovered more than 0.7 of the
  28 minutes spent waiting — because four of the five long shapes had
  been seen exactly once. A retrospective rule cannot catch a command it
  has never seen, and long commands are almost always new ones.

  **This is not the same thing as what jbx does now, and the difference
  is the whole design.** jbx guesses nothing: it runs the line and finds
  out. That is why it must wrap EVERYTHING — a list of "commands worth
  wrapping" would be the refused rule wearing a different hat.
- **Capping wrapped lines.** A cap holds back work that has not started.
  `run` never holds anything back: the command was going to run either
  way, and detaching it does not change how many processes exist. The cap
  lives on `queue`, which is the only door work goes through before it
  starts.
- **Racing rtk's hook instead of calling it.** Two hooks that both
  rewrite `command` are two writers of one value, in an order no harness
  documents. Finishing later would not help — a hook cannot read another
  hook's output, so being last means erasing the rewrite you waited for.
  `init` unregisters rtk's hook and jbx calls it directly.
- **Teeing through a pipe.** It is exactly live, and it needs somebody
  alive at the other end forever — which is the one thing that stops
  being true the moment a line is detached. The output goes to a file the
  child writes directly, and the front copies what appears. Measured: it
  tracks a bare shell within about ten milliseconds.
- **A heartbeat emitted by the scripts.** It would only work for scripts
  we write, and whoever forgets it looks dead. Log freshness rewards the
  one who says where it is at without demanding anything of the others.
- **A flat polling interval.** Twenty milliseconds looks harmless and put
  20 ms on every command on the machine, because a line taking one
  millisecond still waited a whole tick to be noticed. The first hundred
  milliseconds are watched closely and the rest is not.
- **`task-spooler` as the substrate.** It held the queue for the Python
  this replaces, and it does not exist on Windows. jbx had independently
  reimplemented the output file, the exit code, the job identity and
  survival before anybody decided to — only the cap was left.
- **No cap on parallelism by default.** Defensible, and it lost narrowly:
  most queued work is I/O-bound and half the cores throttles it for
  nothing — but the caller here is usually an agent, and an unbounded
  queue driven by one does not survive a loop that files fifty jobs.
  `jbx slots none` is one word away.

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
