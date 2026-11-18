<#
.SYNOPSIS
    Developer cleanup: removes every CustomerFD registry entry (all 64 reserved CLSIDs) and the
    settings folder. End users should uninstall from Settings > Apps > Installed apps instead.
#>
$ErrorActionPreference = "SilentlyContinue"

Write-Host "Cleaning up CustomerFD registry entries and settings..." -ForegroundColor Cyan

# Clean up all reserved CustomerFD CLSIDs (base + up to 64 root folders)
$baseBytes = [guid]::Parse("9F3B4A10-7C2E-4D8F-A512-6E8C3B9D1042")
for ($i = 0; $i -lt 64; $i++) {
    $suffix = (0x1042 + $i).ToString("X4")
    $clsid = "{9F3B4A10-7C2E-4D8F-A512-6E8C3B9D$suffix}"

    $nsKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Desktop\NameSpace\$clsid"
    if (Test-Path $nsKey) {
        Remove-Item -Path $nsKey -Recurse -Force
        Write-Host "Removed $nsKey"
    }

    $clsidKey = "HKCU:\Software\Classes\CLSID\$clsid"
    if (Test-Path $clsidKey) {
        Remove-Item -Path $clsidKey -Recurse -Force
        Write-Host "Removed $clsidKey"
    }

    $hideKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\HideDesktopIcons\NewStartPanel"
    if (Test-Path $hideKey) {
        Remove-ItemProperty -Path $hideKey -Name $clsid -ErrorAction SilentlyContinue
    }
}

$cfgDir = Join-Path $env:LOCALAPPDATA "CustomerFD"
if (Test-Path $cfgDir) {
    Remove-Item -Path $cfgDir -Recurse -Force
    Write-Host "Removed config directory: $cfgDir"
}

Write-Host "CustomerFD cleanup complete! You may restart Explorer if needed." -ForegroundColor Green
