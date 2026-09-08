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

## [0.13.0] - 2026-09-08

### Added

- **`jbx hook gemini`** — Gemini CLI is wrapped too, and adding the next
  client is a row in a table rather than a branch in the code. The
  clients disagree on almost every word: Gemini calls the shell tool
  `run_shell_command`, names the event `BeforeTool`, and *merges* what a
  hook sends into the model's arguments where Claude *replaces* them
  outright. Those shapes were measured against each client rather than
  read off a page, are published by `jbx describe`, and a test holds
  them still so changing one without re-measuring fails a build instead
  of a session. A name nobody wired is refused rather than quietly
  treated as Claude — a hook speaking the wrong dialect answers nothing
  and looks perfectly healthy.

### Changed

- **`jbx stats` is now `jbx gain`, and the old name is gone.** One verb,
  not two doors onto the same number. There is no alias: `jbx stats`
  answers `unknown verb`.
- **`jbx gain` leads with the answer.** A headline block — commands
  wrapped, how many detached, what they took, what you stood through,
  what came back — sits above the table with a meter beside it, because
  the question you had when you typed the command was whether this is
  worth having, and that was previously answered under nine rows of
  detail. `detached` is second on purpose: a wrapper that detached
  nothing gave nothing back, and without that number a low percentage
  reads as a broken program rather than as a fact about your commands.
- **Each row carries an impact bar**, scaled against the tallest row
  rather than the total, so the shape of the distribution is visible
  instead of nine slivers.
- The readings file keeps its old name. Renaming it with the verb would
  have orphaned every measurement already taken.
- **`jbx init` now declares one hook, not four.** Only the hook that
  wraps a command is needed to detach one: an ending is delivered by
  `jbx wait <id>` run in the background by whatever runs your commands,
  which is what the detachment message already tells you to do. Three
  hooks in somebody else's settings file is a large footprint for a case
  the message covers.

  What the other three bought was the **unasked** announcement — a job
  nobody waited on finishing quietly rather than saying so. That is now
  a choice: `jbx init --announce` declares them. `jbx init --core` goes
  the other way and takes them back out of an install that has them.
  The plugin, which cannot take a flag, is the announcing install.

### Fixed

- **Colour no longer shifts a column.** The table measured a cell by its
  raw string, escapes included — and those are not a fixed size, so a
  dim row was padded one column further than a green one. It never
  showed while the coloured column was the last one, because a stagger
  needs something after it to push; the impact bar is that something.
  Widths are now measured as a reader sees them.

## [0.12.0] - 2026-09-08

### Added

- **`jbx after [seconds]`** — read the threshold and where it came from,
  or set it for this project. `jbx stats --thresholds` is the evidence
  for choosing one.
- **`/jbx:slots` and `/jbx:after`** in the plugin, beside `/jbx:jbx`.
  User-invoked only, like the façade: a model that may set its own
  threshold can set it to five minutes and stop detaching anything,
  which is the tool switching itself off to avoid its own discipline.

### Changed

- **`jbx slots <n>` writes the project's settings instead of a file of
  its own**, and `jbx after` does the same. Both land in `.jbx.yaml`
  where a project begins, in the global file otherwise, and say which
  they wrote. The line is edited rather than the document re-emitted, so
  the comments that make those files worth opening survive.
- **The plugin is named `jbx`**, so its skills are `/jbx:…`. The
  marketplace records the rename.

### Fixed

- **`jbx config` no longer lies about `slots`.** The cap had a third
  home — a file `jbx slots` wrote, invisible to the configuration — so
  after `jbx slots 3` the verb answered 3 while `jbx config`, whose
  entire job is to say every value AND where it came from, answered 6,
  `default`. Measured. A state carrying the same name as a setting is
  that setting copied, and the copy is what drifts; there is one place
  now.

## [0.11.1] - 2026-09-08

### Added

