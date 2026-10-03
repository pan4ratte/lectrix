# Why can WebDriver not attach? Starts the release app directly (no driver) with a sample
# document and reports what happens: whether it stays up, makes a window, starts WebView2,
# and what Folio's own log says. CI runs this when the E2E tests fail.
#   powershell -ExecutionPolicy Bypass -File tests/e2e/diagnose.ps1
$ErrorActionPreference = 'Continue'
$root = Resolve-Path (Join-Path $PSScriptRoot '../..')
$exe = Join-Path $root 'target/release/folio.exe'
$out = Join-Path $root 'target/test-output/e2e'
New-Item -ItemType Directory -Force $out | Out-Null

Write-Output "OS: $((Get-CimInstance Win32_OperatingSystem).Caption) $([Environment]::OSVersion.Version)"
$client = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
foreach ($key in "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$client",
                 "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$client") {
    $pv = (Get-ItemProperty -Path $key -Name pv -ErrorAction SilentlyContinue).pv
    if ($pv) { Write-Output "WebView2 runtime ($key): $pv" }
}
Write-Output "Session: $([Environment]::UserInteractive) interactive, session id $((Get-Process -Id $PID).SessionId)"

Get-Process folio -ErrorAction SilentlyContinue | Stop-Process -Force
$sample = Join-Path $out 'diagnose.pdf'
$cli = @('release', 'debug') | ForEach-Object { Join-Path $root "target/$_/pdf-cli.exe" } | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($cli) { & $cli gen $sample --pages 2 | Out-Null }
$env:FOLIO_EPHEMERAL = '1'
$env:FOLIO_OPEN = $sample
$stdout = Join-Path $out 'diagnose-stdout.log'
$stderr = Join-Path $out 'diagnose-stderr.log'
$p = Start-Process -FilePath $exe -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
for ($i = 0; $i -lt 15; $i++) {
    Start-Sleep -Seconds 1
    $p.Refresh()
    if ($p.HasExited) { break }
}
if ($p.HasExited) {
    Write-Output "folio.exe exited after about $i s with code $($p.ExitCode)"
} else {
    Write-Output "folio.exe is running; main window handle: $($p.MainWindowHandle)"
    $children = Get-CimInstance Win32_Process -Filter "ParentProcessId = $($p.Id)"
    Write-Output "child processes: $(($children | ForEach-Object { $_.Name }) -join ', ')"
    Stop-Process -Id $p.Id -Force
}
Write-Output '--- stdout'; Get-Content $stdout -ErrorAction SilentlyContinue | Select-Object -Last 40
Write-Output '--- stderr'; Get-Content $stderr -ErrorAction SilentlyContinue | Select-Object -Last 40
$logs = Join-Path $env:LOCALAPPDATA 'org.folio.pdf\logs'
Write-Output "--- app log ($logs)"
Get-ChildItem $logs -ErrorAction SilentlyContinue | Sort-Object LastWriteTime | Select-Object -Last 1 |
    ForEach-Object { Get-Content $_.FullName | Select-Object -Last 40 }
