# Put jbx on your PATH, from this checkout. The Windows counterpart of
# install.sh.
#
# IF YOU DO NOT ALREADY BUILD RUST ON THIS MACHINE, DOWNLOAD THE BINARY
# INSTEAD — the release page has one, under a megabyte, nothing to
# install. `winget install Rustlang.Rustup` does get you cargo, but
# rustup then asks for the Visual Studio build tools to link with, which
# is several gigabytes of prerequisite for a program this size. That is a
# fine trade if you were going to write Rust anyway, and a poor one if
# you were not.
#
#   .\install.ps1              build and copy
#   .\install.ps1 -Symlink     point the install at this checkout
#   .\install.ps1 -Uninstall   remove it; your logs and readings stay

param(
    [switch]$Symlink,
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'

$Bin = Join-Path $env:LOCALAPPDATA 'jbx\bin'
$Src = Split-Path -Parent $MyInvocation.MyCommand.Path
$Exe = Join-Path $Bin 'jbx.exe'

if ($Uninstall) {
    # THE HOOKS COME OUT BEFORE THE BINARY DOES, and the order is the
    # whole point: a settings file pointing at a binary that is gone
    # breaks every shell command in every session that reads it, and the
    # error names a path rather than a cause.
    if (Test-Path $Exe) {
        try { & $Exe init --undo } catch { Write-Host "  (could not undo the hooks — check ``jbx init --undo``)" }
    }
    Remove-Item -Force -ErrorAction SilentlyContinue $Exe
    Write-Host "  removed $Exe"
    Write-Host "  your logs and readings are untouched, in $($env:JBX_DIR ?? "$env:LOCALAPPDATA\jbx")."
    exit 0
}

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    Write-Host "install.ps1: cargo is not on your PATH."
    Write-Host "  The quick way is not to build at all: grab jbx.exe from"
    Write-Host "  https://github.com/quazardous/jobbox/releases and skip the rest."
    Write-Host ""
    Write-Host "  To build anyway:  winget install Rustlang.Rustup"
    Write-Host "  rustup will then ask for the Visual Studio build tools, which it"
    Write-Host "  needs to link. Expect a large download."
    exit 1
}

Write-Host "  building..."
Push-Location $Src
try { cargo build --release --quiet } finally { Pop-Location }

New-Item -ItemType Directory -Force -Path $Bin | Out-Null
Remove-Item -Force -ErrorAction SilentlyContinue $Exe
$Built = Join-Path $Src 'target\release\jbx.exe'

if ($Symlink) {
    # A SYMLINK ON WINDOWS NEEDS DEVELOPER MODE OR AN ELEVATED SHELL, so
    # failing here is ordinary rather than exceptional — say which it is
    # and fall back, instead of stopping on something the user cannot fix
    # from inside this script.
    try {
        New-Item -ItemType SymbolicLink -Path $Exe -Target $Built | Out-Null
        Write-Host "  linked  $Exe -> $Built"
    } catch {
        Copy-Item $Built $Exe
        Write-Host "  copied  $Exe   (a symlink needs Developer Mode or an elevated shell)"
    }
} else {
    Copy-Item $Built $Exe
    Write-Host "  copied  $Exe"
}

Write-Host "  $(& $Exe --version)"

$onPath = ($env:PATH -split ';') -contains $Bin
if ($onPath) {
    Write-Host ""
    Write-Host "  Next: ``jbx init`` declares its hooks — and takes rtk's over rather"
    Write-Host "  than racing it. ``jbx init --undo`` puts everything back."
} else {
    Write-Host ""
    Write-Host "  $Bin is NOT on your PATH. The hooks jbx declares will still work"
    Write-Host "  (they carry the full path) but you cannot type ``jbx``. To add it:"
    Write-Host "      setx PATH `"`$env:PATH;$Bin`""
    Write-Host "  then open a new terminal."
}
