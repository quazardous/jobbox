# Using jbx

The README says what this is and how to install it. This says how to use
it, how it decides what it decides, and every setting it has.

## The verbs

```
jbx run -- '<line>'            run it, detaching after 30s
jbx fg -- '<line>'             run it and NEVER let go — said on purpose
jbx fg <id>                    bring a detached job back to the foreground
jbx queue <intent> -- '<line>' hand it over BEFORE it starts, and name it
jbx ps [--all] [--full] [--json] [--width <n>]
                               what is happening right now, in this project
jbx describe                   every verb and what it does, as JSON
jbx list                       … and what has finished, for a day
jbx status <id>                state, exit code, where its log is
jbx tail <id> [-f]             what it printed
jbx wait <id>                  block until it ends, exit with its code
jbx watch [--all] [--json]     one line per job event, until nothing runs
jbx kill <id>                  stop it, and everything it started
jbx slots [n|none]             how many queued jobs may run at once
jbx after [seconds]            how long a line may hold before detaching
jbx health                     what runs, what is mute, what is stranded
jbx clients                    whose endings are still unread
jbx signals <agent|user>       endings not yet read
jbx gain [project]             what the wrapping bought, and what it cost
jbx gain --thresholds          … and whether 30s is the right cut
jbx gain --since 1h|24h|all    … over a window rather than everything kept
jbx gain --project-path        … with full paths instead of names
jbx config                     every setting, and where it came from
jbx help [id]                  the way in: every verb, or one job
jbx how                        the gestures: what to do, and when
jbx why                        why it works this way
jbx init [--undo] [--global-only]
                               declare the wrapping hook
jbx init --announce            … and the ones that report an ending unasked
jbx init --core                … only the wrapping one, taking the rest back
jbx hook [client]              answers an agent CLI; init declares this one
```

`run` is what the hook calls. You rarely type it.

**Every verb that reads answers `--json`**, and every verb refuses a flag
it does not take. What a verb accepts is declared once — in the table
`jbx describe` publishes — so a flag cannot be accepted without being
documented, nor documented without being accepted.

**A listing says how long ago each job started.** `finished exit 0`
reads the same for something that ended a minute ago and something that
ended yesterday, which is exactly what a list is opened to tell apart.
The column is relative — a clock time is a timezone, and this program
carries no calendar to be right about one; `--json` publishes the
instant itself.

**A listing shows the intent beside the line.** The intent is what the
caller said the job was for — the harness already asks, so it usually
costs nobody anything — and the line is what actually runs, shortened to
fit and stripped of the wrappers it arrived in (`cd <root> &&`,
`timeout <n>`, `rtk proxy`). The name column is only drawn when somebody
named something: a name read off the line would repeat the line.
`--full` gives the line back as recorded, `--json` drops nothing, and
`--width` says how much room to use — by default it asks the terminal,
and falls back to 100 columns for the reader who has none, which is
usually the agent.

## Waiting without standing still

An ending reaches you two ways, and one of them is better.

**Left alone**, it is announced on the next turn. That is free and it
costs a turn's delay — and an agent with nothing else queued has no next
turn at all, so it is left with idling or polling, which are the same
waiting in different clothes.

**Monitored**, it arrives the moment it happens. Hand the waiting to
whatever runs your commands and it wakes you then rather than later:

```console
# one wake-up, when this job ends
jbx wait j7f3a91c            ← as a BACKGROUND command, not a foreground one

# one line per ending, until nothing is running — for a monitor
jbx watch --json
```

