$ErrorActionPreference = "Stop"

Write-Host "Building CustomerFD release binaries..." -ForegroundColor Cyan
cargo build --release --workspace
if ($LASTEXITCODE -ne 0) { exit $LASTEXITCODE }

$distDir = Join-Path $PSScriptRoot "dist"
New-Item -ItemType Directory -Force -Path $distDir | Out-Null

Copy-Item (Join-Path $PSScriptRoot "target\release\cfd_shell.dll") $distDir -Force
Copy-Item (Join-Path $PSScriptRoot "target\release\cfd-manager.exe") $distDir -Force
Copy-Item (Join-Path $PSScriptRoot "uninstall.ps1") $distDir -Force

Write-Host "Build complete! Artifacts in $distDir" -ForegroundColor Green
