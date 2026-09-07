# JobBox

### Time is money. Your agent spends both, standing still.

A five-minute build runs. The agent waits. You wait. Nothing else
happens — and you are billed for all of it, twice: your hour, and the
tokens burning in a session that is doing nothing.

No single wait is worth stopping for. **It is their sum that costs**, and
the sum is invisible until something counts it.

```console
$ jbx stats
project      calls  detached  elapsed  waited  saved
acme             12         0      14m    13m52s     8s (1%)
  api            96         7    2h11m   18m03s  1h53m (86%)
  front          34         4      39m    03m17s   36m (92%)

2h29m saved — command time that ran while the caller was free, 81% of 3h04m.
`waited` is what you actually stood still for, and `saved` is the rest
of `elapsed` — it already subtracts the time you gave back to `jbx wait`.
It cannot see you waiting some other way: a ceiling, not a receipt.
```

That is one week. Put your own rate on it.

---

**JobBox wraps every command your agent runs.** The quick ones come back
untouched — output as it is written, exit code unchanged, as though
nothing were there. The slow ones **detach themselves**, say so, and tell
whoever needs to know when they end.

Nobody judges in advance which is which. That judgement is the thing
everybody gets wrong, so JobBox does not make it: it runs the line and
finds out.

The command is `jbx`.

One binary. Rust, `serde_json`, nothing else. Linux, macOS, Windows.

## Quickstart

```console
$ curl -fsSL https://raw.githubusercontent.com/quazardous/jobbox/main/install.sh | sh
$ jbx init                        # declares its hooks, merges safely
```

On Windows:

```powershell
irm https://raw.githubusercontent.com/quazardous/jobbox/main/install.ps1 | iex
```

It downloads the binary for your machine — under a megabyte, nothing
compiled, nothing outside your home — and checks it against the sums
published with the release. `--from-source` builds a checkout instead,
`--uninstall` removes it, `--version=vX.Y.Z` pins one. The
[releases](https://github.com/quazardous/jobbox/releases) hold the
archives if you would rather do it by hand.

Open a new session. Nothing changes — until something is slow:

```console
$ npm run build
> building…
jbx: this passed 30s, so it is now in the BACKGROUND — detached as j7f3a91c.
Nothing was lost. It is still running, and still printing to its log.

DO NOT SIT AND WAIT FOR IT. You will be told when it ends, on a later turn —
waiting here is the exact cost jbx exists to remove. Go and do something else.

  jbx how j7f3a91c   what you can do with it   ·   jbx why   why it works this way
```

The build output arrived **as it was written**, not replayed at the end.
When it finishes, the model is told on its next turn and you are told
when the session stops — a failure holds the session open and points at
the log.

## The one judgement left to make

The old answer to "when should this go to the background?" was a document
telling an agent to estimate how long a command would take. Agents get
that wrong, and so do people. jbx removes the question and leaves a
smaller one, asked where the agent will read it:

> Do you need this result **before you can do anything else**?

Almost always, no. When the answer is yes, say so — `jbx fg -- '<line>'`
runs without ever letting go, and `jbx stats` counts what that cost.

## On Windows

It works there — the suite runs on a Windows runner on every change and
passes. **One thing is different and it is temporary: these releases are
not signed yet**, so if Smart App Control is on it refuses the binary,
a published release as flatly as a local build. `-TrustLocally` signs
the install with a certificate your own machine trusts, which gets past
it.

**[WINDOWS.md](WINDOWS.md)** has that flag, which shell runs your
commands, and the two things that behave differently there.

## The rest

- **[USAGE.md](USAGE.md)** — every verb, every setting, how `saved` is
  counted and why it is a ceiling, how it composes with rtk, and what
  was deliberately left out.
- **[WINDOWS.md](WINDOWS.md)** — Smart App Control, which shell runs
  your commands, and the two things that behave differently there.
- **[CONTRIBUTING.md](CONTRIBUTING.md)** — what belongs in a test, and
  what has already been ruled out.
- **[CHANGELOG.md](CHANGELOG.md)** — what changed, and why it mattered.

## License

MIT — see [LICENSE](LICENSE).
