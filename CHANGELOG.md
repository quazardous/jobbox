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
