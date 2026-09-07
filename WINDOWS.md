# jbx on Windows

It works there. The full test suite runs on a Windows runner on every
change and passes, the release binaries are built there, and somebody
has walked the whole thing by hand on a Windows 11 desktop in a real
Claude Code session.

This page holds the two things that are genuinely different on Windows.
Everything else — the verbs, the settings, what the tool is for — is in
the [README](README.md) and is the same everywhere.

## Installing

```powershell
irm https://raw.githubusercontent.com/quazardous/jobbox/main/install.ps1 | iex
jbx init
```

It downloads the binary for your machine, checks it against the sum
published with the release, puts it in `%LOCALAPPDATA%\jbx\bin`, and adds
that directory to your `PATH` — this window included, so `jbx init` on
the next line works without opening a new terminal.

To pass an option through the pipe, PowerShell wants the scriptblock
form:

```powershell
& ([scriptblock]::Create((irm https://raw.githubusercontent.com/quazardous/jobbox/main/install.ps1))) -TrustLocally
```

| flag | what it does |
|---|---|
| `-TrustLocally` | sign the installed binary so Smart App Control accepts it |
| `-FromSource` | build this checkout instead (needs cargo) |
| `-Symlink` | build, and point the install at the checkout |
| `-Version vX.Y.Z` | a particular release rather than the latest |
| `-Uninstall` | remove it: the binary, the `PATH` entry, the hooks, the certificate |

You can also take `jbx.exe` from the
[releases](https://github.com/quazardous/jobbox/releases) by hand and put
it anywhere on your `PATH`.

## Smart App Control, and why you may need `-TrustLocally`

**These releases are not signed yet.** A signature Windows accepts on its
own means a code-signing certificate bought from an authority it already
trusts and renewed every year, and there is not one for this project
*yet*. That is the whole of the reason, and it is the only reason.

If [Smart App Control][sac] is on — Windows 11 only, and off on most
machines that were upgraded rather than installed fresh — it refuses
unsigned executables, and it refuses them **whatever their origin**:

- a build from source fails first, and not at `jbx.exe` — the build
  scripts, the test binaries and `cargo-clippy.exe` are refused too, as
  `os error 4551`;
- a published release is refused in exactly the same way. Measured: the
  zip downloaded, its SHA-256 matched the sum that release published, and
  the binary inside still would not start.

There is no per-file exception to grant, and turning the feature off is a
one-way door — Microsoft documents that it cannot be turned back on
without reinstalling Windows. So do not turn it off for this.

**What works is a certificate your own machine trusts:**

```powershell
.\install.ps1 -TrustLocally
```

It makes a code-signing certificate, asks Windows to trust it for your
user, and signs the installed binary with it. Measured on a machine with
Smart App Control on: before the certificate is trusted the signature
reads `UnknownError`, *"a certificate chain ended in a root which is not
trusted"*; after, it reads `Valid` and jbx starts. The local trust store
counts — which is worth saying plainly, because the documentation reads
as though only Microsoft's opinion did.

### Read this before you use that flag

Signing jbx locally does not make it more trustworthy than it was. It
makes it **identifiable**, which is what the control is actually asking
for. Nothing is wrong with the download either: the checksum published
beside it matches, and you can check it yourself.

But a root certificate you trust can vouch for anything signed with it,
so an installer that adds one without being told to is the exact thing
this control exists to stop. Four things keep it honest:

- the certificate carries the code-signing use and no other;
- it is named `jbx local install`, so you can find it in
  `certmgr.msc`;
- Windows asks you to confirm before it goes into your trust store —
  that prompt is the point, not an obstacle;
- `.\install.ps1 -Uninstall` takes it back out.

None of that makes it free. It makes it yours to decide, which is why it
is a flag and not something the script does on your behalf when a launch
fails.

**When there is a real certificate this page gets shorter**, and
`-TrustLocally` stops being needed.

[sac]: https://support.microsoft.com/en-us/topic/what-is-smart-app-control-285ea03d-fa88-4d56-882e-6698afdb7003

## Which shell runs your commands

**This is the part that decides whether anything works.** The hook
rewrites a command into `jbx run -- '<line>'`, quoted for a POSIX shell —
which is right, because Claude Code on Windows drives Git Bash.

So jbx runs the line with `bash` whenever `bash` is on the `PATH`, and
falls back to `cmd /C` only when there is none. `jbx config` prints which
one it picked; `shell: cmd` in the configuration settles it if the guess
is wrong for your setup.

## Two things that behave differently

By construction rather than by neglect:

- **Files live in `%LOCALAPPDATA%\jbx` and `%APPDATA%\jobbox\config.yaml`.**
- **`jbx health` never calls a job mute for the right reason.** Liveness
  from the log still works; the extra observation about a line reading
  its input needs `/proc`, which Windows has not. It answers "I do not
  know" rather than guessing — the same rule that cost a false alarm on
  Linux.

## Building from source

```powershell
git clone https://github.com/quazardous/jobbox
cd jobbox
.\install.ps1 -FromSource
```

Needs cargo, and rustup wants the Visual Studio build tools to link —
a large download for a program under a megabyte, which is why the
default is to fetch the release instead.

With Smart App Control on, this is where it stops: freshly built
unsigned executables are refused before `jbx.exe` is even reached. There
is nothing to work around; use the published release and
`-TrustLocally`.

`install.ps1` runs under both `powershell` 5.1 and `pwsh` 7. It is kept
in plain ASCII on purpose: 5.1 reads a `.ps1` with no byte-order mark as
ANSI, and a single em-dash was once enough to break the parse into ten
errors, none of them near the cause.
