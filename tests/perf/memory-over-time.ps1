# How WebView2's memory develops after scrolling (Phase 2 investigation, ADR 0002):
# starts Folio in FOLIO_PERF mode on a document, waits for the scripted scroll tests, then
# measures the process tree several times: right after, after a minute idle, and after
# minimizing the window (WebView2 lowers its memory use for hidden windows).
#
#   powershell -ExecutionPolicy Bypass -File tests/perf/memory-over-time.ps1 -Pdf <file.pdf>
param(
    [Parameter(Mandatory = $true)][string]$Pdf,
    # Skip the image format comparison that precedes the scroll tests.
    [switch]$ScrollOnly,
    [string]$Exe = '',
    [int]$IdleSeconds = 60
)
$ErrorActionPreference = 'Stop'
if (-not $Exe) { $Exe = Join-Path $PSScriptRoot '..\..\target\release\folio.exe' }
$Exe = (Resolve-Path $Exe).Path
$Pdf = (Resolve-Path $Pdf).Path
$out = Join-Path $PSScriptRoot '..\..\target\test-output\perf'
New-Item -ItemType Directory -Force $out | Out-Null

Add-Type -Namespace Folio -Name Win32 -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool ShowWindow(System.IntPtr hWnd, int nCmdShow);
'@

function Get-Descendants([int]$ParentId) {
    $children = Get-CimInstance Win32_Process -Filter "ParentProcessId = $ParentId"
    foreach ($c in $children) { $c; Get-Descendants $c.ProcessId }
}

function Show-Memory([string]$When, $Process) {
    $Process.Refresh()
    $byType = [ordered]@{}
    $total = $Process.WorkingSet64
    foreach ($c in @(Get-Descendants $Process.Id)) {
        $type = if ($c.CommandLine -match '--type=([\w-]+)') { $Matches[1] } else { 'browser' }
        $proc = Get-Process -Id $c.ProcessId -ErrorAction SilentlyContinue
        if ($proc) {
            $byType[$type] = [int]$byType[$type] + [math]::Round($proc.WorkingSet64 / 1MB)
            $total += $proc.WorkingSet64
        }
    }
    $types = ($byType.GetEnumerator() | ForEach-Object { "$($_.Key)=$($_.Value)" }) -join ' '
    '{0,-28} folio={1,4} tree={2,5}  {3}' -f $When, [math]::Round($Process.WorkingSet64 / 1MB), [math]::Round($total / 1MB), $types
}

Get-Process folio -ErrorAction SilentlyContinue | Stop-Process -Force
$copy = Join-Path $out (Split-Path $Pdf -Leaf)
Copy-Item $Pdf $copy -Force
$log = Join-Path $out 'memory-over-time.log'
$env:FOLIO_EPHEMERAL = '1'
$env:FOLIO_PERF = if ($ScrollOnly) { 'scroll' } else { '1' }
$p = Start-Process -FilePath $Exe -ArgumentList "`"$copy`"" -PassThru -RedirectStandardOutput $log
Remove-Item Env:FOLIO_PERF
$sw = [Diagnostics.Stopwatch]::StartNew()
while (-not (Select-String -Path $log -Pattern 'first_page_visible_ms' -Quiet -ErrorAction SilentlyContinue)) {
    if ($sw.Elapsed.TotalSeconds -gt 60) { throw 'no first page' }
    Start-Sleep -Milliseconds 50
}
Start-Sleep -Milliseconds 800
Show-Memory 'document open (perf idle)' $p
while (-not (Select-String -Path $log -Pattern 'perf_done' -Quiet -ErrorAction SilentlyContinue)) {
    if ($sw.Elapsed.TotalSeconds -gt 300) { throw 'perf run did not finish' }
    Start-Sleep -Milliseconds 200
}
Start-Sleep -Seconds 5
Show-Memory 'after scrolling, +5 s' $p
Start-Sleep -Seconds $IdleSeconds
Show-Memory "after scrolling, +$($IdleSeconds + 5) s" $p
[Folio.Win32]::ShowWindow($p.MainWindowHandle, 6) | Out-Null   # SW_MINIMIZE
Start-Sleep -Seconds 10
Show-Memory 'minimized, +10 s' $p
[Folio.Win32]::ShowWindow($p.MainWindowHandle, 9) | Out-Null   # SW_RESTORE
Start-Sleep -Seconds 5
Show-Memory 'restored, +5 s' $p
Stop-Process -Id $p.Id -Force
