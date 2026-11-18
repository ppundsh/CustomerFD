# Local helper: runs the workspace checks with a live progress bar; full output -> checks.log.
$ErrorActionPreference = 'Continue'
Set-Location -LiteralPath $PSScriptRoot

$log = Join-Path $PSScriptRoot 'checks.log'
$stateDir = Join-Path $PSScriptRoot 'target'
$stateFile = Join-Path $stateDir 'run-checks-progress.json'

$steps = @(
    @{ Key = 'fmt';    Name = 'cargo fmt --check';          Cmd = 'cargo fmt --all -- --check'; Weight = 5; Lines = 5 },
    @{ Key = 'clippy'; Name = 'cargo clippy --workspace';   Cmd = 'cargo clippy --workspace --all-targets -- -D warnings'; Weight = 25; Lines = 40 },
    @{ Key = 'test';   Name = 'cargo test --workspace';     Cmd = 'cargo test --workspace -- --test-threads=1'; Weight = 25; Lines = 60 },
    @{ Key = 'build';  Name = 'build.ps1';                  Cmd = 'powershell -NoProfile -ExecutionPolicy Bypass -File build.ps1'; Weight = 45; Lines = 120 }
)

# Line counts of the previous run make the in-step percentage track real progress.
if (Test-Path -LiteralPath $stateFile) {
    try {
        $prev = Get-Content -LiteralPath $stateFile -Raw | ConvertFrom-Json
        foreach ($s in $steps) {
            $v = $prev.($s.Key)
            if ($v -and [int]$v -gt 0) { $s.Lines = [int]$v }
        }
    } catch { }
}

function Read-Shared([string]$path) {
    try {
        $fs = [IO.File]::Open($path, 'Open', 'Read', 'ReadWrite')
        try {
            $sr = New-Object IO.StreamReader($fs, [Text.Encoding]::UTF8)
            return $sr.ReadToEnd()
        } finally { $fs.Dispose() }
    } catch { return '' }
}

$utf8 = New-Object Text.UTF8Encoding($false)
[IO.File]::WriteAllText($log, '', $utf8)
$totalWeight = ($steps | ForEach-Object { $_.Weight } | Measure-Object -Sum).Sum
$doneWeight = 0
$counts = @{}
$results = @()
$sw = [Diagnostics.Stopwatch]::StartNew()

Write-Host 'Running checks (full output -> checks.log) ...'
for ($i = 0; $i -lt $steps.Count; $i++) {
    $s = $steps[$i]
    $tag = "[$($i + 1)/$($steps.Count)]"
    [IO.File]::AppendAllText($log, "==== $tag $($s.Name) ====`r`n", $utf8)
    $tmp = [IO.Path]::GetTempFileName()

    $psi = New-Object Diagnostics.ProcessStartInfo
    $psi.FileName = $env:ComSpec
    $psi.Arguments = "/d /c $($s.Cmd) > `"$tmp`" 2>&1"
    $psi.WorkingDirectory = $PSScriptRoot
    $psi.UseShellExecute = $false
    $psi.CreateNoWindow = $true
    $proc = [Diagnostics.Process]::Start($psi)
    $stepSw = [Diagnostics.Stopwatch]::StartNew()

    $lines = 0
    while (-not $proc.WaitForExit(250)) {
        $text = Read-Shared $tmp
        $all = @($text -split "`r?`n" | Where-Object { $_.Trim() -ne '' })
        $lines = $all.Count
        $last = if ($lines -gt 0) { $all[-1].Trim() } else { 'starting...' }
        if ($last.Length -gt 70) { $last = $last.Substring(0, 67) + '...' }
        $frac = [Math]::Min(0.97, $lines / [double][Math]::Max(1, $s.Lines))
        $pct = [int](100 * ($doneWeight + $s.Weight * $frac) / $totalWeight)
        $elapsed = '{0:mm\:ss}' -f $stepSw.Elapsed
        Write-Progress -Activity "CustomerFD checks  $tag $($s.Name)  ($elapsed)" -Status "$pct%  $last" -PercentComplete $pct
    }
    $proc.WaitForExit()
    $code = $proc.ExitCode

    $text = Read-Shared $tmp
    Remove-Item -LiteralPath $tmp -Force -ErrorAction SilentlyContinue
    $counts[$s.Key] = @($text -split "`r?`n" | Where-Object { $_.Trim() -ne '' }).Count
    [IO.File]::AppendAllText($log, $text, $utf8)
    [IO.File]::AppendAllText($log, "EXIT $code`r`n", $utf8)

    $doneWeight += $s.Weight
    $dur = '{0:mm\:ss}' -f $stepSw.Elapsed
    if ($code -eq 0) {
        Write-Host ("  [ OK ] {0} {1}  ({2})" -f $tag, $s.Name, $dur) -ForegroundColor Green
    } else {
        Write-Host ("  [FAIL] {0} {1}  (exit {2}, {3})" -f $tag, $s.Name, $code, $dur) -ForegroundColor Red
    }
    $results += $code
}

Write-Progress -Activity 'CustomerFD checks' -Completed
[IO.File]::AppendAllText($log, "ALL DONE`r`n", $utf8)

try {
    if (-not (Test-Path -LiteralPath $stateDir)) { New-Item -ItemType Directory -Path $stateDir | Out-Null }
    ($counts | ConvertTo-Json) | Set-Content -LiteralPath $stateFile -Encoding UTF8
} catch { }

$failed = @($results | Where-Object { $_ -ne 0 }).Count
$total = '{0:mm\:ss}' -f $sw.Elapsed
if ($failed -eq 0) {
    Write-Host "All checks passed in $total. Send checks.log if asked." -ForegroundColor Green
} else {
    Write-Host "$failed step(s) failed (total $total). See checks.log." -ForegroundColor Red
}
