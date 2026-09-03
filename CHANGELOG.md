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

## [0.4.0] - 2026-09-03

### Added

- `jobbox timings` reports what actually takes time in a session, ranked
  by total time spent waiting rather than by the slowest single call —
  the forty-second command run thirty times costs more than the one
  ten-minute build, and only the ranking shows it. `jobbox init` wires
  the pair of hooks that collect it.
- The measurement stores only a command's shape, never the line as
  typed: a command can carry a secret inline, and the table lives in a
  cache directory for weeks.
- `jobbox --version` says which copy you have — an installed command that
  cannot answer that turns every report into a conversation.
- **jobbox no longer depends on `click`.** One file, Python 3.11+, and
  `task-spooler` — nothing to install first, and it runs with
  site-packages switched off. The command-line behaviour is unchanged.
- The measurement answered the question it was built for: a hook that
  sends long commands to the background by itself **would not work
  here**, so it was not built. Four of five long command shapes had been
  seen exactly once, and no rule can catch what it has never seen.
  `jobbox timings --detail` remains, and would answer the question again
  if the work changed shape.
- `jobbox init` now recognises its own declarations by verb rather than
  by their exact text, so a declaration that gains a flag replaces the
  old one instead of piling up beside it.
- Empty mailboxes left by finished sessions are now forgotten
  automatically, from `jobbox clients` and behind every job that ends —
  the `--prune` flag is gone. One holding an ending is never removed, at
  any age.
- `jobbox timings --detail` reads the table for you: bands, what a guard
  at each threshold would buy against how often it would interrupt, and
  the per-session split — plus a warning when one call carries most of
  the total, or when it all comes from one session. `--session` narrows
  it to one.
- `jobbox init` installs a skill describing **when** to use a background
  queue at all — the judgement that does not fit in `--help`, and that
  used to live in whichever project happened to host the tool. An edited
  copy is never overwritten without `--force`.
- `jobbox timings` says how many sessions its table covers, and each row
  records which one made the call. The table is machine-wide, so a
  reading drawn from one session is not the same claim as one drawn from
  several — and nothing in it could tell them apart.

## [0.3.0] - 2026-09-03

### Added

- Sessions now name themselves, so two of them stop taking each other's
  notifications without anyone configuring anything. `JOBBOX_CLIENT` is
  still there to pin one fixed name — for a CI runner or a shared worker.
- `jobbox slots` reads and sets how many jobs may run at once, and
  `jobbox health` now says how wide the queue is and how many jobs are
  waiting. One slot is the default, so jobs run strictly one after
  another; that is the usual answer to "why has my job not started".
- `jobbox clients --prune` forgets the empty mailboxes of sessions that
  have closed. It never touches one that still holds an ending.
- `jobbox health` now reports endings waiting in mailboxes other than
  yours, and `jobbox signals --client <name>` reads one. Who a client is
  comes from the harness, so a job filed under a name nobody comes back
  to is possible — this makes it visible instead of silent.

### Changed

- The human's mailbox is no longer split per session. One person wants
  every ending, whichever session started it — splitting it would have
  lost the endings of jobs whose session closed first.

## [0.2.0] - 2026-09-03

### Added

- Several sessions can now use jobbox at the same time without stealing
  each other's notifications. Name a session with `JOBBOX_CLIENT` and it
  gets its own mailbox; the queue itself stays shared, so ordering and
  parallelism still protect the machine.
- `jobbox clients` lists every mailbox and what is still waiting in it,
  so endings left behind by a session that closed are visible instead of
  silently piling up.
- `install.sh` installs jobbox into `~/.local` — no root, reentrant, with
  `--symlink` for development and `--uninstall` to remove it.
- `jobbox init` wires jobbox into the project in the current directory by
  merging its hooks into `.claude/settings.json`. It never overwrites
  hooks that belong to other tools, and running it again is safe.
- `jobbox claude-hook` shapes pending endings for Claude Code, so an
  integrator no longer has to write that bridge themselves.
- `JOBBOX_SOCKET` selects the queue to talk to, which is what makes a
  second, isolated instance possible — and what lets the test suite
  exercise the real notification chain without touching a live queue.
- `jobbox list --mine` narrows a shared queue to the current client.

### Changed

- **Breaking:** the JSON returned by `jobbox signals --json` now uses
  English field names — `log`, `intent`, `command`, `finished_at` — and
  carries the new `client` field. Integrators reading the previous French
  names must update.
- **Breaking:** the muteness threshold is now `JOBBOX_MUTE_AFTER`
  (previously `JOBBOX_MUET_DEPUIS`).
- `jobbox health` now says when the daemon was started *by the check
  itself*. Asking `tsp` whether it is alive is what brings it back, so
  the old answer was always "alive" — including right after a queue had
  died with its daemon.
- Everything the project ships — code, comments, documentation — is now
  in English.

### Fixed

- A job ending that arrived while `signals` was reading could be erased
  before anyone saw it. The mailbox is now claimed atomically first, so a
  concurrent ending lands in a fresh file instead.
- An unparseable `JOBBOX_MUTE_AFTER` no longer makes every command,
  `--help` included, fail with a traceback.
- An over-long socket path is now refused with an explanation instead of
  segfaulting `tsp`.

## [0.1.0] - 2026-09-03

### Added

- Initial release: queue a long command with a readable intent, list what
  is waiting, running and finished, follow a log, kill a job, and be told
  when something ends.
