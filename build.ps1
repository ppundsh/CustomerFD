#Requires -Version 5.1
<#
.SYNOPSIS
    Builds CustomerFD (release) and packages dist\CustomerFD-Setup.exe.
.DESCRIPTION
    1. Builds cfd_shell.dll and cfd-manager.exe.
    2. Builds the installer, which embeds those binaries plus LICENSE and README.md.
    3. Refuses to ship binaries that contain build-machine paths (user name / folders).
    Output: dist\CustomerFD-Setup.exe (and the loose binaries for development).
#>
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path

Push-Location $Root
$savedRustFlags = $env:RUSTFLAGS
try {
    $relDir = Join-Path $Root "target\release"
    $distDir = Join-Path $Root "dist"

    Stop-Process -Name "cfd-manager", "cfd-installer", "CustomerFD-Setup" -Force -ErrorAction SilentlyContinue

    # ---- Privacy: strip build-machine paths from the binaries -------------------------------
    # rustc embeds absolute source paths (panic locations) for every crate. Remap them to
    # neutral prefixes; when several match, rustc applies the LAST one.
    $cargoHome = if ($env:CARGO_HOME) { $env:CARGO_HOME } else { Join-Path $env:USERPROFILE ".cargo" }
    $rustupHome = if ($env:RUSTUP_HOME) { $env:RUSTUP_HOME } else { Join-Path $env:USERPROFILE ".rustup" }
    $remap = @()
    if ($env:USERPROFILE) { $remap += "--remap-path-prefix=$($env:USERPROFILE)=~" }
    $remap += "--remap-path-prefix=$rustupHome=/rustup"
    $remap += "--remap-path-prefix=$cargoHome=/cargo"
    $remap += "--remap-path-prefix=$Root=/src"
    $env:RUSTFLAGS = (@($env:RUSTFLAGS) + $remap | Where-Object { $_ }) -join ' '

    # Explorer keeps a registered cfd_shell.dll loaded (e.g. one registered from target\release
    # during development). Cargo cannot replace a loaded DLL, but Windows allows renaming it.
    foreach ($dll in @((Join-Path $relDir "cfd_shell.dll"), (Join-Path $relDir "deps\cfd_shell.dll"))) {
        Get-ChildItem (Split-Path $dll) -Filter "cfd_shell.dll.*.old" -ErrorAction SilentlyContinue |
            Remove-Item -Force -ErrorAction SilentlyContinue
        if (Test-Path $dll) {
            try { [System.IO.File]::OpenWrite($dll).Close() }
            catch {
                $old = "cfd_shell.dll.$([DateTimeOffset]::UtcNow.ToUnixTimeSeconds()).old"
                Rename-Item -Path $dll -NewName $old
                Write-Host "    $dll is in use by Explorer; renamed to $old" -ForegroundColor Yellow
            }
        }
    }

    Write-Host "==> Building cfd_shell.dll and cfd-manager.exe..." -ForegroundColor Cyan
    cargo build --release -p cfd-shell -p cfd-manager
    if ($LASTEXITCODE -ne 0) { throw "cargo build --release failed ($LASTEXITCODE)" }

    Write-Host "==> Packaging installer (CustomerFD-Setup.exe)..." -ForegroundColor Cyan
    $env:CFD_PAYLOAD_DIR = $relDir
    $env:CFD_PACK_STAMP = [DateTimeOffset]::UtcNow.ToUnixTimeSeconds().ToString()
    cargo build --release -p cfd-installer
    if ($LASTEXITCODE -ne 0) { throw "cfd-installer build failed ($LASTEXITCODE)" }

    New-Item -ItemType Directory -Force -Path $distDir | Out-Null
    # Explorer keeps a registered cfd_shell.dll loaded: rename a locked file out of the way.
    function Copy-Replacing([string]$src, [string]$dst) {
        try { Copy-Item -Force $src $dst -ErrorAction Stop }
        catch {
            $old = "$dst.$([DateTimeOffset]::UtcNow.ToUnixTimeSeconds()).old"
            Rename-Item -Path $dst -NewName (Split-Path -Leaf $old) -ErrorAction Stop
            Copy-Item -Force $src $dst -ErrorAction Stop
        }
    }
    Get-ChildItem $distDir -Filter "*.old" -ErrorAction SilentlyContinue | Remove-Item -Force -ErrorAction SilentlyContinue
    $setupDst = Join-Path $distDir "CustomerFD-Setup.exe"
    Copy-Replacing (Join-Path $relDir "cfd-installer.exe") $setupDst
    Copy-Replacing (Join-Path $relDir "cfd_shell.dll") (Join-Path $distDir "cfd_shell.dll")
    Copy-Replacing (Join-Path $relDir "cfd-manager.exe") (Join-Path $distDir "cfd-manager.exe")
    Remove-Item (Join-Path $distDir "uninstall.ps1") -ErrorAction SilentlyContinue

    # ---- Privacy gate -------------------------------------------------------------------------
    Write-Host "==> Scanning release binaries for embedded build-machine paths..." -ForegroundColor Cyan
    $needles = @()
    if ($env:USERPROFILE) { $needles += $env:USERPROFILE }
    if ($env:USERNAME -and $env:USERNAME.Length -ge 3) { $needles += "\Users\$($env:USERNAME)\" }
    $needles += $Root
    $bins = @(
        (Join-Path $relDir "cfd_shell.dll"),
        (Join-Path $relDir "cfd-manager.exe"),
        $setupDst
    ) | Where-Object { Test-Path $_ }
    $leaks = @()
    foreach ($bin in $bins) {
        $bytes = [System.IO.File]::ReadAllBytes($bin)
        $ascii = [System.Text.Encoding]::GetEncoding(28591).GetString($bytes)
        $utf16 = [System.Text.Encoding]::Unicode.GetString($bytes)
        foreach ($n in $needles) {
            if ($ascii.IndexOf($n, [StringComparison]::OrdinalIgnoreCase) -ge 0 -or
                $utf16.IndexOf($n, [StringComparison]::OrdinalIgnoreCase) -ge 0) {
                $leaks += "$(Split-Path -Leaf $bin): contains a build-machine path"
            }
        }
    }
    if ($leaks.Count -gt 0) {
        $leaks | Sort-Object -Unique | ForEach-Object { Write-Host "    $_" -ForegroundColor Red }
        throw "Privacy gate failed: release binaries embed build-machine paths."
    }

    Write-Host "`n==> Build artifacts:" -ForegroundColor Green
    Get-Item (Join-Path $distDir "*") -Exclude "*.old" | Select-Object Name, Length, LastWriteTime | Format-Table -AutoSize
}
finally {
    $env:RUSTFLAGS = $savedRustFlags
    Pop-Location
}
