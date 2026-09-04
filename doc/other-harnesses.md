# Wiring jobbox into any CLI

jobbox knows one harness — Claude Code — and only in two verbs named
after it. Everything else returns **facts**, and that is the integration
point.

If your CLI can run a command at some point in its loop, it can use
jobbox. You need about ten lines.

## The contract

```console
$ jobbox signals agent --json
{"id": "j7f3a91c", "queue_id": "0", "code": "0", "log": "/tmp/ts-out.x",
 "client": "myproject-1de7-92183ccf", "intent": "build-the-front",
 "command": "npm run build", "finished_at": 1756900000.0}
```

One object per job that has finished since the last look. Empty output
and exit 0 when nothing has.

| field | |
|---|---|
| `id` | jobbox's own id, never reused |
| `queue_id` | `task-spooler`'s number — restarts at zero with its daemon |
| `code` | the exit code, as a string |
| `log` | path to the job's output, an ordinary file |
| `client` | who queued it |
| `intent` | the name given at `jobbox run` |
| `command` | what ran |
| `finished_at` | unix time |

**Reading consumes.** The call returns each ending exactly once and
erases it, so there is no cursor to keep, no list of already-announced
jobs, and nothing to compare. Call it as often as you like; it is silent
until something has actually finished.

**Two audiences, drained separately.** `agent` is for the model, `user`
is for the person. That the model has read a result does not mean the
human has seen it, so each gets its own copy. Draining one leaves the
other untouched.

## The shape of an integration

Whatever your CLI calls a turn boundary, do this at it:

```sh
#!/bin/sh
# Called wherever your harness lets you run something per turn.
jobbox signals agent 2>/dev/null || true
```

That is enough for a text-based loop: the plain form prints one readable
line per ending. Three properties are worth keeping whatever you build
on top:

**Never fail.** A hook that errors is a hook somebody deletes. Silence
is the normal case — most turns have nothing to report.

**Never speak when there is nothing to say.** A line printed every turn
becomes a banner nobody reads, and then the one that matters is missed
too.

**Drain the audience you are.** If your integration serves the model,
read `agent` and leave `user` for whatever tells the person.

## No harness at all

Nothing requires one. `jobbox signals user` in a shell prompt, a desktop
notification, a `while` loop, an IRC bot — the verb returns facts and the
shaping is yours.

```sh
jobbox signals user --json | while read -r line; do
    notify-send "jobbox" "$(printf '%s' "$line" | jq -r '.intent + " — " + .code')"
done
```

## Adding a harness to jobbox itself

The Claude Code integration is two verbs and nothing else:

- `jobbox claude-hook <audience> <shape>` — the only place that knows
  what `systemMessage` and `decision` mean;
- `jobbox init` — writes that harness's settings file.

A second harness is a **second pair beside them**, not a change to
anything above. That boundary is deliberate and is stated in the source
where the temptation to blur it will come.

If you build one, it needs to answer two questions this one answers:
which moments in the loop can run a command, and which of them can reach
the model rather than a log. In Claude Code only one hook of three can,
which is why failures — and only failures — hold the session open.
