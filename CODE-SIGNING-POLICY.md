# Code signing policy

This page exists because a signing service is asked to vouch for us, and
it should be able to read what it is vouching for. It describes who can
change this code, who can release it, what the released program does to
the machine it runs on, and what it does not do.

It is written to be checked, not believed. Every claim below points at a
file or a command.

## What is signed

The Windows executable `jbx.exe`, as published on the
[releases page](https://github.com/quazardous/jobbox/releases), built by
[`.github/workflows/release.yml`](.github/workflows/release.yml) from the
tagged commit and from nothing else.

Nothing is built on a maintainer's machine and uploaded. The workflow is
the only publisher, and the archive it produces carries `LICENSE` and
`README.md` beside the binary.

The binary declares its own identity: `build.rs` writes a Windows
`VERSIONINFO` block, and the release refuses to continue if the built
file's `ProductVersion` disagrees with the tag. A nameless binary cannot
leave here by accident.

## Roles

The project has a single maintainer, and this section says so plainly
rather than describing a committee that does not exist.

| role | who | what they may do |
|---|---|---|
| **Author** | anyone | open a pull request. No write access is needed or granted. |
| **Reviewer** | the maintainer | every pull request from outside is read before it lands. |
| **Approver** | the maintainer | tags a release and approves each signing request individually. |

Accounts with write access must have multi-factor authentication enabled.

Contributions arrive as pull requests against `main`. Releases are cut
from tags; the workflow signs what the tag names, and a release that was
not approved is not signed.

## Privacy

**jbx collects nothing and transmits nothing.** There is no telemetry, no
update check, no crash reporting, no analytics — the program opens no
network connection at all. That is checkable in one command:

```console
grep -rn "TcpStream\|UdpSocket\|reqwest\|hyper\|http" src/ --include=*.rs
```

The only match is a `$schema` URL printed as text by `jbx describe`. It is
a string in JSON output, never fetched.

What jbx writes stays on the machine:

- job records and command output, under `~/.cache/jbx/jobs` on Linux and
  the platform equivalent elsewhere — `jbx config` prints the real path,
  and `JBX_DIR` moves it;
- its own settings, in `.jbx.yaml` in a project or in the user's
  configuration directory;
- a hook declaration in the harness's settings file, written by
  `jbx init` and removed by `jbx init --undo`.

Command output is written because that is the program's purpose — a
detached job has to put its output somewhere the caller can read it
later. It is never sent anywhere.

## Changes to the system, and undoing them

Installing jbx puts one binary on the machine, adds its directory to the
user's `PATH`, and declares a hook. Each is reversible:

```powershell
.\install.ps1 -Uninstall     # binary, PATH entry, hook, and local trust
```

```console
jbx init --undo              # the hook alone
```

Logs and recorded readings are left alone by an uninstall, and the
message says so, because deleting a user's data on the way out is not the
installer's decision to make.

## A disclosure we would rather make than have found

`install.ps1` carries an **opt-in** flag, `-TrustLocally`. It creates a
self-signed code-signing certificate, asks Windows to trust it — the
system prompts, and the person must accept — signs the installed binary
with it, and then **destroys the private key**, verifying the signature
again afterwards to prove the key was not needed for that.

We are naming it here because, read quickly, it resembles something the
Foundation's terms prohibit: a tool that circumvents a security measure.
Our reading is that the prohibition is about software whose *function* is
circumvention, and this is an installer flag that:

- does nothing unless explicitly asked for, and is never used to recover
  from a failed launch;
- cannot act without the user accepting a Windows trust prompt;
- creates a certificate with the code-signing usage and no other, named
  `jbx local install` so it is findable in `certmgr.msc`;
- keeps no key afterwards — it signs once and cannot sign again;
- is removed by `.\install.ps1 -Uninstall`, key material included;
- is documented, with its risks, in
  [WINDOWS.md](WINDOWS.md#smart-app-control-and-why-you-may-need--trustlocally).

It exists because the releases are not signed yet, and Smart App Control
refuses unsigned executables with no per-file exception. **A signature is
what makes it unnecessary for released binaries**, which is why we are
applying at all. If the Foundation would rather it did not exist in a
signed project, say so and it goes.

## Attribution

Free code signing is provided by [SignPath.io](https://signpath.io),
certificate by the [SignPath Foundation](https://signpath.org).

*(This section states the intended attribution; it becomes a statement of
fact once an application is accepted.)*
