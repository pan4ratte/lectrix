# Measures the AGENTS.md section 2 performance targets for a release build.
#
#   powershell -ExecutionPolicy Bypass -File tests/perf/measure.ps1 -Pdf <file.pdf> [-Runs 3]
#
# Reports: time from launch to a visible main window, the app's own metrics
# (main_to_ready_ms, first_page_visible_ms), and idle memory with the document open
# (folio.exe alone, and including its WebView2 processes).
param(
    [Parameter(Mandatory = $true)][string]$Pdf,
    [string]$Exe = '',
    [int]$Runs = 3,
    [int]$IdleSeconds = 5
)

$ErrorActionPreference = 'Stop'
# $PSScriptRoot is not available in parameter defaults on Windows PowerShell 5.1.
if (-not $Exe) { $Exe = Join-Path $PSScriptRoot '..\..\target\release\folio.exe' }
$Exe = (Resolve-Path $Exe).Path
$Pdf = (Resolve-Path $Pdf).Path
$out = Join-Path $PSScriptRoot '..\..\target\test-output\perf'
New-Item -ItemType Directory -Force $out | Out-Null

function Get-Descendants([int]$ParentId) {
    $children = Get-CimInstance Win32_Process -Filter "ParentProcessId = $ParentId"
    foreach ($c in $children) { $c; Get-Descendants $c.ProcessId }
}

$results = @()
for ($i = 1; $i -le $Runs; $i++) {
    $log = Join-Path $out "run-$i.log"
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Process -FilePath $Exe -ArgumentList "`"$Pdf`"" -PassThru -RedirectStandardOutput $log
    while ($true) {
        $p.Refresh()
        if ($p.MainWindowHandle -ne 0) { break }
        if ($sw.Elapsed.TotalSeconds -gt 30) { throw 'window did not appear within 30 s' }
        Start-Sleep -Milliseconds 5
    }
    $windowMs = $sw.Elapsed.TotalMilliseconds

    # Wait for the first page metric, then let the app go idle.
    while (-not (Select-String -Path $log -Pattern 'first_page_visible_ms' -Quiet -ErrorAction SilentlyContinue)) {
        if ($sw.Elapsed.TotalSeconds -gt 60) { throw 'first page did not appear within 60 s' }
        Start-Sleep -Milliseconds 50
    }
    Start-Sleep -Seconds $IdleSeconds

    $p.Refresh()
    $own = $p.WorkingSet64
    $ownPrivate = $p.PrivateMemorySize64
    $all = @(Get-Descendants $p.Id | ForEach-Object { Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue })
    $tree = $own + ($all | Measure-Object -Property WorkingSet64 -Sum).Sum
    Stop-Process -Id $p.Id -Force
    Start-Sleep -Milliseconds 500

    $metrics = @{}
    Get-Content $log | ForEach-Object {
        if ($_ -match '^\[folio-metric\] (\w+)=([\d.]+)') { $metrics[$Matches[1]] = [double]$Matches[2] }
    }
    $results += [pscustomobject]@{
        Run                    = $i
        WindowVisibleMs        = [math]::Round($windowMs)
        MainToReadyMs          = $metrics['main_to_ready_ms']
        FirstPageVisibleMs     = $metrics['first_page_visible_ms']
        FolioWorkingSetMB      = [math]::Round($own / 1MB)
        FolioPrivateMB         = [math]::Round($ownPrivate / 1MB)
        WithWebView2WorkingSetMB = [math]::Round($tree / 1MB)
    }
}
$results | Format-Table -AutoSize
