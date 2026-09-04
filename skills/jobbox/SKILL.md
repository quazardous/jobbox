---
name: jobbox
description: Waiting on a long command in the foreground is time thrown away. Past a minute of expected work it goes to the background — jobbox if it must outlive the session, the harness's own background if not. Read before starting a build, a sweep, a deploy, or a long query.
---

# What takes time is not waited on

An agent — or a person — waiting on a three-minute script does nothing
for three minutes. Each one is short; it is their sum that costs.

**Past roughly a minute of expected work, it goes to the background.**

## The question that decides, and it is short

> *Do I need this result for the NEXT command?*

**Yes** — foreground, and too bad. A debugging session in progress, a
check that determines the next move.

**No** — and this is the common case — it goes to the background and you
carry on.

The trap is not the ten-minute build; nobody runs that in the foreground
twice. It is **the forty-second command run thirty times**: each too
short to stop for, and their sum is the half hour.

## Two tools, and one question picks between them

> *Does this have to outlive the session?*

### No → the harness's own background

Many harnesses can detach a command themselves and wake the model when
it ends, inside the same turn. Nothing external can do that, because the
loop belongs to the harness. It is the right choice for anything you
start and consume yourself.

Its limit is real: it dies with the session, a person watching does not
see it, and nothing keeps a trace.

### Yes → jobbox

```
jobbox run <intent> -- <command>    queue it, print the id
jobbox list                         waiting · running · finished
jobbox status <id>                  state, exit code, duration, log
jobbox tail <id> [-f]               the log
jobbox kill <id>                    stop it
jobbox health                       is the daemon there, who is mute
jobbox clients                      whose endings are still unread
```

A deploy, a reindex, a sweep you want to find again tomorrow.

**The intent is mandatory, and that is deliberate.** A queue of six
`bash -c …` lines cannot be read back. Three words at the moment you
have them in mind make the list useful three hours later.

**`--` separates, and it is not decorative.** Without it, your command's
own options are read as jobbox's.

**`health` says who is STUCK, not just who is running.** It reads the
date of the last byte written to the log, so a job that has said nothing
for ten minutes gets named. A script that says where it is at gets
precise liveness for free; one that stays silent gets "I do not know",
which is the honest answer.

## Three things to know before relying on it

**The queue is shared by every session on the machine.** That is the
point — ordering and parallelism are a machine-level resource. A new
queue opens at half the cores; `jobbox slots` reads and sets it. A job
sitting in `queued` is not stuck, it is waiting its turn, and `health`
says how many are ahead of it.

**Each session drains only its own endings**, so two of them do not take
each other's notifications. The human's copy is shared on purpose: one
person wants every ending, whichever session started it.

**The queue dies with its daemon, and the ids die with it.** What was
waiting is lost, and numbering restarts at zero — so an id held across a
restart can name a different job. `status` prints the intent, which is
what tells you. Within one daemon's life they are stable, and that is
the only window in which the queue exists.

## What it is not

**It is not a scheduler.** No dependencies between jobs, no retries, no
calendar. If a change would need reimplementing what `task-spooler`
already does, it is probably the wrong direction.

**It is not something you must remember to check.** `jobbox init` wires
hooks that announce a finished job on their own, to the model and to the
person separately. If you find yourself running `jobbox list` to see
whether something finished, the wiring is missing — not your memory.
