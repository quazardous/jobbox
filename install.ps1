# Put jbx in %LOCALAPPDATA%\jbx\bin.
#
#   irm https://raw.githubusercontent.com/quazardous/jobbox/main/install.ps1 | iex
#
# By default it downloads the binary for this machine from the latest
# release and checks it against the published sums. Nothing is compiled,
# nothing needs admin, and nothing is installed outside your profile.
#
# To pass an option through a pipe, PowerShell wants the scriptblock form:
#
#   & ([scriptblock]::Create((irm https://.../install.ps1))) -FromSource
#
#   -FromSource       build this checkout instead (needs cargo)
#   -Symlink          build, and point the install at the checkout
#   -Version vX.Y.Z   a particular release rather than the latest
#   -Uninstall        remove it; your logs and readings stay
#
# It offers to run `jbx init` at the end, and asks first. With no console
# to ask at, it prints the line instead of assuming.

param(
    [switch]$FromSource,
    [switch]$Symlink,
    [switch]$Uninstall,
    [switch]$TrustLocally,
    [string]$Version
)

$ErrorActionPreference = 'Stop'
$Repo = 'quazardous/jobbox'
$Bin  = if ($env:JBX_BIN) { $env:JBX_BIN } else { Join-Path $env:LOCALAPPDATA 'jbx\bin' }
$Exe  = Join-Path $Bin 'jbx.exe'
# PIPED FROM `irm`, THERE IS NO SCRIPT ON DISK -- so the checkout is only
# where one is, and building is only offered when one is.
$Src  = if ($PSScriptRoot) { $PSScriptRoot } else { $null }

function Say($text) { Write-Host "  $text" }

# THE USER'S PATH, NOT THE PROCESS'S -- AND THE DIFFERENCE IS NOT
# COSMETIC.
#
# `$env:PATH` is the machine PATH and the user PATH already joined
# together. Writing that back to the user scope, which is exactly what
# the advice printed here used to tell people to do
# (`setx PATH "$env:PATH;..."`), copies every machine entry into the
# user's own and leaves it there; `setx` compounds it by truncating at
# 1024 characters. So the value is read from the scope it will be written
# to, and from nowhere else.
function Add-ToUserPath($dir) {
    $current = [Environment]::GetEnvironmentVariable('PATH', 'User')
    $parts = @()
    if ($current) { $parts = @($current -split ';' | Where-Object { $_ }) }
    if ($parts -contains $dir) { return $false }
    [Environment]::SetEnvironmentVariable('PATH', (@($parts) + $dir) -join ';', 'User')
    # AND THE SHELL WE ARE STANDING IN, which will not see the registry
    # change on its own. Somebody who runs `jbx init` on the next line
    # should not have to open a new window between the two.
    $env:PATH = "$env:PATH;$dir"
    return $true
}

function Remove-FromUserPath($dir) {
    $current = [Environment]::GetEnvironmentVariable('PATH', 'User')
    if (-not $current) { return $false }
    $kept = @($current -split ';' | Where-Object { $_ -and $_ -ne $dir }) -join ';'
    if ($kept -eq $current) { return $false }
    [Environment]::SetEnvironmentVariable('PATH', $kept, 'User')
    return $true
}

# THE CERTIFICATE'S NAME IS ITS HANDLE. `-Uninstall` finds it by subject
# and by nothing else, so this string must not drift.
$CertSubject = 'CN=jbx local install'

# SIGN IT WITH A CERTIFICATE THIS MACHINE ALREADY TRUSTS.
#
# MEASURED, and the measurement is the whole reason this exists. On a
# Windows 11 with Smart App Control ON, jbx.exe is refused -- a published
# release as flatly as a local build, and the zip's checksum matched what
# the release published, so this was never about a bad download. Sign
# that same file with a self-signed code-signing certificate, put the
# certificate in the user's Root and TrustedPublisher stores, and it
# runs. Before the root is trusted the signature reads `UnknownError`,
# "a certificate chain ended in a root which is not trusted"; after, it
# reads `Valid` and the binary starts. So the local store DOES count,
# which is worth writing down because it is the opposite of what the
# documentation led us to expect.
#
# AND IT IS OPT-IN, AND IT STAYS OPT-IN. A root you trust can vouch for
# anything signed with it, and an installer that quietly adds one is the
# exact shape of the thing this control exists to stop. Four things keep
# it honest: the certificate can only sign code, that being the only EKU
# it carries; it is named so you can find it; `-Uninstall` takes it back
# out; and Windows asks you to confirm before the root goes in. That
# prompt is not an obstacle, it is the point -- nobody should be able to
# do this to you without your clicking.
function Remove-LocalTrust {
    param([switch]$Quiet)
    $gone = 0
    foreach ($store in @('Cert:\CurrentUser\My', 'Cert:\CurrentUser\Root', 'Cert:\CurrentUser\TrustedPublisher')) {
        Get-ChildItem $store -ErrorAction SilentlyContinue |
            Where-Object { $_.Subject -eq $CertSubject } |
            ForEach-Object { Remove-Item $_.PSPath -Force -ErrorAction SilentlyContinue; $gone++ }
    }
    if ($gone -and -not $Quiet) { Say "removed the local signing certificate." }
}