- **The plugin has a face: `/jobbox:jbx`.** Loaded, it declared nothing
  a person could see — no skill, no command — so a plugin that worked
  was indistinguishable from one that had failed to load. The skill is
  **user-invoked only**: a document the model may reach for, telling it
  when to background things, is exactly what this project refused and
  wrote a program instead of. It defers to `jbx help` rather than
  repeating anything, because a second copy is a second copy to keep in
  step.

### Fixed

- **The session discipline is said once per session, not once per
  declaration.** Installed as a plugin *and* declared by `jbx init`, the
  same four hooks run twice in two processes that know nothing of each
  other — measured on a real load, with the whole paragraph printed
  twice, which is how a paragraph becomes wallpaper. A marker created
  atomically settles it: whoever creates it speaks.
- **The discipline says the two things it means.** It buried the first —
  a judgement NOT to make, whether a line will be long — under the
  escape hatch for when you truly need a result now. A reader who is not
  told which judgement to skip makes it anyway, by backgrounding things
  by hand or sitting on a build in case.

## [0.11.0] - 2026-09-08

### Added

- **jbx installs as a Claude Code plugin.** `claude plugin marketplace
  add quazardous/jobbox` then `claude plugin install jobbox@jobbox`: the
  hooks, the binary and a background watch in one thing, with nothing to
  download and nothing to put on a `PATH`. The plugin declares `jbx
  watch` as a monitor, so endings arrive as notifications without
  anybody starting it.

  `bin/` has no platform convention — every executable in it lands on
  the `PATH` as named — so one `bin/jbx` chooses: the binary for this
  machine from the release archive, else an already installed jbx, else
  a message saying so and a non-zero exit. musl first on Linux, since
  the statically linked build runs where the other might not.

  **`jbx init` still matters** where rtk is installed: a plugin declares
  hooks and cannot displace somebody else's, and two hooks rewriting one
  field is a race no harness documents.

  **It does not help with Smart App Control.** The binary a plugin
  carries is the same unsigned binary, and that policy refuses unsigned
  executables whatever their origin — a plugin included. The signing
  question is unchanged.

## [0.10.2] - 2026-09-08

### Changed

- **Documentation only — the binaries are identical to 0.10.1.** Being
  told on the next turn was written up as *the* way an ending reaches
  you, with monitoring offered afterwards as what to do when there is
  nothing else to do. That inverts them: monitoring does not compensate
  for idleness, it removes the turn's delay for everyone. The two are
  ranked now, and the detachment message's word — MONITOR, not *wait* —
  is finally explained somewhere.

## [0.10.1] - 2026-09-07

### Fixed

- **A command the harness already put in the background is no longer
  detached underneath it.** The harness notifies its caller when that
  command EXITS — and a wrapper that lets go at thirty seconds exits
  then, so the notification fired early and reported work as finished
  that had barely started. That is the exact lie jbx exists to prevent,
  introduced by jbx. Measured rather than assumed: `tool_input` carries
  `run_in_background`, and carries it only when it is true.

  Such a command is now wrapped with a threshold of infinity rather than
  left unwrapped: the output still mirrors, the exit code still
  survives, the reading is still taken — and `waited` becomes the whole
  duration, which is the truth, since jbx saved nothing there. It is not
  counted as a deliberate foreground either: `jbx fg` counts a decision,
  and this is not one.

## [0.10.0] - 2026-09-07

### Added

- **`jbx watch`** — one line per job event, until nothing is running.
  For whatever watches a stream: it emits on every terminal state,
  including the failures, because a watch that speaks only on success is
  silent through a crash and silence looks exactly like "still running".
  It **ends by itself** when nothing is left running, and it observes
  without consuming — unlike `jbx signals`, which destroys what it
  reports and would eat the endings the session is waiting for.
  `--json` streams one object per line rather than an array, since an
  array is only valid once closed.

### Changed

