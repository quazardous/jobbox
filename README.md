# jobbox

**Your agent runs a five-minute build and then sits there.** So do you.
Each wait is short; it is their sum that costs.

jobbox queues the command instead and **tells whoever needs to know when
it ends** — the model on its next turn, you when the session stops.
Nobody has to remember to look.

One file, the standard library, and
[`task-spooler`](https://vicerveza.homeunix.net/~viric/soft/ts/).

## Quickstart

```console
$ sudo dnf install task-spooler          # or apt, brew, pkg…
$ git clone https://github.com/quazardous/jobbox && cd jobbox
$ ./install.sh                           # into ~/.local, no root

$ cd ~/your-project && jobbox init       # wires Claude Code, merges safely
```

If `~/.local/bin` is not on your `PATH`, `install.sh` says so.

Open a new session, then:

```console
$ jobbox run build-the-front -- npm run build
j7f3a91c
```

Go and do something else. When it ends, the model is told on its next
turn and you are told when the session stops — a failure holds the
session open and points at the log.

→ **[doc/claude-code.md](doc/claude-code.md)** for what `init` writes and
how the telling works.

**Not tied to Claude Code.** Two verbs know that harness; everything else
returns facts, and any CLI — or none — can read them.
→ [doc/other-harnesses.md](doc/other-harnesses.md)

## The verbs

```
jobbox run <intent> -- <command>    queue it, print the id
jobbox list [--mine|--all]          waiting · running · finished
jobbox status <id>                  state, exit code, times, log
jobbox tail <id> [-f]               the log
jobbox kill <id>                    stop it
jobbox health                       is the daemon there, who is stuck
jobbox clients                      whose endings are still unread
jobbox config                       every setting, and where it came from
jobbox slots [n]                    how many jobs may run at once
jobbox timings [--detail]           what actually takes time, measured
```

```console
$ jobbox list
        id  state    intent           project          session
  jf4eacbb  running  build-the-front  jobbox-1de7      92183ccf
  j3ca27c8  queued   nightly-backup   imagematch-4a01  d4a69872
```

A column nobody filled is not printed — nothing has exited here, so
there is no `exit` column to skim past.

**The intent is mandatory**, and that is the point: a queue of
`bash -c …` lines cannot be read back three hours later.

**`--` separates.** Without it, your command's own options are read as
jobbox's.

**The id is jobbox's own and never reused** — `tsp`'s numbers restart at
zero when its daemon dies. → [doc/sessions.md](doc/sessions.md)

## Install

`./install.sh --symlink` points the install at this checkout so edits are
live; `--uninstall` removes it and keeps your logs. **No dependencies** —
it runs under `python3 -S`, with site-packages switched off.

| variable | default | what it sets |
|---|---|---|
| `JOBBOX_DIR` | `~/.cache/jobbox` | where the logs go |
| `JOBBOX_SOCKET` | `/tmp/jobbox-<uid>.sock` | which queue to talk to |
| `JOBBOX_CLIENT` | the project and session | pins one fixed mailbox |
| `JOBBOX_SLOTS` | half the cores | how wide a NEW queue opens (`none` for no cap) |
| `JOBBOX_MUTE_AFTER` | `600` | seconds before a running job is called mute |

## What it does not do

- **It does not outlive its daemon.** The queue lives with the
  `task-spooler` daemon; if that dies, what was waiting is lost.
  Acceptable for development work — worth knowing, not worth hiding.
- **It does not replace a scheduler.** No dependencies between jobs, no
  retries, no calendar.
- **It does not decide what is long.** That judgement stays with the
  caller — a hook that decided automatically was built, measured, and
  dropped. → [CONTRIBUTING.md](CONTRIBUTING.md)

## Documentation

| | |
|---|---|
| [doc/claude-code.md](doc/claude-code.md) | wiring, hooks, what gets told to whom |
| [doc/other-harnesses.md](doc/other-harnesses.md) | using jobbox from any CLI, or none |
| [doc/sessions.md](doc/sessions.md) | several sessions on one queue, mailboxes, ids |
| [doc/liveness.md](doc/liveness.md) | running vs making progress, and `health` |
| [doc/timings.md](doc/timings.md) | measuring what your commands actually cost |
| [CONTRIBUTING.md](CONTRIBUTING.md) | scope, tests, and what was ruled out |

## Tests

```console
$ python3 tests/run.py     no dependency
$ pytest tests/            if you have it
```

They cover the places that can be wrong in silence: reading `tsp`'s
table, consuming a signal, merging into a settings file that belongs to
other tools, and the real notification chain against a live daemon on its
own socket.

## License

MIT — see [LICENSE](LICENSE).
