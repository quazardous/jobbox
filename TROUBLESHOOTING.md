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

## A job is `MUTE`, or `stranded`

`MUTE` means nothing has been printed for ten minutes — often ordinary
for a long compile, and worth a look if it was supposed to be chatty.
`jbx health` lists both, and stranded records are swept once they are
six hours past their last sign of life.

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
