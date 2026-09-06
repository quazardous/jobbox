# JobBox

### Time is money. Your agent spends both, standing still.

A five-minute build runs. The agent waits. You wait. Nothing else
happens — and you are billed for all of it, twice: your hour, and the
tokens burning in a session that is doing nothing.

No single wait is worth stopping for. **It is their sum that costs**, and
the sum is invisible until something counts it.

```console
$ jbx stats
project      calls  detached  elapsed  waited  saved
acme             12         0      14m    13m52s     8s (1%)
  api            96         7    2h11m   18m03s  1h53m (86%)
  front          34         4      39m    03m17s   36m (92%)

2h29m saved — command time that ran while the caller was free, 81% of 3h04m.
`waited` is what you actually stood still for, and `saved` is the rest
of `elapsed` — it already subtracts the time you gave back to `jbx wait`.
It cannot see you waiting some other way: a ceiling, not a receipt.
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

One binary. Rust, `serde_json`, nothing else. Linux, macOS, Windows.

## Quickstart

```console
$ curl -fsSL https://raw.githubusercontent.com/quazardous/jobbox/main/install.sh | sh
$ jbx init                        # declares its hooks, merges safely
```

On Windows:

```powershell
irm https://raw.githubusercontent.com/quazardous/jobbox/main/install.ps1 | iex
```

It downloads the binary for your machine — under a megabyte, nothing
compiled, nothing outside your home — and checks it against the sums
published with the release. `--from-source` builds a checkout instead,
`--uninstall` removes it, `--version=vX.Y.Z` pins one. The
[releases](https://github.com/quazardous/jobbox/releases) hold the
archives if you would rather do it by hand.

Open a new session. Nothing changes — until something is slow:

```console
$ npm run build
> building…
jbx: this passed 30s, so it is now in the BACKGROUND — detached as j7f3a91c.
Nothing was lost. It is still running, and still printing to its log.

DO NOT SIT AND WAIT FOR IT. You will be told when it ends, on a later turn —
waiting here is the exact cost jbx exists to remove. Go and do something else.

  jbx how j7f3a91c   what you can do with it   ·   jbx why   why it works this way
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
jbx ps [--all] [--full] [--json] [--width <n>]
                               what is happening right now, in this project
jbx describe                   every verb and what it does, as JSON
jbx list                       … and what has finished, for a day
jbx status <id>                state, exit code, where its log is
jbx tail <id> [-f]             what it printed
jbx wait <id>                  block until it ends, exit with its code
jbx kill <id>                  stop it, and everything it started
jbx slots [n|none]             how many queued jobs may run at once
jbx health                     what runs, what is mute, what is stranded
jbx clients                    whose endings are still unread
jbx signals <agent|user>       endings not yet read
jbx stats [project]            how much time was saved
jbx stats --thresholds         … and whether 30s is the right cut
jbx stats --project-path       … with full paths instead of names
jbx config                     every setting, and where it came from
jbx how [id]                   what you can do with it, right now
jbx why                        why it works this way
jbx init [--undo]              declare the hooks
jbx hook                       answers the harness; init declares this one
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

**Honest first: the suite runs there now, and does not all pass yet.**
It compiles and lints on every change, the release binaries are built
there, and every change runs the full test suite on a Windows runner —
which is how the first real Windows bug was found rather than reasoned
about: the log was opened append-only, which on Windows grants a handle
Git Bash cannot use, so every line ran perfectly and lost all of its
output. Nobody has yet used it in a real session on a real desktop, so
what follows is still partly what the code does rather than what anybody
has watched it do. Reports welcome.

Install `jbx.exe` from the
[releases](https://github.com/quazardous/jobbox/releases) and put it
somewhere on your `PATH`, or run `.\install.ps1` from a checkout. Then
`jbx init`, same as anywhere.

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
| `JBX_WIDTH` | `width` | columns a listing draws in (`auto` asks the terminal) |
| `JBX_RTK` | `integration.rtk.compose` | `auto`, `always`, or `never` |
| `JBX_SHELL` | `shell` | which shell runs a line (`cmd` for Windows' own) |
| `JBX_CLIENT` | — | pins one fixed mailbox |
| `JBX_CONFIG` | — | read a different global file |

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