- **The detachment message is three lines instead of thirteen.** It was
  an argument, printed on every single detachment, with the two things
  that matter — the id, and the one command to run — at the bottom of
  it. An argument is read once and skimmed after that. What stays is the
  order, because it is the habit the tool exists to break, and the
  instruction:

  ```
  jbx: this passed 30s, so it is in the BACKGROUND as j7f3a91c — nothing lost.
  DO NOT WAIT FOR IT, DO SOMETHING ELSE. With nothing else: Monitor
  `jbx wait j7f3a91c`, which ends when the job does.

    jbx help j7f3a91c
  ```

- **`jbx help` replaces `jbx how`, and is the one way in.** `jbx help`
  lists every verb, `jbx help <id>` says what to do with one job, and
  both name `jbx why` for the reasoning — which is where the argument
  the message used to carry now lives, read once by somebody who wanted
  it rather than skimmed ten times a session by somebody who did not.
- **Waiting is named for what it costs, in `jbx why`.** Polling is
  waiting with extra steps; a background `jbx wait` is neither, because
  it ends when the job does and its ending wakes you.

## [0.9.1] - 2026-09-07

### Fixed

- **The three windows say how many jobs detached, not only how many
  calls there were.** Almost every call finishes long before the cut and
  was never a candidate to save anything, so `calls` alone invites the
  reader to divide one number by the other and conclude the tool is
  lying — a hundred and ninety calls saving four minutes reads as broken
  until you see that seven of them ever detached. The arithmetic itself
  was checked against an independent recomputation of the raw table and
  agrees to the call and to the percent.
- **Those columns were ragged where the format said they were not.** A
  `serde_json::Value` renders itself and ignores the width it is given.

## [0.9.0] - 2026-09-07

### Added

- **`jbx stats` says the same thing at three distances** — last hour,
  last day, and everything kept. "Am I saving time" and "am I saving
  time TODAY" are different questions, and one figure covering weeks
  answers the first while looking like an answer to the second.
- **`jbx stats --since 1h|24h|7d|all`** narrows the table to a window.
  Every reading has carried the instant it was taken since the first
  one; nothing had ever read it back.
- **Colour, where somebody is looking.** The `saved` column and the
  three summary lines are coloured by what the number is worth, on one
  scale so the eye learns it once. `auto` means when standard output is
  a terminal and `NO_COLOR` is unset — so an agent reading a pipe gets
  exactly what it got before, and `--json` is never painted at all.
  `color: always | never` settles it, and `never` is there for the
  console that shows escapes rather than obeying them.

## [0.8.0] - 2026-09-07

### Added

- **The install offers to declare the hooks, and asks first.** Installing
  jbx without them leaves a binary that does nothing — the whole tool is
  the hook — so both installers end with `Declare the hooks now? [Y/n]`
  and run it for you. `init` edits a settings file other tools share, so
  it is a question and not an assumption; with no terminal to ask at
  (a Dockerfile, CI) it prints the line instead.
- **`jbx init --global-only`** — the hooks and the global file, and no
  project file. An installer runs from wherever somebody happened to be
  standing, which may well be inside a repository, and a `.jbx.yaml`
  appearing in your project because you installed a tool is a surprise.

## [0.7.1] - 2026-09-07

### Fixed

- **On Windows, `jbx init` declared a hook the shell could not run.** The
  harness does not exec a hook's command; it hands the line to `bash`,
  where a backslash is an escape and not a separator. So a declaration
  written as `C:\Users\...\jbx.exe hook` arrived as
  `C:UsersAppDataLocaljbxbinjbx.exe: command not found` — on every
  prompt, in every session, from an install that had just reported
  success, and with nothing anywhere saying which of the two was wrong.
  The path is now spelled with forward slashes, which Windows accepts
  everywhere it accepts a backslash and `bash` leaves alone. Running
  `jbx init` again repairs a declaration already written the old way.

## [0.7.0] - 2026-09-07

### Added

- **The Windows installer puts jbx on your PATH instead of telling you
  how.** It writes the user scope, and the shell it is running in, so
  `jbx init` on the next line works without opening a new window.
  `-Uninstall` takes the entry back out along with the binary.

### Fixed

