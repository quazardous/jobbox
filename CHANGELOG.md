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

## [0.5.12] - 2026-09-06

### Added

- **A job is named by whoever ran it.** The harness already asks for a
  one-line description of every command and hands it to the hook, so the
  hook passes it on: `jbx ps` shows "replay the DAG simulation" where it
  showed four words off the front of the line. `--intent` sets it by hand
  for anyone calling `jbx run` directly.

### Fixed

- **The shell that was checked is the shell that is run.** The check
  skipped `C:\Windows\System32\bash.exe` — the WSL launcher, which is not
  a shell — and then spawned `bash` by bare name, letting `Command` do
  its own PATH lookup and find it again two lines later. Verifying one
  thing and using another is its own kind of bug. Git for Windows is also
  looked for where it installs, since it is often present and often not
  on the PATH.
- **`jbx kill` reports what happened, not what was attempted.** `kill`
  answering successfully does not mean the process went; on one runner
  the group form failed and a job stayed readable as waiting for a slot
  it had been stopped from ever taking. The signal is now sent to the
  tree AND to the supervisor by name, the outcome is checked, and `KILL`
  follows if `TERM` did not settle it.

## [0.5.11] - 2026-09-06

### Added

- **`jbx describe`** — every verb and what it does to the world, as JSON
  (#2067). A tool that checks an agent's commands before they run needs
  to tell `jbx list` from `jbx kill`, and reading `--help` for the names
  is where it goes wrong: they MOVE, and a hardcoded table then
  classifies an unknown verb at random inside a program whose job is
  deciding what to let through. The binary answers for itself, so
  whatever is installed is right by construction.

  The document follows [OpenCLI](https://opencli.org/) so the tools that
  read it can — but a CLI schema says a verb `kill` exists and takes an
  id, never that it tears down a process tree. OpenAPI escapes that
  because HTTP carries the answer in the method; a command line has no
  such thing. So the consequence is ours to add — as `x-jbx-tags`, simple
  verbs a guard can COMPARE (`read`, `consume`, `execute`, `create`,
  `destroy`, `capacity`, `configure`, `rewrite`, `block`), with the
  sentence kept beside them for a person. A prose effect would have to be
  matched as text by whoever read it, which is guessing, in a program
  whose job is not guessing. The vocabulary travels in the document, so a
  reader needs to know nothing in advance, and a test refuses a tag the
  document does not define. This is the only part worth relying on: the
  specification is pre-1.0 and several projects publish one, which the
  document says about itself rather than letting `"opencli"` read as a
  promise. `signals` is marked as CONSUMING, because a guard must not
  mistake a destructive read for a look.
- **`--full` and `--json` on `ps` and `list`.** The `line` column has
  always shown the intent — four words — which is what makes a list
  readable and what makes two jobs beginning the same way
  indistinguishable.
- **The name of a job ignores a leading `cd`.** The harness writes one in
  front of every command, so a whole list read `cd /home/…/bms && e…` and
  named nothing. The shape calculation already dropped it; the name did
  not — one lesson applied in one place out of two.

### Fixed

- **The queue has a real order.** Waiting was "whoever asks when a slot
  frees", which followed the filing order only because waiters happen to
  start asking in that order — in practice, never a guarantee: a waiter
  whose poll lands just after a slot frees loses to one that polls just
  before, however long it has been there. Each waiter now takes a
  numbered ticket, allocated by exclusive file creation because that is
  the one atomic primitive both platforms share, and only the head of the
  line may take a slot. A dead holder's ticket is reclaimed on every turn
  of the wait — a queue with an order can otherwise do worse than one
  without, and that is the way.
- **A zombie is no longer alive.** Liveness was "does `/proc/<pid>`
  exist", and a reaped-by-nobody process still has one. On an ordinary
  machine an orphan is reparented to init and reaped at once, so this
  never showed; inside a container whose pid 1 reaps nothing, a stopped
  job read `queued` or `background` for ever. Found by running the suite
  on CI runners, which this machine could not have shown.
- **`bash` is not taken from `System32` on Windows.** What is there is
  the WSL launcher, and on a machine with no distribution installed it
  answers every command with "Windows Subsystem for Linux has no
  installed distributions" — in UTF-16, which is how it was finally
  recognised. Found the first time the suite ran on a real Windows
  runner; no amount of reading would have shown it.
- **Liveness is asked once for everybody.** It is a `stat` on Linux and a
  PROCESS on Windows, and the queue asked about every outstanding ticket
  on every turn of a 200 ms wait: ten waiters meant ten `tasklist`
  launches five times a second.
- **A job record carries its project**, so `jbx ps --all` groups by the
  Claude Code that ran it rather than by whatever directory the launcher
  stood in. Records written before this reconstruct it from that
  directory, which is what it used to mean.

## [0.5.10] - 2026-09-06

### Added

- **`jbx ps`** — what is happening right now. "What is going on" is asked
  far more often than "what went on today", and a day of finished jobs
  between you and the answer is a list you stop reading. `jbx list` still
  shows everything kept.
- **`jbx ps` and `jbx list` show this project by default**, with `--all`
  for the machine. The store is machine-wide, and a list holding four
  projects' work is a list where you cannot find your own. The scope is
  the PROJECT and not the session: two Claude Codes open on one directory
  are working on the same thing, and scoping by session would blind each
  to half of it. What is hidden and still running is COUNTED, because
  hiding other work makes a busy machine look idle — and `--all` adds a
  project column, since otherwise it mixes projects without saying so.
- **`jbx queue` says out loud when a job does not start.** A verb that
  answers with an id and nothing else lets somebody believe the work has
  begun; a job held back by a full queue looks exactly like one already
  running, until they go and look. The id is still the first line, alone,
  because that is what a script reads.
- **A job says whether the launcher still holds it.** `running` covered
  two situations that want different things done about them: still held,
  where output is mirroring to whoever asked and the line may yet finish
  in time and leave nothing behind; and let go of, where only the log
  receives anything. They read `foreground` and `background` now — and a
  record written before this existed reads `running`, because it has not
  told us and guessing would assert what nobody observed.

### Fixed

- **Stopping a job before it starts leaves a state that says so.** The
  "waiting for a slot" branch answered before the liveness check, so a
  queued job whose supervisor had been stopped read as waiting for ever —
  and `jbx wait` on it blocked for ever with it. The queue moved on
  without it, so the cancel had worked; only the record disagreed.

### Changed

- **`jbx init` declares the link it was called through.** `current_exe()`
  follows symlinks, so a development install nailed the hooks to the
  build tree: right in that a rebuild is picked up with no re-init, wrong
  in that moving or deleting the tree broke every session at once —
  measured the hard way. Declared through the link, both hold: a rebuild
  still follows, and reinstalling or switching to a release binary moves
  the hooks with it, because the address stays and only its target
  changes.
- **Re-running `init` brings an existing declaration up to date** instead
  of noticing one. It used to answer "already declared" and leave the
  harness pointing wherever it pointed before, which reads like nothing
  to do and was not.

## [0.5.9] - 2026-09-06

### Fixed

- **A reading belongs to the Claude Code that ran it, not to wherever the
  command happened to be.** Readings were filed by the launcher's working
  directory, and a session's working directory moves — one `cd` moves it
  for every command after. Measured on a real store: a session began at a
  repository root, stepped into a sub-project, and its first row froze at
  that minute while a second row started filling. A tree worked in for
  hours from elsewhere never appeared at all.

  The hook writes down the calling session's own directory the first time
  it sees it — only a hook is told that, `CLAUDE_PROJECT_DIR` is not
  given to commands — and everything else looks it up by session id.
  Written once and never updated, because a later `cd` must not move it:
  that is the whole point of preferring it to the working directory.

  A plain shell has no session and no hook to have written one down; it
  still walks up from where it stands, which is what a person in a
  terminal means anyway.

## [0.5.8] - 2026-09-06

Documentation only — but the README travels inside every release
archive, so a wrong one ships with the binary.

### Fixed

- **The stats example showed a parent totalling its children**, which no
  version has ever done: each row counts its own calls, and a parent that
  summed its subtree would say the same minute twice in the total
  underneath. The figures add up now, and the text says which way it
  works.
- **`jbx hook` was missing from the list of verbs** — the one `init`
  writes into a settings file people then read. The guard could not have
  caught it: it checked that the README named nothing false, never that
  it named everything. It checks both directions now.
- **`JBX_SHELL` and `JBX_CONFIG`** are in the settings table.

## [0.5.7] - 2026-09-05

### Added

- **One line to install it, from a release, without a checkout.**
  `curl -fsSL …/install.sh | sh`, or `irm …/install.ps1 | iex` on
  Windows. Building needed the repository AND a Rust toolchain, which is
  the wrong order for a tool whose whole point is that you do not have to
  build anything; `--from-source` still does it for whoever wants to.
- **The release publishes `SHA256SUMS`, and the installer checks them.**
  TLS says the bytes came from GitHub; it does not say they are the bytes
  that release built. A release without sums is said out loud rather than
  passed over — every release before this one has none, and the installer
  reports that instead of implying a check it did not make.
- **`JBX_BIN`** moves where the installer puts the binary, so trying it
  never overwrites an install somebody is using.

## [0.5.6] - 2026-09-05

### Fixed

- **A wrapped line that runs `jbx` makes one job, not two** (#2066). The
  hook wraps every command; when the command it wrapped was itself a
  `jbx run`, there were two — and the id announced was the OUTER one. It
  ends in seconds with `exit 0` and a log holding nothing but the inner's
  detachment message, which reads exactly like a finished job while the
  real one runs on under an id nobody was told.

  It cost four wrong ids in one session: a suite declared finished, a
  `kill` aimed at the wrong thing, and a `wait` that returned at once so
  the next command started underneath the work. It needs no pipe to
  happen, and it explains the first report on that ticket as well.

  An inner `jbx run` now steps aside — the outer wrapper already gives
  the line a log, an exit code, a detachment and an announcement, so a
  second one adds nothing and costs the truth about which id to trust.
  Exactly what it already does for a terminal.
- **A job's measurement is on disk before the job reads as finished.** The
  exit code was written first, and the launcher returns the moment it
  sees that file — so a caller could read back a job it had just watched
  finish and find no reading for it. The code file is written last now:
  it is the commit point, and the order is the contract.

### Changed

- **`blocked` is called `waited`.** It is what you actually stood still
  for, and the plain word reads against `saved` without a glossary. The
  footer now says what each column is: `waited` is the standing, `saved`
  is the rest of `elapsed`.

### Added

- **A reader that leaves early is told its view was partial** (#2066).
  What jbx prints is a MIRROR of the job's log, not the job:
  `jbx run … | head -3` truncates what you see and never what runs — and
  the truncated mirror reads exactly like the whole story. Somebody
  concluded a suite had finished, re-ran it, and the two collided over
  the same Docker project.

  The condition is that a write FAILED, not that stdout is a pipe: an
  ordinary `x=$(jbx run …)` reads to the end and misses nothing, and
  warning there would be noise. And because `… 2>&1 | head` leaves no
  channel open at all — both streams are the closed pipe — the fact is
  also written on the job, so `jbx status` says it when nothing else
  could.

## [0.5.5] - 2026-09-05

### Fixed

- **Two projects that share a directory name are no longer one project.**
  `jbx stats` grouped by name, so every `api` on the machine became a
  single row whose every number was the sum of unrelated things. It now
  groups by path, and spends four characters of a hash on a label only
  where one is genuinely ambiguous.
- **A session working across projects no longer strands its endings.** The
  mailbox was addressed by project, derived from the working directory —
  but an ending is deposited by a supervisor that inherited the COMMAND's
  directory, and read by a hook running in the SESSION's. The moment a
  session touched a second repository the two disagreed. Measured on a
  real store: one session held two mailboxes, and two endings sat in the
  one it had stopped reading.

  The address is the session now, which both ends can read whatever
  directory they are in. The project was never an address — it is a label
  on the work, and every job record still carries it.

### Changed

- **The number is called `saved` again.** "Compressed" was chosen to stop
  the column over-claiming, and it bought that at the price of needing
  the footer to be read before the table meant anything. The caveat has
  not moved — `saved` still subtracts the time given back to `jbx wait`,
  and the line under the table still says it is a ceiling and not a
  receipt.

### Added

- **Projects nest in `jbx stats`.** A repository inside a repository is
  the ordinary case, and a flat list hid it exactly where it mattered:
  three of the four rows on this machine live inside the fourth. A child
  is shown under its parent, named by what it is rather than by the whole
  road to it. `--project-path` prints the roads.

## [0.5.4] - 2026-09-05

### Changed

- **The detachment message instructs instead of narrating.** It said what
  had happened and left the reader to infer what to do, and the one fact
  that makes waiting pointless was missing from it: that the ending is
  ANNOUNCED, on a later turn, without anybody remembering to look. It now
  leads with "this is now in the BACKGROUND" and says outright not to
  wait, and why, in the same breath.

### Added

- **`jbx how [id]` and `jbx why`** — the two halves of the explanation,
  travelling with the binary, because a downloaded one has no repository
  to read. `how` is what to type, and given an id it answers about that
  job so the lines can be copied as they are. `why` is why it works this
  way: what was measured to refuse prediction, why letting go costs
  nothing (the ending is announced), why asking for the foreground is a
  first-class gesture, and why the compressed time is a ceiling.
- The detachment message no longer carries the list of verbs. It was four
  lines longer than the one thing it exists to say — do not wait — and
  the list is one word away in `jbx how <id>`.

## [0.5.3] - 2026-09-05

### Fixed

- **The two halves of the tool no longer assume different shells.** The
  hook rewrites a command into `jbx run -- '<line>'`, quoted for a POSIX
  shell — correct, since the harness that hands us the line drives one,
  Git Bash included on Windows. The runner then handed that line to
  `cmd /C` there, which understands neither the syntax nor the quotes.
  Found by reading, not by running: nobody has run jbx on Windows. It now
  uses `bash` wherever `bash` exists, on every platform, and `shell:`
  settles it for a setup we guessed wrong.

### Added

- **`jbx config` says which shell will run a line**, and the README says
  what is known about Windows and what is not.

## [0.5.2] - 2026-09-05

### Fixed

- **A running command is no longer declared stuck because it is reading
  its input.** `jbx` announced "WAITING FOR INPUT […] it will not finish
  on its own" whenever anything in the subtree was stopped in a read of
  file descriptor 0 — which is what every pipeline stage waiting on a
  slow producer looks like, and every `docker` client relaying a
  terminal. It said it about a deployment that had already succeeded, and
  advised killing or re-running it: one loses the result, the other does
  it twice. `sleep 5 | cat` reproduced it in one line.

  What is left is an observation with its duration, offered only after
  ten seconds of silence, and it says outright that an ordinary pipeline
  looks the same. Nothing predicts any more. The measurement that missed
  this was a witness chosen to agree — a `cat` on a pty with nobody
  writing — which confirmed that a stuck process reads, and never asked
  whether a reading process is stuck.
- **A job is no longer called "gone, no exit code" because of a race.**
  That state is the one a caller acts on, and it is also the one a
  moment's timing invents: a supervisor between its last write and its
  exit is briefly neither running nor recorded. The absence must now
  persist before it is believed. And a supervisor that cannot write the
  exit code says so in the job's log rather than leaving a silence that
  reads exactly like a kill.

## [0.5.1] - 2026-09-05

Everything 0.5.0 promised, plus a way to install it — and the macOS bug
that would have made the first Mac binary lie.

### Added

- **A binary you can just download.** Every tag builds one for Linux
  (statically linked, so the distribution that built it does not matter),
  Windows and macOS, and publishes it with the notes taken from this
  file, which is the source. `cargo install` needs a Rust toolchain, and
  on Windows that means the Visual Studio build tools — gigabytes of
  prerequisite for a program under a megabyte. A platform whose users
  cannot install it is a platform supported on paper.
- **`install.ps1`**, the Windows counterpart of `install.sh`, for anyone
  who does want to build.

### Fixed

- **A finished job no longer reads as killed on macOS and the BSDs.**
  Liveness was read from `/proc` under `cfg(unix)`; where there is no
  `/proc` that is not a missing answer but a WRONG one — every job that
  had ended cleanly showed as "gone, no exit code", which is what a
  killed one looks like. Those platforms are asked with `ps` now. Found
  while preparing the first Mac binary, which would have shipped it.

### Changed

- Two guards for what drifts while nobody looks: the version in
  `Cargo.toml` against this file's newest entry, and every verb the
  README names against what the binary actually answers to.

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
