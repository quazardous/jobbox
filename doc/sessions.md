# Several sessions, one queue

The queue is shared by every session on the machine, deliberately.
Ordering and parallelism are a **machine-level** resource — that is what
`task-spooler` is for. Giving each session its own daemon would let N
sessions start N heavy jobs at once, which is the problem a queue exists
to prevent.

A new queue opens at **half the cores**. `jobbox slots 2` narrows it,
`JOBBOX_SLOTS=none` lifts the cap. The width is applied only when jobbox
is the one bringing a daemon into being, so a deliberate setting is never
quietly undone by the next command.

## What is not shared: the endings

Reading an ending erases it. That is what makes each job announced
exactly once with no state kept on the side — and it is also a theft if
two sessions share one mailbox: whichever looks first takes the other's,
and the loss is invisible, because what is missing is a job that
finished.

So **each session drains only its own**.

```console
$ jobbox clients
  project            session   unread
  (shared, user)               2
  BookShepherd-f7e5  92183ccf  1       ← you
  imagematch-4a01    d4a69872  1
```

**Your copy as a human stays shared**, on purpose. One person wants every
ending, whichever session started it — splitting that too would mean a
job launched by a session that has since closed is announced to nobody.
The split follows the reader, not the file.

Empty mailboxes left by finished sessions are forgotten on their own. One
still holding an ending never is: it is the only evidence that a job
finished and nobody was told.

## Names

Sessions name themselves, so none of this needs configuring:

```
myproject-1de7-92183ccf
└────────┘ └──┘ └──────┘
 directory  path  session
   name     hash
```

**A project is a directory, not a name.** `~/work/jobbox` and
`~/forks/jobbox` are two projects; letting them answer to one name would
put their jobs in one mailbox — the same theft, one level down, and
invisible because both names look right. Four hex characters of the
path's digest separate them, and `jobbox list --project-path` shows the
directory behind the tag.

The name is captured once, by `init`, and never derived at call time: a
client renamed because a command ran from a subdirectory would split its
mailbox mid-session and strand what was in it.

`JOBBOX_CLIENT` pins one fixed name instead — right for a CI runner or a
shared worker, wrong for a person with two windows open.

## Ids

**The id jobbox prints is its own, and never reused.**

`task-spooler` numbers jobs from zero and starts over when its daemon
dies, so a number kept from yesterday can name a different job today —
and would hand back the wrong log without a word.

A minted id does not make the queue outlive its daemon; nothing can. It
makes a stale reference **fail**:

```console
$ jobbox status j7f3a91c
  job j7f3a91c unknown to the queue        # and exit 1
```

`tsp`'s own number is still accepted wherever a job is named — refusing
it would make jobbox disagree with the queue anyone can read directly —
and `status` prints both. A job queued by hand with `tsp -L` has only a
number, and is listed under it.