- **The PATH advice it used to print would have damaged your PATH.**
  `setx PATH "$env:PATH;..."` reads the machine and user PATH joined
  together and writes the result back to the user scope alone, so
  following it copies every machine entry into your own and leaves it
  there — and `setx` truncates at 1024 characters besides. Nothing reads
  `$env:PATH` to decide what to write any more.
- **The installer stopped to ask permission halfway through.** Windows
  PowerShell parses a fetched page with the Internet Explorer engine
  unless told not to, so downloading `SHA256SUMS` raised "the script may
  run when the page is parsed" — a security prompt, mid-install, whose
  default answer is No. Both fetches ask for basic parsing now.

### Changed

- **Four test payloads that were in French are in English.** This is a
  public repository; the apostrophe test keeps its apostrophes, which was
  the only thing it ever asserted.

## [0.6.0] - 2026-09-07

### Added

- **`install.ps1 -TrustLocally`, for a Windows that refuses to run jbx at
  all.** Smart App Control blocks unsigned executables whatever their
  origin — a published release as flatly as a local build — and offers no
  exception for one file. This flag makes a code-signing certificate,
  asks Windows to trust it for your user, and signs the installed binary
  with it. Measured on such a machine: before the root is trusted the
  signature reads `UnknownError`, "a certificate chain ended in a root
  which is not trusted"; after, `Valid`, and jbx starts. The local trust
  store counts, which is the opposite of what we had assumed and written
  down. It is opt-in and stays opt-in: the certificate signs code and
  nothing else, it is named so you can find it, Windows asks before the
  root goes in, and `-Uninstall` takes it back out.

### Fixed

- **`install.ps1` could not be run from a checkout on Windows.** The file
  held a handful of em-dashes, and `powershell` — 5.1, the one every
  Windows has — reads a `.ps1` as ANSI when it carries no byte-order
  mark. The mangled bytes broke the parse: ten errors, none of them
  anywhere near the real cause, on a path the README tells people to use.
  Only `pwsh` 7 and the `irm | iex` form ever worked. The script is plain
  ASCII now, which fixes it everywhere rather than for one shell.

## [0.5.13] - 2026-09-07

### Fixed

- **`install.ps1` never actually checked what it downloaded.** It fetched
  `SHA256SUMS`, failed to find the line naming the file it had just
  pulled, and reported "no sum published for this file — NOT verified"
  while the sum had been published all along. Every Windows install this
  script has ever done was therefore unverified, in words that read like
  the release's omission rather than the installer's. GitHub serves a
  release asset as `application/octet-stream`, which PowerShell hands
  back as bytes and not as text, so splitting it into lines split the
  bytes: the first three came out `49 | 101 | 50`, the decimal codes of
  `1e2`.
- **`install.ps1` ended in a stack trace on a machine that will not run
  the binary.** The last thing it does is ask jbx its version, and Smart
  App Control refuses unsigned executables whatever their origin — a
  published release included. The install itself had worked; what
  followed was a PowerShell error about output encoding that named
  nothing. It now says what is actually in the way.

## [0.5.12] - 2026-09-07

### Added

- **A job is named by whoever ran it.** The harness already asks for a
  one-line description of every command and hands it to the hook, so the
  hook passes it on: `jbx ps` shows "replay the DAG simulation" where it
  showed four words off the front of the line. `--intent` sets it by hand
  for anyone calling `jbx run` directly.

### Added

- **`--json` on every verb that reads.** `status`, `health`, `clients`,
  `config`, `slots`, `stats`, `how` and `why` answer as JSON when asked;
  before, three verbs did and the rest ignored the flag in silence. A
  verb now builds a value and one place decides whether it is printed as
  JSON or rendered for a person, so the two cannot come to disagree.