`jbx wait` exits when the job does and carries its exit code, so a
background command ends exactly when there is something to say — and it
**clears that job's ending** on the way out, since waiting is the
delivery. Only that job's: the endings nobody has collected stay. `jbx
watch` streams an event per job as it changes and **ends by itself** when
nothing is left running, which is what keeps a watch from staying armed
after the thing it waited for.

Both observe and neither consumes: `jbx signals` destroys what it reports
— right for an agent reading its own mail exactly once, ruinous for a
watcher, which would eat the endings the session is waiting for.

`jbx watch` covers the failures and not only the happy path. A watch that
speaks only on success is silent through a crash, and silence looks
exactly like "still running".

## Letting go is what gives you a grip

Detaching is usually sold as time handed back. It is also the moment a
running line acquires a **name** — and the name is what lets you act on
it while it still runs.

A foreground line has none. Once it starts you are committed to whatever
it does, up to its timeout, including the part you would have stopped had
you known.

```console
jbx list                     # what is running, and under which id
jbx kill j7f3a91c            # stop it, and everything it started
```

Measured on 08/09/2026: a three-step `az` line detached at sixty seconds.
The freed turn was spent reading a document, which said the third step
would create a billable resource that could not be used at all. The job
was killed between the second step and the third. Nothing had judged that
line long — that is precisely the point — and nothing else in the session
could have reached it.

**"And everything it started" is not decoration.** A shell line is
normally one process that spawns others; signalling the parent alone
leaves the real work running under a new one. `kill` tears down the
group.

## The one judgement left to make

The old answer to "when should this go to the background?" was a document
telling an agent to estimate how long a command would take. Agents get
that wrong, and so do people.

jbx removes the question. What is left is a smaller one, and it is asked
where the agent will read it — in the tool's own output, once per
session:

> Do you need this result **before you can do anything else**?

Almost always, no: let it run, and jbx hands the shell back if it drags.
When the answer is yes, say so — `jbx fg -- '<line>'` runs without ever
letting go, and `jbx gain` counts what that cost. A habit of reaching
for it becomes visible instead of invisible.

Changed your mind halfway? `jbx fg <id>` picks a detached job back up:
everything it has printed, then what it prints next, then its exit code.

## Why the cut is at thirty seconds

It was picked out of the air, and then measured. Replayed over 5,165 real
readings — median 0.1s, p90 18s, p99 2m08s:

| cut | detached | of those, done within 10s | you waited | would have saved |
|---|---|---|---|---|
| 10s | 919 | 466 — **51%** | 3h47m | 8h07m (68%) |
| 15s | 614 | 259 — **42%** | 4h49m | 7h05m (60%) |
| **30s** | 272 | 79 — **29%** | 6h33m | **5h22m (45%)** |
| 60s | 118 | 15 — 13% | 8h00m | 3h54m (33%) |

**Thirty seconds is the first round number above the p90**, and that is
the principle rather than the number: the cut belongs where nine lines in
ten have already finished, so detaching stays the exception. At fifteen
it sits *below* the p90 and detaching becomes ordinary.

Lowering it is not free. Fifteen seconds hands back 1h43m more — a third
again — while pushing pointless detachments from 29% to 42%: a job that
lets go, announces itself, takes an id, and finishes moments later. Each
of those costs an announcement, tokens, and often a wait armed for
nothing.

`jbx gain --thresholds` replays this against your own readings, and
`jbx after <n>` sets the cut for one project — which is where the answer
belongs, since it depends on what a wasted detachment costs you.

## Two doors, and they are not the same door

**`run` wraps a command that was going to run either way.** It holds
nothing back, so there is nothing to queue and no cap to apply —
detaching a line does not change how many processes exist.

**`queue` takes work that has not started.** That can wait its turn, so
`jbx slots` holds it: a loop that files fifty jobs does not start fifty
at once. It is also the only place a name is required — somebody choosing
to hand work over has one in mind, and three words at that moment make a
list readable three hours later.

## Two numbers, and they answer different halves

`given back` is time that ran while you were free. `reached for` is how
many detached jobs were later killed, brought back, or read.

The second exists because the first missed something. Detaching hands
back a **window** and hands over a **name**, and only the window was
counted — so a job killed between its second step and its third, on the
strength of what the freed window turned up, showed here as a few
seconds saved.

It counts the act, not its worth: whether reaching for a job was a good
idea is not knowable from a ledger, and a number invented to look like
value would be worse than none.

A job killed before it finished leaves no reading of its own — the run
is recorded when it ends — so `reached for` can name a job that
`detached` never counted. Worth knowing before reading the two together.

## `saved` is a ceiling, and it says so

Detaching a job you then stand and wait for saved nobody anything, and a
tool that counted it would be reporting its own good intentions — so
`saved` subtracts what you handed back to `jbx wait`.

What it cannot see is somebody waiting *some other way*. So it is an
upper bound made as tight as the evidence allows — a ceiling, not a
receipt, and the line under the table says so.

**Each row counts its own calls** — a parent does not total its
children, so the figures never say the same minute twice.

**Projects nest, because they nest on disk.** A repository inside a
repository is the ordinary case, and a flat list hides it exactly where
it matters. `--project-path` prints the roads instead of the names, and
two projects that share a name get four characters of their path to tell
them apart.

It never stores a command line as typed: `TOKEN=… ./deploy` is recorded
as `./deploy`. A truncated secret is still a leaked prefix, so
assignments are dropped whole.

## More than one agent CLI

`jbx hook` answers **Claude Code** by default and **Gemini CLI** as
`jbx hook gemini`. Both are declared the same way — one entry in that
client's settings, pointing at this binary.

They disagree on nearly every word. Gemini calls the shell tool
`run_shell_command` where Claude calls it `Bash`, names the event
`BeforeTool` rather than `PreToolUse`, and **merges** the object we send
into the model's arguments where Claude **replaces** it outright. That
last one matters: under Claude a field left out is a field deleted, so
everything is echoed back, `timeout` included.

Those shapes were **measured**, by sending payloads at each client and
reading the answer — which is how two of them turned out to key on a
tool name other than `Bash` after first looking simply broken. They are
published by `jbx describe` under `x-jbx-dialects`, and a test holds
them still, so changing one without re-measuring fails a build rather
than a session.

**Only one hook is needed**, and that is why this list can grow cheaply.
jbx once declared four events: one to wrap a line, three to carry its
ending back. But `jbx wait <id>` is an ordinary command that exits when
the job does — run in the background by whatever runs your commands, it
delivers the ending through no hook at all, and the detachment message
says so at the moment it matters. What the other three bought was the
**unasked** announcement, and that is what `jbx init --announce` is for.

`jbx hook --list` names them, with the tool each watches for and the
file it declares in. `jbx init --cli gemini` writes the entry there —
and `--undo`, `--core` and `--announce` all follow the same address.
`--announce` uses each client's own event names, and tells you when a
client has no equivalent rather than declaring names it never sends.

An unknown name is refused rather than treated as Claude: a hook
speaking the wrong dialect answers nothing and looks perfectly healthy,
which is the one failure worth being loud about.

## It composes with rtk, rather than racing it

[rtk](https://github.com/rtk-ai/rtk) rewrites commands to spend fewer
tokens, from its own `PreToolUse` hook. Two hooks that both rewrite the
same field are two writers of one value, in an order no harness
documents: whoever writes last erases the other.

So `jbx init` unregisters rtk's hook and **calls it directly**, on the
original line, before wrapping. Both effects, every time, no race to win.
`jbx init --undo` puts its registration back exactly.

Set `compose: never` and jbx leaves rtk alone — including its hook, which
`init` then does not touch: unregistering a tool it has also decided not
to call would remove it from the machine outright.

## What it does not do

- **It does not predict.** A rule that guessed which commands would be
  long was built, measured and refused: replaying 136 real calls, no
  threshold recovered more than 0.7 of the 28 minutes — four of the five
  long shapes had been seen exactly once. → [CONTRIBUTING.md](CONTRIBUTING.md)
- **It gets out of the way of a terminal.** With a tty the line goes
  straight to a shell and jbx stops existing. Everything that makes
  wrapping safe is a fact about a terminal that is not there.
- **It is not a scheduler.** No dependencies between jobs, no retries, no
  calendar.
- **It does not spend your permissions.** The wrapped line is what the
  harness asks you about, so an existing rule like `Bash(cargo test:*)`
  stops matching and you are asked where you were not before. A hook can
  silence that by answering `allow` for itself — jbx never does. It wraps
  *every* command, so allowing on your behalf would allow all of them,
  including the one you would have refused. The extra prompt is the cost
  of keeping that answer yours.

## Where it keeps things

```
~/.jobbox/readings.jsonl        what `jbx gain` counts
~/.jobbox/displaced-hooks.json  what `jbx init --undo` puts back
~/.jobbox/config.yaml           what you wrote
~/.jobbox/cache/                logs, job records, mailboxes
```

**Only `cache/` is safe to delete**, and that is the point of the split.
The readings and the record of displaced hooks used to live in
`~/.cache`, whose contract is that it may be emptied at any hour — so
weeks of measurement and the ability to uninstall depended on nobody
tidying up. An older layout is moved here by itself, once, the first
time jbx runs.

`JBX_DIR` moves the whole house; the settings file stays where it is,
since the setting that says where things go cannot live where it points.

## Settings

**jbx works everywhere by default.** A project says otherwise in a
`.jbx.yaml` at its root — found by walking up to the nearest `.claude`
or `.git`, so it still applies three directories down:

```yaml
enabled: false          # jbx stays out of the way in this project
after: 60               # …or just wait longer here
integration:
  rtk:
    compose: auto       # auto | always | never
