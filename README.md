# JobBox

### Time is money. Your agent spends both, standing still.

A five-minute build runs. The agent waits. You wait. Nothing else
happens — and you are billed for all of it, twice: your hour, and the
tokens burning in a session that is doing nothing.

No single wait is worth stopping for. **It is their sum that costs**, and
the sum is invisible until something counts it.

```console
$ jbx stats
project  calls  detached  elapsed  blocked  saved
acme-api    142        11    3h04m   35m12s   2h29m (81%)

2h29m saved — command time that ran while the caller was free, 81% of 3h04m.
```

That is one week. Put your own rate on it.

---

**JobBox wraps every command your agent runs.** The quick ones come back
untouched — output as it is written, exit code unchanged, as though
nothing were there. The slow ones **detach themselves**, say so, and tell
whoever needs to know when they end.

Nobody judges in advance which is which. That judgement is the thing
everybody gets wrong, so JobBox does not make it: it runs the line and
finds out.

The command is `jbx`.

One binary. Rust, `serde_json`, nothing else. Linux and Windows.

## Quickstart

**Download the binary** — under a megabyte, nothing to install — from the
[releases](https://github.com/quazardous/jobbox/releases): Linux
(static, so distribution age does not matter), Windows, macOS. Put it on
your `PATH`, then:

```console
$ jbx init                        # declares its hooks, merges safely
```

Or build it, if you already have Rust:

```console
$ git clone https://github.com/quazardous/jobbox && cd jobbox
$ ./install.sh                    # or .\install.ps1 on Windows
```

On Windows, `winget install Rustlang.Rustup` does get you cargo — but
rustup then wants the Visual Studio build tools to link with, which is
gigabytes of prerequisite for a program this size. Take the binary
unless you were going to write Rust anyway.

Open a new session. Nothing changes — until something is slow:

```console
$ npm run build
> building…
jbx: still running after 30s, so it was detached as j7f3a91c. Nothing
was lost and it is still going; what it prints keeps going to its log.
  jbx status j7f3a91c   where it is, and its exit code once it lands
  jbx tail j7f3a91c     what it has printed so far
  jbx wait j7f3a91c     block here until it ends, and exit with its code
Prefer doing something else and coming back: that is what detaching it was for.
```

The build output arrived **as it was written**, not replayed at the end.
When it finishes, the model is told on its next turn and you are told
when the session stops — a failure holds the session open and points at
the log.

## The verbs

```
jbx run -- '<line>'            run it, detaching after 30s
jbx fg -- '<line>'             run it and NEVER let go — said on purpose
jbx fg <id>                    bring a detached job back to the foreground
jbx queue <intent> -- '<line>' hand it over BEFORE it starts, and name it
jbx list                       what is detached, and how it went
jbx status <id>                state, exit code, where its log is
jbx tail <id> [-f]             what it printed
jbx wait <id>                  block until it ends, exit with its code
jbx kill <id>                  stop it, and everything it started
jbx slots [n|none]             how many queued jobs may run at once
jbx health                     what runs, what is mute, what is stranded
jbx clients                    whose endings are still unread
jbx signals <agent|user>       endings not yet read
jbx stats [project]            how much time was saved
jbx stats --project-path       … with full paths instead of names
jbx config                     every setting, and where it came from
jbx how [id]                   what you can do with it, right now
jbx why                        why it works this way
jbx init [--undo]              declare the hooks
```

`run` is what the hook calls. You rarely type it.

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
letting go, and `jbx stats` counts what that cost. A habit of reaching
for it becomes visible instead of invisible.

Changed your mind halfway? `jbx fg <id>` picks a detached job back up:
everything it has printed, then what it prints next, then its exit code.

## Two doors, and they are not the same door

**`run` wraps a command that was going to run either way.** It holds
nothing back, so there is nothing to queue and no cap to apply —
detaching a line does not change how many processes exist.

**`queue` takes work that has not started.** That can wait its turn, so
`jbx slots` holds it: a loop that files fifty jobs does not start fifty
at once. It is also the only place a name is required — somebody choosing
to hand work over has one in mind, and three words at that moment make a
list readable three hours later.

## `saved` is a ceiling, and it says so

`blocked` already subtracts the time handed back to `jbx wait`. Detaching
a job you then stand and wait for saved nobody anything, and a tool that
counted it would be reporting its own good intentions.

What it cannot see is somebody waiting *some other way*. So it is an
upper bound made as tight as the evidence allows — a ceiling, not a
receipt, and the line under the table says so.

It never stores a command line as typed: `TOKEN=… ./deploy` is recorded
as `./deploy`. A truncated secret is still a leaked prefix, so
assignments are dropped whole.

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

## On Windows

**Honest first: nobody has run it there yet.** It compiles and lints on
every change, the release binaries are built there, and the test suite
does not run there — the tests drive `sh` and `sleep`. So what follows is
what the code does, not what anybody has watched it do. Reports welcome.

Install `jbx.exe` from the [releases](https://github.com/quazardous/jobbox/releases)
and put it somewhere on your `PATH`, or run `.\install.ps1` from a
checkout. Then `jbx init`, same as anywhere.

**The shell is the part that matters.** The hook rewrites a command into
`jbx run -- '<line>'`, quoted for a POSIX shell — which is right, because
Claude Code on Windows drives Git Bash. So jbx runs the line with `bash`
whenever `bash` is on the `PATH`, on every platform, and falls back to
`cmd /C` only when there is none. `jbx config` prints which one it picked;
`shell: cmd` in the configuration settles it if the guess is wrong for
your setup.

Two things behave differently, by construction rather than by neglect:

- **Files live in `%LOCALAPPDATA%\jbx` and `%APPDATA%\jobbox\config.yaml`.**
- **`jbx health` never calls a job mute for the right reason.** Liveness
  from the log still works; the extra observation about a line reading
  its input needs `/proc`, which Windows has not. It answers "I do not
  know" rather than guessing — the same rule that cost a false alarm on
  Linux.

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
| `JBX_DIR` | `dir` | where logs and records live (`~/.cache/jbx`) |
| `JBX_SLOTS` | `slots` | how many QUEUED jobs run at once (`none` for no cap) |
| `JBX_MUTE_AFTER` | `mute_after` | seconds of silence before a job is called mute (`600`) |
| `JBX_RTK` | `integration.rtk.compose` | `auto`, `always`, or `never` |
| `JBX_CLIENT` | — | pins one fixed mailbox |

`jbx config` prints every value, where it came from, and which files it
would be edited in.

## Tests

```console
$ cargo test
```

They go through the command, never through the function. What this tool
is worth is what happens **between** processes: a child that outlives its
parent, an exit code written by one and read by another, a hook answering
a harness on standard output. Wires are all there is here.

## License

MIT — see [LICENSE](LICENSE).