- **`jbx stats --thresholds`** — what another `after` would have cost,
  replayed on the lines already measured. Every reading holds the
  duration the line really took, so this is a counterfactual rather than
  a prediction: it says what already happened would have cost. It also
  shows the distribution (median, p90, p99) and, per candidate, how many
  jobs would have detached *and then finished within ten seconds* —
  which is the price of lowering the cut, and the one thing `saved`
  cannot see. Its columns are named as the conditionals they are —
  `would detach`, `would wait`, `would save` — because `saved` next door
  is a measured fact and one word for both meanings is how a reader
  comes to trust a number that was never true. The cut actually in force
  is marked.
- **Every verb refuses a flag it does not take**, and answers `--help`.
  What a verb accepts is declared once, in the same table `jbx describe`
  publishes, so a flag cannot be accepted without being documented nor
  documented without being accepted. `jbx describe` now carries each
  verb's `options`.
- **`--width <n>` on `jbx ps` and `jbx list`**, and a `width` setting to
  go with it. A listing asks the terminal how wide it is and uses it; the
  fallback of 100 columns is for the reader who has no terminal, which is
  usually the agent.
- **An `age` column in `jbx ps` and `jbx list`** — how long ago each job
  started. `finished exit 0` read the same for something that ended a
  minute ago and something that ended yesterday.
- **`jbx list --help` and `jbx ps --help` answer.** They used to print a
  list of jobs and say nothing about the flag, which is a typo that looks
  like it worked. Any flag those verbs do not know is now an error.

### Changed

- **`jbx ps` and `jbx list` show the intent AND the line.** They answer
  different questions — what somebody meant to do, and what is actually
  running — and a table showing one of them left the other to guesswork.
  The line is cut to fit and loses the `cd` the harness writes in front
  of every command, which had four columns of identical path standing
  where the difference between two jobs should be; `--full` prints it as
  recorded and `--json` drops nothing. The state column keeps one word,
  and `jbx status` keeps the sentence that used to be in the table.
- **A name read off the line is derived when it is read, not when it was
  written.** It was computed once and stored, so a store held every past
  version of the rule at once and old jobs kept naming `cd /home/…` for
  ever. What a caller SAID is still recorded — that is data; what was
  read off the line is read again, and improves with the rule.
- **The compact line drops the wrappers it arrived in**, not just the
  leading `cd`: `timeout <n>` and `rtk proxy` go too. Measured on a real
  store, twenty of fifty records began with all three — forty characters
  of identical preamble standing where the difference between two jobs
  should be. `rtk proxy` goes and a bare `rtk` stays, since `rtk gain` is
  a verb of rtk's own.

### Fixed

- **On Windows, `jbx queue` never gave the shell back at all.** The fix
  that stopped `jbx run` holding the caller's pipe was never applied to
  its twin, so a queued job still handed the supervisor our standard
  output. Measured on the same twenty-five second line: `queue` returned
  in one second with its output discarded and twenty-five with it
  captured — the whole job, from a verb whose entire purpose is to hand
  work over before it starts. It also made the queue look broken: with
  one slot, three queued jobs could never be seen to overlap, because
  the caller was stuck rather than the queue wrong.
- **On Windows, a detached job kept the caller waiting anyway.** The
  launcher announced that it had let go — and the harness reading its
  output stayed blocked until the job finished, because Windows hands a
  child every inheritable handle and not only the three it is given, so
  the supervisor was holding the caller's pipe. Measured on a runner:
  the same call returned in one second with its output discarded and six
  with it captured. A wrapper that says it gave the shell back and did
  not is worse than one that never claimed to.
- **On Windows, a line's output never arrived.** The log was opened
  append-only, which there grants a handle Git Bash cannot use: every
  write from the line failed, the shell exited 1, and the error saying
  so went to the same dead handle — an empty log and no explanation.
  Twenty of the suite's tests failed on it.
- **Nested projects never nested on Windows.** `jbx stats` compared
  paths with a hand-written `/`, so no row ever contained another and
  the table read as flat — and entirely plausible.
- **A listing drew one character past the width it was given.** The
  space after the intent column was printed by the column and counted by
  nobody, so a full-screen terminal wrapped every row of the table. A
  test now measures each row against the width it asked for.
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