```

`jbx init` writes both files for you when they are missing — the global
one, and this project's — **fully commented, so they change nothing**.
The project's records what `rtk --version` actually answered on the day
it was written, rather than what somebody assumed later. `init --undo`
leaves your `.jbx.yaml` alone: it may have been edited, and it may be
committed. **The project file wins key by
key** — naming one setting does not silence the others — and an
environment variable wins over both, because it is what you typed for
this one run.

| variable | key | what it sets |
|---|---|---|
| `JBX_ENABLED` | `enabled` | whether jbx does anything here at all |
| `JBX_AFTER` | `after` | seconds before a line is detached (`30`) |
| `JBX_DIR` | `dir` | where jbx keeps its house (`~/.jobbox`) |
| `JBX_SLOTS` | `slots` | how many QUEUED jobs run at once (`none` for no cap) |
| `JBX_MUTE_AFTER` | `mute_after` | seconds of silence before a job is called mute (`600`) |
| `JBX_WIDTH` | `width` | columns a listing draws in (`auto` asks the terminal) |
| `JBX_RTK` | `integration.rtk.compose` | `auto`, `always`, or `never` |
| `JBX_COLOR` | `color` | `auto`, `always` or `never` (`auto` = when a terminal reads) |
| `JBX_SHELL` | `shell` | which shell runs a line (`cmd` for Windows' own) |
| `JBX_CLIENT` | — | pins one fixed mailbox |
| `JBX_CONFIG` | — | read a different global file |

`jbx config` prints every value, where it came from, and which files it
would be edited in.

## `jbx wait` is for Monitor

Waiting is legitimate — for the caller that genuinely has nothing else
to do, `jbx wait <id>` ends exactly when the job does, so the ending
wakes whatever is watching. **Handed to Monitor**, that is the fastest
an ending can reach you. Run in front of you, it is the waiting the
detachment had just removed.

**The detachment message does not offer the command**, and that is
deliberate: saying "do not wait" and then handing over the line that
waits is a contradiction, and an agent resolves it the easy way — by
pasting what it was given. `jbx help <id>` lists it, one step further
away, along with everything else that can be done with the job.

Where the habit has already set in, a project can take the foreground
use away:

```yaml
allow_wait: false      # `jbx wait` is for Monitor, not the foreground
```

`jbx wait` then refuses with exit 2 and names Monitor instead. Nothing
else changes: endings are still announced on the next turn, `jbx watch`
still reports them, `jbx status` still says where a job is.

**It is on by default**, because on a client with no end-of-turn hook —
Cursor, today — `jbx wait` is the only way an ending reaches anyone at
all.

## What the wrapping costs

`jbx gain` says what the detaching bought. Saying that without saying
what it costs is half a sentence, and the flattering half.

```console
$ jbx bench
  what the wrapping costs, over 200 interleaved runs

  the hook, on every command             6.2 ms
  the wrapper, on a short line          26.0 ms
    and at its slowest                  29.4 ms   ← nine runs in ten
  so a short command pays               32.0 ms

  for comparison, a bare `/bin/sh -c true` takes 1.7 ms
```

**The hook is the number to judge it by.** It runs on every command an
agent issues, including the overwhelming majority that finish instantly
and are never touched again — six milliseconds each, and nobody escapes
it. The wrapper's twenty-six is paid only by commands that actually run
through `jbx run`.

Two spawns of the same binary and a log file is what those milliseconds
are. There is no polling floor hiding in them: the first poll comes
after 500µs.

**The detached path is deliberately not timed.** A command that outlasts
thirty seconds is dominated by itself by four orders of magnitude, and
quoting an overhead percentage against it would be arithmetic designed
to look good.

`jbx bench --json` prints the same numbers for a regression check. The
figures above were measured on one Linux machine; run it on yours,
because that is the only number that concerns you.

## Tests

```console
$ cargo test
```

They go through the command, never through the function. What this tool
is worth is what happens **between** processes: a child that outlives its
parent, an exit code written by one and read by another, a hook answering
a harness on standard output. Wires are all there is here.