function Add-LocalTrust {
    Write-Host ""
    Say "Making a code-signing certificate for this machine, and trusting it."
    Say "Windows will ask you to confirm. The certificate signs code and"
    Say "nothing else. ``.\install.ps1 -Uninstall`` removes it again."
    # A STALE ONE FIRST: re-running must not leave a drawer of roots.
    Remove-LocalTrust -Quiet
    $cert = New-SelfSignedCertificate -Type CodeSigningCert -Subject $CertSubject `
                -CertStoreLocation Cert:\CurrentUser\My -NotAfter (Get-Date).AddYears(3)
    $cer = Join-Path ([System.IO.Path]::GetTempPath()) 'jbx-local-trust.cer'
    try {
        Export-Certificate -Cert $cert -FilePath $cer | Out-Null
        foreach ($store in @('Cert:\CurrentUser\Root', 'Cert:\CurrentUser\TrustedPublisher')) {
            Import-Certificate -FilePath $cer -CertStoreLocation $store | Out-Null
        }
    } finally {
        # THE EXPORTED COPY IS NOT NEEDED ONCE IT IS IN THE STORES.
        Remove-Item -Force -ErrorAction SilentlyContinue $cer
    }
    Set-AuthenticodeSignature -FilePath $Exe -Certificate $cert -HashAlgorithm SHA256 | Out-Null
    $status = (Get-AuthenticodeSignature $Exe).Status
    if ($status -ne 'Valid') {
        Say "the signature came back $status rather than Valid."
        Say "If you declined the confirmation, run it again and accept it."
        return $false
    }
    Say "signed, and the certificate is trusted."
    return $true
}

if ($Uninstall) {
    # THE HOOKS COME OUT BEFORE THE BINARY DOES, and the order is the
    # whole point: a settings file pointing at a binary that is gone
    # breaks every shell command in every session that reads it, and the
    # error names a path rather than a cause.
    if (Test-Path $Exe) {
        try { & $Exe init --undo } catch { Say "(could not undo the hooks -- check ``jbx init --undo``)" }
    }
    Remove-Item -Force -ErrorAction SilentlyContinue $Exe
    Say "removed $Exe"
    Remove-LocalTrust
    if (Remove-FromUserPath $Bin) { Say "took $Bin back out of your PATH." }
    Say "your logs and readings are untouched."
    exit 0
}

if ($FromSource -or $Symlink) {
    if (-not $Src -or -not (Test-Path (Join-Path $Src 'Cargo.toml'))) {
        Write-Host "install.ps1: no checkout here -- building needs the repository."
        Write-Host "  Run it without -FromSource to download the binary instead."
        exit 1
    }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Host "install.ps1: cargo is not on your PATH."
        Write-Host "  Run it without -FromSource to download the binary instead -- that needs"
        Write-Host "  nothing installed. rustup wants the Visual Studio build tools to link,"
        Write-Host "  which is a large download for a program under a megabyte."
        exit 1
    }
    Say 'building...'
    Push-Location $Src
    try { cargo build --release --quiet } finally { Pop-Location }
    New-Item -ItemType Directory -Force -Path $Bin | Out-Null
    Remove-Item -Force -ErrorAction SilentlyContinue $Exe
    $built = Join-Path $Src 'target\release\jbx.exe'
    if ($Symlink) {
        # A SYMLINK HERE NEEDS DEVELOPER MODE OR AN ELEVATED SHELL, so
        # failing is ordinary rather than exceptional: say which it is and
        # fall back, instead of stopping on something the user cannot fix
        # from inside this script.
        try {
            New-Item -ItemType SymbolicLink -Path $Exe -Target $built | Out-Null
            Say "linked  $Exe -> $built"
        } catch {
            Copy-Item $built $Exe
            Say "copied  $Exe   (a symlink needs Developer Mode or an elevated shell)"
        }
    } else {
        Copy-Item $built $Exe
        Say "copied  $Exe"
    }
} else {
    # WHICH BINARY. Only x86-64 is published; an ARM machine is told so
    # rather than handed something that will not run.
    if ($env:PROCESSOR_ARCHITECTURE -notmatch 'AMD64|x86') {
        Write-Host "install.ps1: no published binary for $env:PROCESSOR_ARCHITECTURE."
        Write-Host "  Build it: git clone https://github.com/$Repo; .\install.ps1 -FromSource"
        exit 1
    }

    if (-not $Version) {
        $latest = Invoke-RestMethod "https://api.github.com/repos/$Repo/releases/latest"
        $Version = $latest.tag_name
    }
    $name = "jbx-$Version-windows-x86_64.zip"
    $tmp  = Join-Path ([System.IO.Path]::GetTempPath()) ("jbx-" + [guid]::NewGuid())
    New-Item -ItemType Directory -Force -Path $tmp | Out-Null
    try {
        Say "fetching $Version for windows-x86_64..."
        $zip = Join-Path $tmp $name
        Invoke-WebRequest "https://github.com/$Repo/releases/download/$Version/$name" -OutFile $zip -UseBasicParsing

        # CHECKED AGAINST THE PUBLISHED SUMS. TLS says the bytes came from
        # GitHub; it does not say they are the bytes that release built.
        # A release without sums is said out loud rather than passed over.
        try {
            $sums = Invoke-WebRequest "https://github.com/$Repo/releases/download/$Version/SHA256SUMS" -UseBasicParsing
            # GITHUB SERVES A RELEASE ASSET AS `application/octet-stream`,
            # so PowerShell hands `Content` back as a Byte[] and not as a
            # string -- and `-split` then splits the BYTES. MEASURED: the
            # first three elements of SHA256SUMS came back `49 | 101 | 50`,
            # the decimal codes of "1e2". No line ever matched the file
            # name, so every Windows install this script has ever done
            # said "no sum published for this file" while the sum WAS
            # published, and installed the download unverified.
            $text = if ($sums.Content -is [byte[]]) {
                [System.Text.Encoding]::UTF8.GetString($sums.Content)
            } else { [string]$sums.Content }
            $line = $text -split "`r?`n" |
                Where-Object { $_ -match [regex]::Escape($name) } |
                Select-Object -First 1
            $want = ($line -split "\s+")[0]
            $got  = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
            if ($want -and $got -ne $want.ToLower()) {
                throw "the download does not match the published sum. Nothing was installed."
            }
            if ($want) { Say 'checksum ok' } else { Say 'no sum published for this file -- NOT verified.' }
        } catch [System.Net.WebException] {
            Say "$Version publishes no sums -- the download was NOT verified."
        }

        Expand-Archive -Path $zip -DestinationPath $tmp -Force
        New-Item -ItemType Directory -Force -Path $Bin | Out-Null
        Remove-Item -Force -ErrorAction SilentlyContinue $Exe
        Copy-Item (Join-Path $tmp "jbx-$Version-windows-x86_64\jbx.exe") $Exe
        Say "installed $Exe"
    } finally {
        # WHATEVER HAPPENS, THE SCRATCH GOES.
        Remove-Item -Recurse -Force -ErrorAction SilentlyContinue $tmp
    }
}

# THE LAST WORD IS THE BINARY'S OWN -- AND IT MAY NOT GET TO SPEAK.
#
# Smart App Control refuses unsigned executables whatever their origin, a
# published release included, and the call then throws about
# StandardOutputEncoding: a message naming nothing, printed just after a
# line saying the install worked. It did work. Say that, and say the one
# thing that is known to get past it.
if ($TrustLocally) {
    if (-not (Add-LocalTrust)) { exit 1 }
}

try {
    Say (& $Exe --version)
} catch {
    Say "installed -- but it will not start on this machine."
    Say "That is Smart App Control. It blocks unsigned binaries however"
    Say "they got here, a published release included, and it offers no"
    Say "exception for one file. What does get past it is signing the"
    Say "binary with a certificate this machine trusts -- measured on such"
    Say "a machine, not assumed:"
    Write-Host ""
    Say "    .\install.ps1 -TrustLocally"
    Write-Host ""
    Say "Read what that does before running it: it makes a code-signing"
    Say "certificate, trusts it for your user, and signs the binary with"
    Say "it. Windows asks you to confirm. ``-Uninstall`` takes it back out."
    exit 1
}

if (($env:PATH -split ';') -notcontains $Bin) {
    Write-Host ""
    if (Add-ToUserPath $Bin) {
        Say "added $Bin to your PATH."
        Say "This window has it already; other terminals need reopening."
    } else {
        Say "$Bin is on your PATH, but this shell has not picked it up yet."
        Say "Open a new terminal, or restart this one."
    }
}

# THE STEP NOBODY SHOULD HAVE TO BE TOLD TO TAKE.
#
# Installing jbx without declaring its hooks leaves a binary that does
# nothing: the whole tool is the hook. So the install offers to finish
# rather than printing a line and trusting it will be read.
#
# ASKED, NOT ASSUMED. `init` edits a settings file other tools share, and
# an installer that edits it unasked is one nobody should run. With no
# console to ask at -- a scheduled task, CI -- there is nobody to answer,
# so it says what to run and stops.
#
# `--global-only` because this runs wherever the user happened to be
# standing, and a file appearing in somebody's project because they
# installed a tool is a surprise.
Write-Host ""
$asked = $false
if ([Environment]::UserInteractive) {
    try {
        $answer = Read-Host "  Declare the hooks now? ``jbx init --global-only`` [Y/n]"
        $asked = $true
    } catch { $asked = $false }
}
if (-not $asked) {
    Say "Next: ``jbx init`` declares its hooks -- and takes rtk's over rather"
    Say "than racing it. ``jbx why`` says what it does and why."
} elseif ($answer -eq '' -or $answer -match '^(y|yes)$') {
    Write-Host ""
    & $Exe init --global-only
    Write-Host ""
    Say "Done. ``jbx why`` says what it does and why."
} else {
    Say "Left alone. ``jbx init`` declares them when you want them."
}
