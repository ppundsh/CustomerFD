$ErrorActionPreference = "Stop"

& (Join-Path $PSScriptRoot "build.ps1")

$distDir = Join-Path $PSScriptRoot "dist"
$dllPath = Join-Path $distDir "cfd_shell.dll"
$exePath = Join-Path $distDir "cfd-manager.exe"

Write-Host "Registering cfd_shell.dll (per-user HKCU)..." -ForegroundColor Cyan
regsvr32.exe /s $dllPath

Write-Host "Launching CustomerFD Control Center..." -ForegroundColor Cyan
Start-Process $exePath
