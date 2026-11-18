#Requires -Version 5.1
<#
.SYNOPSIS
    Development deploy: build, install silently for the current user, open the control center.
.DESCRIPTION
    Installs to %LOCALAPPDATA%\Programs\CustomerFD exactly like the published installer
    (per user, no UAC). Settings in %LOCALAPPDATA%\CustomerFD are kept.
#>
$ErrorActionPreference = 'Stop'
$Root = Split-Path -Parent $MyInvocation.MyCommand.Path

& (Join-Path $Root "build.ps1")

$setup = Join-Path $Root "dist\CustomerFD-Setup.exe"
Write-Host "==> Installing for the current user..." -ForegroundColor Cyan
$p = Start-Process $setup -ArgumentList "--quiet" -Wait -PassThru
if ($p.ExitCode -ne 0) { throw "Installer failed ($($p.ExitCode))" }

$exe = Join-Path $env:LOCALAPPDATA "Programs\CustomerFD\cfd-manager.exe"
Write-Host "==> Launching $exe" -ForegroundColor Cyan
Start-Process $exe
