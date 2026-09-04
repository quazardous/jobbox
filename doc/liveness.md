# Running is not the same as making progress

`tsp` tells you whether a job **runs**. It never tells you whether it is
**getting anywhere**: a wedged script and a computing one are both
`running`.

jobbox reads the date of the last byte written to the job's log.

```
running + fresh log            it is working
running + nothing for a while  MUTE?   ← what `health` names
not running                    done, and the code says how
```

```console
$ jobbox status j679cf47
  …
  last wrote 41s ago

$ jobbox health
  daemon     alive, 1 job(s) known
  slots      1/6 busy, 0 waiting
  STUCK?     j679cf47 reindex — nothing written for 900s
```

`JOBBOX_MUTE_AFTER` moves the threshold, 600 seconds by default.

## Why a date and not a heartbeat

A heartbeat would only work for scripts you write yourself, and whoever
forgot to emit one would look dead. Log freshness **rewards the script
that says where it is at** — it gets precise liveness for free — and asks
nothing of the others, which get "I do not know". That is the honest
answer, and it is better than a wrong one.

## `health` returns 0 on a mute job

Deliberately. It may be a script computing without saying anything, and
returning a failure would make `health` an alarm people switch off. It
**names** the job so somebody goes and looks; it does not judge.

## Asking whether the daemon is alive is what starts it

`tsp -l` on a fresh socket does not fail — it starts the server and
returns an empty list. So "did the daemon answer" is a question this verb
cannot settle by asking it, and a queue that died with its daemon would
otherwise read as an ordinary empty queue.

What *is* observable is whether the socket existed **before** the
question:

```console
$ jobbox health
  daemon     STARTED BY THIS CHECK — it was not running
             anything queued before it died with it
             opened at 6 slot(s) (half the cores; JOBBOX_SLOTS overrides)
```

A stale socket left by a hard kill would read as "was up". The check is
honest about what it sees, not about what happened.

## Endings nobody came back for

```console
$ jobbox health
  UNREAD     2 ending(s) in 1 other mailbox(es)
             myproject-1de7-d4a69872 holds 2
             `jobbox signals agent --client <name>` reads one
```

Who a client is belongs to the harness, not to jobbox — it names sessions
from an identifier we do not control. If one ever comes looking under a
different name than its jobs were filed under, those endings wait
forever. jobbox does not guess which mailboxes are abandoned: an idle
session's looks exactly like a dead one's, and draining someone else's on
a guess is the theft the whole design removes. It only makes them
visible, and only from `health` — saying it every turn would be a banner.
