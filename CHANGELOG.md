# Changelog

All notable changes to this project will be documented in this file.

> This is a curated, human-readable record — **not a commit log**. Each
> entry says *what changed and why it matters to a user*, in plain
> language, not *how* it was implemented. Skip internal refactors.
>
> **House style** for editors:
> - One short bullet per change. Multi-paragraph entries are only for
>   the major changes a user really needs to read in full.
> - No internal tracker IDs (`#NNN`, `PROJ-123`) unless that tracker
>   has a public link — they're noise otherwise. Mention the change,
>   not the ticket.
> - **Version bump = SemVer**: any `### Added` entry is at least
>   MINOR; `### Fixed` alone is PATCH; breaking change is MAJOR.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.5.0] - 2026-09-05

**The project is JobBox, the command is now `jbx`, and it is a different
bargain.** The old tool
queued what you told it to queue; you had to judge in advance which
commands were worth backgrounding, and that judgement is the thing
everybody gets wrong. `jbx` wraps every command and lets the long ones
detach themselves. Rewritten in Rust, so it runs on Windows too.

### Added

- **Every line is wrapped, and the long ones let go of themselves.** A
  command that finishes is handed back untouched — output as it is
  written, exit code unchanged. One that passes the threshold is
  detached, says so, and names the verbs that pick it up again.
- **`jbx stats`, which says how much time was compressed**, per project,
  and subtracts the time handed back to `jbx wait` rather than counting
  it as a win. It is a ceiling, not a receipt, and it says so.
- **`jbx queue <intent> -- <line>`** for work handed over before it
  starts — the old `jobbox run`. It is the only door with a cap, because
  it is the only work that has not started yet.
- **It composes with rtk instead of racing it.** `jbx init` unregisters
  rtk's hook and calls it directly, so both effects survive an ordering
  no harness documents. `--undo` puts it back exactly.
- **Windows.** No `task-spooler`, no `/proc` required, no shell traps —
  the supervisor is the same binary invoked again.
- **`jbx fg`, and the one judgement left to make.** The old build shipped
  a skill telling an agent to estimate which commands would be long —
  a judgement everybody gets wrong. jbx removes that question and asks a
  smaller one, once per session, in its own output: do you need this
  result before you can do anything else? When the answer is yes,
  `jbx fg -- '<line>'` runs without ever letting go, and `jbx stats`
  counts what it cost. `jbx fg <id>` picks a detached job back up —
  everything it has printed, then what it prints next, then its code.
- **Configuration files, written for you.** `jbx init` leaves a global
  `~/.config/jobbox/config.yaml` and a `.jbx.yaml` at the project's root,
  both fully commented so they change nothing — the settings become
  findable by reading rather than by asking. The project's file records
  what `rtk --version` actually answered when it was written, and
  `init --undo` leaves it alone: it may have been edited, and committed.
  Either file can then say anything the project wants differently —
  `enabled: false` and jbx stays out of the way there entirely. The
  project file wins key by key; an environment variable wins over both.
  `jbx config` says where every value came from and which file to edit.
- **A project is where `.claude` is**, then where `.git` is. A directory
  Claude Code works in is a project whether or not anybody ran
  `git init`, and looking only for a repository put every such directory
  in the same nameless heap.

### Changed

- **The whole tool is one command.** `jobbox`, `jobbox observe` and the
  skill are gone; `jbx hook` answers all four harness events, dispatching
  on the one the harness declares.
- **A project is the git repository it ran in**, found by walking up. The
  old build needed `JOBBOX_PROJECT` written into a settings file, because
  reading the working directory renamed a session's mailbox the moment a
  command ran from a subdirectory. A repository root does not move.
- **The measurement table records a command's SHAPE**, never the line as
  typed — `TOKEN=… ./deploy` becomes `./deploy`. It already did; it now
  also drops a leading `rtk`, so one command is not filed under two
  shapes depending on which door it came through.

### Removed

- **`task-spooler` is no longer the substrate**, along with the two
  defects the old code carried workarounds for: the segfault on a socket
  path over ~108 characters, and `TMPDIR` governing both the socket and
  the logs.
- **`jobbox timings`**, replaced by `jbx stats` — which measures from the
  supervisor rather than from a pair of hooks, so it misses nothing and
  knows the real duration and exit code of a detached job.

## [0.4.0] - 2026-09-04

First public release. The versions before it were bookkeeping inside a
single afternoon — none was tagged, none was installed by anybody, and a
changelog that adds a flag in one of them and removes it in the next is
not history, it is noise a reader has to parse.

### Added

- **Queue a long command and be told when it ends.** `jobbox run <intent>
  -- <command>` returns immediately; the ending reaches whoever needs it
  without anyone remembering to look.
- **A mandatory intent**, because a queue of `bash -c …` lines cannot be
  read back three hours later.
- **Liveness, not just "running".** jobbox reads the date of the last
  byte written to a job's log, so a job that has said nothing for ten
  minutes is named by `jobbox health` — without any script having to
  cooperate.
- **Claude Code in one command.** `jobbox init` merges its hooks into
  `.claude/settings.json` without removing anybody else's, and installs a
  skill describing when a queue is the right move. Any other CLI — or
  none — can read `jobbox signals <audience> --json`.
- **Several sessions on one queue, without stealing each other's
  notifications.** Sessions name themselves, so this needs no
  configuration; the human's copy stays shared because one person wants
  every ending. A project is identified by its directory, so two
  checkouts of the same name are two projects.
- **Ids jobbox mints and never reuses**, so a reference to a queue that
  no longer exists is refused rather than answered with somebody else's
  job.
- A failed job holds a session open only when **that session** queued
  it. Every ending is still announced to the person, whichever session
  ran it — but blocking is not announcing, and stopping an unrelated
  agent to demand a fix for somebody else's command is not help.
- **`jobbox config`**, which says every setting in effect and names the
  environment variables overriding anything.
- **`jobbox timings`**, which measures what shell commands actually cost.
  It was built to settle whether a hook should force long commands into
  the background by itself, and it settled it: no. See CONTRIBUTING.

### Notes

- One file, the standard library, and `task-spooler`. It runs under
  `python3 -S`.
- The queue does not outlive its `task-spooler` daemon. Acceptable for
  development work — worth knowing, not worth hiding.
