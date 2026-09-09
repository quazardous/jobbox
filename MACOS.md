# On macOS

Two builds are published, `macos-arm64` and `macos-x86_64`, and
`install.sh` picks the right one. Neither is notarised. This says what
that costs, and when it costs nothing at all.

## The install command is unaffected

```console
$ curl -fsSL https://raw.githubusercontent.com/quazardous/jobbox/main/install.sh | sh
```

Gatekeeper's warning comes from a **quarantine flag**, and that flag is
written by the application that downloads the file — a browser sets it,
`curl` does not. So a binary fetched by `install.sh` carries no flag,
and macOS never asks about it.

That is the path the README gives and the one to give anybody else.

And it proves itself: `install.sh` runs `jbx --version` once the binary
is in place, so an install that Gatekeeper had blocked would fail there,
loudly, instead of leaving something that refuses to start later.

## Downloading a release in a browser is the case that bites

Click a `.tar.gz` on the releases page in Safari or Chrome and the file
arrives flagged. Open it and macOS refuses:

> **"jbx" cannot be opened because the developer cannot be verified.**

Two ways past it, both a few seconds:

```console
$ xattr -d com.apple.quarantine ./jbx     # drop the flag, then run it
```

or right-click the binary in Finder and choose **Open**, which offers a
one-off exception. Either is a decision to trust a binary you fetched
from a public repository — check it against `SHA256SUMS` first, which is
published with every release.

## Why it is not notarised

Notarising needs an Apple Developer Program membership at **$99 a year**,
a Developer ID certificate, and a submission step on every release. It
would remove one warning, on the install path this project does not
document, for people who chose to download by hand instead of running
the one-line command.

That trade may become worth making — a Homebrew formula would need it,
for instance. It is not worth making today, and pretending the binaries
are signed when they are not would be worse than saying this.

**The same reasoning is written down for Windows** in
[CODE-SIGNING-POLICY.md](CODE-SIGNING-POLICY.md), where the answer came
out differently because Smart App Control blocks the install path itself
rather than a detour around it.

## What has not been verified here

The mechanism above is Apple's documented behaviour — quarantine is set
by the downloading application, not by the file — but **nobody has run
these binaries on a Mac and watched what happens**. This project is
developed on Linux and its macOS builds come from CI.

If you are on a Mac: run `install.sh`, then `jbx config`, and say in an
issue whether anything asked you a question. That is a more useful
report than it sounds.
