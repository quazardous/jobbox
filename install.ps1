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
#   & ([scriptblock]::Create((irm https://…/install.ps1))) -FromSource
#
#   -FromSource       build this checkout instead (needs cargo)
#   -Symlink          build, and point the install at the checkout
#   -Version vX.Y.Z   a particular release rather than the latest
#   -Uninstall        remove it; your logs and readings stay

param(
    [switch]$FromSource,
    [switch]$Symlink,
    [switch]$Uninstall,
    [string]$Version
)

$ErrorActionPreference = 'Stop'
$Repo = 'quazardous/jobbox'
$Bin  = if ($env:JBX_BIN) { $env:JBX_BIN } else { Join-Path $env:LOCALAPPDATA 'jbx\bin' }
$Exe  = Join-Path $Bin 'jbx.exe'
# PIPED FROM `irm`, THERE IS NO SCRIPT ON DISK — so the checkout is only
# where one is, and building is only offered when one is.
$Src  = if ($PSScriptRoot) { $PSScriptRoot } else { $null }

function Say($text) { Write-Host "  $text" }

if ($Uninstall) {
    # THE HOOKS COME OUT BEFORE THE BINARY DOES, and the order is the
    # whole point: a settings file pointing at a binary that is gone
    # breaks every shell command in every session that reads it, and the
    # error names a path rather than a cause.
    if (Test-Path $Exe) {
        try { & $Exe init --undo } catch { Say "(could not undo the hooks — check ``jbx init --undo``)" }
    }
    Remove-Item -Force -ErrorAction SilentlyContinue $Exe
    Say "removed $Exe"
    Say "your logs and readings are untouched."
    exit 0
}

if ($FromSource -or $Symlink) {
    if (-not $Src -or -not (Test-Path (Join-Path $Src 'Cargo.toml'))) {
        Write-Host "install.ps1: no checkout here — building needs the repository."
        Write-Host "  Run it without -FromSource to download the binary instead."
        exit 1
    }
    if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
        Write-Host "install.ps1: cargo is not on your PATH."
        Write-Host "  Run it without -FromSource to download the binary instead — that needs"
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
        Invoke-WebRequest "https://github.com/$Repo/releases/download/$Version/$name" -OutFile $zip

        # CHECKED AGAINST THE PUBLISHED SUMS. TLS says the bytes came from
        # GitHub; it does not say they are the bytes that release built.
        # A release without sums is said out loud rather than passed over.
        try {
            $sums = Invoke-WebRequest "https://github.com/$Repo/releases/download/$Version/SHA256SUMS"
            $want = ($sums.Content -split "`n" | Where-Object { $_ -match [regex]::Escape($name) }) -split '\s+' | Select-Object -First 1
            $got  = (Get-FileHash $zip -Algorithm SHA256).Hash.ToLower()
            if ($want -and $got -ne $want.ToLower()) {
                throw "the download does not match the published sum. Nothing was installed."
            }
            if ($want) { Say 'checksum ok' } else { Say 'no sum published for this file — NOT verified.' }
        } catch [System.Net.WebException] {
            Say "$Version publishes no sums — the download was NOT verified."
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

Say (& $Exe --version)

if (($env:PATH -split ';') -contains $Bin) {
    Write-Host ""
    Say "Next: ``jbx init`` declares its hooks — and takes rtk's over rather"
    Say "than racing it. ``jbx why`` says what it does and why."
} else {
    Write-Host ""
    Say "$Bin is NOT on your PATH. The hooks jbx declares will still work"
    Say "(they carry the full path) but you cannot type ``jbx``. To add it:"
    Say "    setx PATH `"`$env:PATH;$Bin`""
    Say "then open a new terminal."
}
