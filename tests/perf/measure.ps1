# Measures the AGENTS.md section 2 performance targets for a release build.
#
#   powershell -ExecutionPolicy Bypass -File tests/perf/measure.ps1 -Pdf <file.pdf> [-Runs 3] [-Perf]
#
# Each run starts Folio with the file and reports: time from launch to a visible main
# window, the app's own metrics (main_to_ready_ms, first_page_visible_ms), and idle memory
# with the document open (folio.exe alone, and including its WebView2 processes; ADR 0002).
#
# -Perf adds one run with FOLIO_PERF set: the app compares raw RGBA with PNG end to end,
# scrolls the whole document at two steady speeds measuring how long pages stay blank
# (section 2: no blank page for more than 200 ms), and reports memory after scrolling.
#
# FOLIO_EPHEMERAL keeps these runs out of the user's recent files and remembered views.
param(
    [Parameter(Mandatory = $true)][string]$Pdf,
    [string]$Exe = '',
    [int]$Runs = 3,
    [int]$IdleSeconds = 5,
    [switch]$Perf,
    # Page image format for every run: png (the default, ADR 0004) or rgba.
    [ValidateSet('rgba', 'png')][string]$Format = 'png'
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

function Read-Metrics([string]$Log) {
    $metrics = [ordered]@{}
    Get-Content $Log | ForEach-Object {
        if ($_ -match '^\[folio-metric\] (\w+)=([\d.]+)') { $metrics[$Matches[1]] = [double]$Matches[2] }
    }
    $metrics
}

function Measure-Memory($Process) {
    $Process.Refresh()
    $children = @(Get-Descendants $Process.Id)
    $all = @($children | ForEach-Object { Get-Process -Id $_.ProcessId -ErrorAction SilentlyContinue })
    # WebView2 working set by process type (browser, renderer, gpu-process, utility).
    $byType = [ordered]@{}
    foreach ($c in $children) {
        $type = if ($c.CommandLine -match '--type=([\w-]+)') { $Matches[1] } else { 'browser' }
        $proc = Get-Process -Id $c.ProcessId -ErrorAction SilentlyContinue
        if ($proc) { $byType[$type] = [int]$byType[$type] + [math]::Round($proc.WorkingSet64 / 1MB) }
    }
    [pscustomobject]@{
        Own     = [math]::Round($Process.WorkingSet64 / 1MB)
        Private = [math]::Round($Process.PrivateMemorySize64 / 1MB)
        Tree    = [math]::Round(($Process.WorkingSet64 + ($all | Measure-Object -Property WorkingSet64 -Sum).Sum) / 1MB)
        ByType  = ($byType.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ' '
    }
}

function Start-Folio([string]$Log, [int]$Run) {
    # A fresh copy per run: the file name is the same, but nothing is cached anywhere.
    $dir = Join-Path $out "run-$Run"
    New-Item -ItemType Directory -Force $dir | Out-Null
    $copy = Join-Path $dir (Split-Path $Pdf -Leaf)
    Copy-Item $Pdf $copy -Force
    $env:FOLIO_EPHEMERAL = '1'
    $env:FOLIO_IMAGE_FORMAT = $Format
    Start-Process -FilePath $Exe -ArgumentList "`"$copy`"" -PassThru -RedirectStandardOutput $Log
}

function Wait-For([string]$Log, [string]$Pattern, [int]$Seconds, $Stopwatch) {
    while (-not (Select-String -Path $Log -Pattern $Pattern -Quiet -ErrorAction SilentlyContinue)) {
        if ($Stopwatch.Elapsed.TotalSeconds -gt $Seconds) { throw "no '$Pattern' within $Seconds s" }
        Start-Sleep -Milliseconds 50
    }
}

Get-Process folio -ErrorAction SilentlyContinue | Stop-Process -Force

$results = @()
for ($i = 1; $i -le $Runs; $i++) {
    $log = Join-Path $out "run-$i.log"
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Folio $log $i
    while ($true) {
        $p.Refresh()
        if ($p.MainWindowHandle -ne 0) { break }
        if ($sw.Elapsed.TotalSeconds -gt 30) { throw 'window did not appear within 30 s' }
        Start-Sleep -Milliseconds 5
    }
    $windowMs = $sw.Elapsed.TotalMilliseconds
    Wait-For $log 'first_page_visible_ms' 60 $sw
    Start-Sleep -Seconds $IdleSeconds
    $mem = Measure-Memory $p
    Stop-Process -Id $p.Id -Force
    Start-Sleep -Milliseconds 500

    $metrics = Read-Metrics $log
    $results += [pscustomobject]@{
        Run                      = $i
        WindowVisibleMs          = [math]::Round($windowMs)
        MainToReadyMs            = $metrics['main_to_ready_ms']
        FirstPageVisibleMs       = $metrics['first_page_visible_ms']
        FolioWorkingSetMB        = $mem.Own
        FolioPrivateMB           = $mem.Private
        WithWebView2WorkingSetMB = $mem.Tree
        WebView2ByTypeMB         = $mem.ByType
    }
}
$results | Format-Table -AutoSize

if ($Perf) {
    $log = Join-Path $out 'perf.log'
    $env:FOLIO_PERF = '1'
    $sw = [Diagnostics.Stopwatch]::StartNew()
    $p = Start-Folio $log 0
    Remove-Item Env:FOLIO_PERF
    Wait-For $log 'perf_done' 300 $sw
    Start-Sleep -Seconds $IdleSeconds
    $mem = Measure-Memory $p
    Stop-Process -Id $p.Id -Force
    $metrics = Read-Metrics $log
    $metrics['memory_after_scroll_folio_mb'] = $mem.Own
    $metrics['memory_after_scroll_tree_mb'] = $mem.Tree
    $metrics['memory_after_scroll_webview2_by_type'] = $mem.ByType
    [pscustomobject]$metrics | Format-List
}
