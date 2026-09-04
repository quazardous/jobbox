# What actually takes time

`jobbox init` wires a pair of hooks that time every shell command, and
`jobbox timings` reads the table back.

```console
$ jobbox timings --detail
  125 call(s) measured, 18 already detached
  41.9 min spent waiting in the foreground
  across 2 session(s): 92183ccf d4a69872
  5 call(s) started and never closed — these totals are lower bounds

     total  calls   median  shape
      680s     17    39.2s  make test
      300s      1   300.0s  docker compose build

  how the waiting is spread
       2-5s   17 calls      53s   3.7%
      > 60s    5 calls    1014s  71.1%

  what a guard would buy, and what it would interrupt
  at  60s     5 calls (11.1%)  recovers  16.9 min of 23.8
```

**Ranked by total time waited**, not by the slowest single call — the
forty-second command run thirty times can cost more than one ten-minute
build, and ordering by worst case hides exactly that.

**It measures and does nothing else.** No reminder, no refusal, no
rewritten command: a measurement that changes what it measures answers a
different question than the one asked.

## What it will not tell you

The output names the two ways this table misleads, because both happened
before it did:

**One call carrying most of the total** means the numbers describe that
call, not a habit — and a threshold drawn from it would too.

**A distribution from a single session** describes one agent's work.
`--session <id>` reads one at a time; the header says how many are in
there.

And the totals are **lower bounds**: hooks do not fire for every call, so
a stamp with no matching end never becomes a row. The count of unpaired
calls is printed rather than left to be assumed.

## What it costs

About **0.17 s per shell command** — two process starts, measured against
a control rather than in isolation. The measuring path exits before the
rest of the file is even parsed, which is a third of that.

`jobbox timings --reset` forgets everything. Removing the two `observe`
entries from `.claude/settings.json` stops it.

## Why this exists

It was built to settle whether a hook should force long commands into the
background by itself. It settled it: **no**. See "What was already ruled
out" in [../CONTRIBUTING.md](../CONTRIBUTING.md) — the short version is
that four of five long command shapes had been seen exactly once, and no
rule can catch a command it has never seen.

The measurement stayed because it is what answered the question, and what
would answer it again if the work changed shape.
