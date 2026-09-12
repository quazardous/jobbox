# When it does not do what you expected

Most reports are one of the first three, and all three are jbx working
as designed rather than failing.

## Nothing is being detached

**Are you at a terminal?** Then it will not detach, on purpose. A TTY
means a human is watching: something might need to prompt, the output
needs to stay on screen, and nobody is being kept waiting by a turn they
cannot spend elsewhere. jbx runs the line and gets out of the way.

To see the detaching, run it the way an agent does — through a pipe:

```console
$ jbx run --after 3 -- 'sleep 20; echo done' < /dev/null | cat
```

**Is `JBX_WRAPPED` in your environment?** Then something outside already
holds this job, and the inner jbx stands aside rather than claiming a
second id for the same work. That is the fix for #2066, where a suite was
declared finished, a `kill` hit the wrong process, and a `wait` returned
at once. To measure the outer behaviour deliberately:

```console
$ env -u JBX_WRAPPED jbx run --after 3 -- 'sleep 20' < /dev/null
```

**Is the hook actually declared, and for the right client?** `jbx config`
says what is in force and where each setting came from. A hook declared
as a bare `jbx hook` answers as Claude — under Gemini or Copilot it
matches nothing and stays silent, which looks exactly like being
correctly installed. `jbx init --cli <id>` names the client; `jbx hook
--list` says which ones exist.

**Was the command simply short?** The cut is thirty seconds by default.
`jbx after` reads it, `jbx after 10` sets it for this project.

## The output stopped mid-way

The job was detached and kept going; what you saw is what had been
written by then. `jbx tail <id>` has the rest, `jbx tail <id> -f`
follows it, `jbx status <id>` says whether it ended and with what code.

If a message said your view was partial, it was: something reading the
output closed the pipe early — `| head` is the usual one — and the
command itself ran to the end regardless.

## `jbx ps` shows a shorter line than I ran

The column is elided from the **front** when it does not fit, so
`sleep 60; echo built` reads as `echo built`. `jbx ps --full` shows the
whole line. The intent column beside it elides from the end, which is
why the two look inconsistent.

## A job is `NO OUTPUT`, `MUTE`, or `stranded`

`NO OUTPUT` means the job has not written a single byte to its log since
it started. The work may be going perfectly well. A filter that prints
only at the end of its input — `| tail`, `| sort` — keeps the log empty
for as long as the job runs, and so does output a program keeps in a
buffer (`stdbuf -oL`, `PYTHONUNBUFFERED=1`). A job that never ends never
reaches that end: `… | tail -3` on an endless worker stays empty for
ever. `jbx health` shows the end of each such line, so the pipe can be
seen.

`MUTE` means the job did write, and has printed nothing for ten minutes
since — often ordinary for a long compile, and worth a look if it was
supposed to be chatty.

`jbx health` lists all three, and stranded records are swept once they
are six hours past their last sign of life.

## A job says `background` long after it ended

If the machine rebooted, that is what it was. A job's ending is recorded
by its supervisor, and a supervisor that goes down with the machine
records nothing — leaving a record with no exit code, whose pid is the
only thing left to read. The kernel hands that number out again after
the reboot, and the job it once named reads as running under whatever
holds it now.

Such a record reads `gone` from 0.22.0 on: a job that started before the
machine came up cannot be running, and neither can a pid that names a
thread of another program. `jbx prune` clears it. Before 0.22.0, `jbx
kill` on one of these signalled whatever had inherited the number.

## On macOS

If macOS says the developer cannot be verified, the file was downloaded
by a browser, which flags it — `install.sh` uses `curl`, which does not.
[MACOS.md](MACOS.md) has the one-line remedy and why the binaries are
not notarised.

## On Windows

`WINDOWS.md` covers Smart App Control, which shell runs your commands,
and the two behaviours that genuinely differ. If a downloaded release
will not start at all, that is Smart App Control rather than jbx —
`install.ps1 -TrustLocally` is there for it, and
`CODE-SIGNING-POLICY.md` explains what it does and how to undo it.

## It feels slower

Measure it rather than guessing: `jbx bench` on your own machine. On the
one this was written on, the hook costs about six milliseconds on every
command and the wrapper about twenty-six on the ones it wraps — against
a bare shell's two. The numbers and what they exclude are in USAGE.md.

## Still stuck

Open an issue with `jbx config`, `jbx health` and `jbx --version`
pasted in. Those three answer most of what anyone would ask you next.
