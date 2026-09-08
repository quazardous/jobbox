# JobBox

### Time is money. Your agent spends both, standing still.

A five-minute build runs. The agent waits. You wait. Nothing else
happens — and you are billed for all of it twice: your hour, and the
tokens burning in a session doing nothing.

No single wait is worth stopping for. **It is their sum that costs**, and
the sum is invisible until something counts it.

```console
$ jbx stats
project      calls  detached  elapsed  waited  saved
acme             12         0      14m    13m52s     8s (1%)
  api            96         7    2h11m   18m03s  1h53m (86%)
  front          34         4      39m    03m17s   36m (92%)

last hour       6 calls ·    1 detached ·   8m12s saved (73%)
last day       38 calls ·    4 detached ·  40m05s saved (77%)
all           142 calls ·   11 detached ·   2h29m saved (81%)
```

That is one week. Put your own rate on it.

---

**JobBox wraps every command your agent runs.** The quick ones come back
untouched — output as written, exit code unchanged, as though nothing
were there. The slow ones **detach themselves**, say so, and tell whoever
needs to know when they end.

Nobody judges in advance which is which. That judgement is the thing
everybody gets wrong, so JobBox does not make it: it runs the line and
finds out.

One binary. Rust, `serde_json`, nothing else. Linux, macOS, Windows.

## Install

As a Claude Code plugin — the hooks, the binary and a background watch in
one thing:

```console
$ claude plugin marketplace add quazardous/jobbox
$ claude plugin install jbx@jobbox
```

Or as a command, anywhere:

```console
$ curl -fsSL https://raw.githubusercontent.com/quazardous/jobbox/main/install.sh | sh
```

It checks the download against the sums published with the release, puts
it on your `PATH`, and **asks** before declaring its hooks — they go in a
settings file other tools share. On Windows, `irm
https://raw.githubusercontent.com/quazardous/jobbox/main/install.ps1 |
iex`, and [WINDOWS.md](WINDOWS.md) has the rest.

**Run `jbx init` as well if you have [rtk](https://github.com/rtk-ai/rtk).**
A plugin declares hooks; it cannot displace somebody else's, and two
hooks rewriting one field is a race no harness documents. `init` settles
that by calling rtk itself.

## What it looks like

Nothing changes — until something is slow:

```console
$ npm run build
> building…
jbx: this passed 30s, so it is in the BACKGROUND as j7f3a91c — nothing lost.
DO NOT WAIT FOR IT, DO SOMETHING ELSE. With nothing else: Monitor
`jbx wait j7f3a91c`, which ends when the job does.

  jbx help j7f3a91c
```

The build output arrived **as it was written**, not replayed at the end.

**The ending reaches you two ways, and one is better.** Left alone, it is
announced on the next turn — free, and it costs that delay. Monitored, it
arrives the moment it happens: `jbx wait <id>` ends exactly when the job
does, so anything watching it is woken then. `jbx watch` does that for
every job at once, one line each.

That is the difference between waiting and being told. Polling is
neither — it is waiting with extra steps.

## The one judgement left to make

The old answer to "when should this go to the background?" was a document
telling an agent to estimate how long a command would take. Agents get
that wrong, and so do people. jbx removes the question and leaves a
smaller one:

> Do you need this result **before you can do anything else**?

Almost always, no. When the answer is yes, say so — `jbx fg -- '<line>'`
runs without ever letting go, and `jbx stats` counts what that cost.

## The rest

- **[USAGE.md](USAGE.md)** — every verb, every setting, how `saved` is
  counted and why it is a ceiling, and what was deliberately left out.
- **[WINDOWS.md](WINDOWS.md)** — Smart App Control, which shell runs your
  commands, and the two things that differ there.
- **[CONTRIBUTING.md](CONTRIBUTING.md)** — what belongs in a test, and
  what has already been ruled out.
- **[CHANGELOG.md](CHANGELOG.md)** — what changed, and why it mattered.

## License

MIT — see [LICENSE](LICENSE).
