# Why can WebDriver not attach? Starts the release app directly (no driver) with a sample
# document and reports what happens: whether it stays up, makes a window, starts WebView2,
# and what Lectrix's own log says. CI runs this when the E2E tests fail.
#   powershell -ExecutionPolicy Bypass -File tests/e2e/diagnose.ps1
$ErrorActionPreference = 'Continue'
$root = Resolve-Path (Join-Path $PSScriptRoot '../..')
$exe = Join-Path $root 'target/release/lectrix.exe'
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

Get-Process lectrix -ErrorAction SilentlyContinue | Stop-Process -Force
$sample = Join-Path $out 'diagnose.pdf'
$cli = @('release', 'debug') | ForEach-Object { Join-Path $root "target/$_/pdf-cli.exe" } | Where-Object { Test-Path $_ } | Select-Object -First 1
if ($cli) { & $cli gen $sample --pages 2 | Out-Null }
$env:LECTRIX_EPHEMERAL = '1'
$env:LECTRIX_OPEN = $sample
$stdout = Join-Path $out 'diagnose-stdout.log'
$stderr = Join-Path $out 'diagnose-stderr.log'
$p = Start-Process -FilePath $exe -PassThru -RedirectStandardOutput $stdout -RedirectStandardError $stderr
for ($i = 0; $i -lt 15; $i++) {
    Start-Sleep -Seconds 1
    $p.Refresh()
    if ($p.HasExited) { break }
}
if ($p.HasExited) {
    Write-Output "lectrix.exe exited after about $i s with code $($p.ExitCode)"
} else {
    Write-Output "lectrix.exe is running; main window handle: $($p.MainWindowHandle)"
    $children = Get-CimInstance Win32_Process -Filter "ParentProcessId = $($p.Id)"
    Write-Output "child processes: $(($children | ForEach-Object { $_.Name }) -join ', ')"
    Stop-Process -Id $p.Id -Force
}
Write-Output '--- stdout'; Get-Content $stdout -ErrorAction SilentlyContinue | Select-Object -Last 40
Write-Output '--- stderr'; Get-Content $stderr -ErrorAction SilentlyContinue | Select-Object -Last 40
$logs = Join-Path $env:LOCALAPPDATA 'org.lectrix.pdf\logs'
Write-Output "--- app log ($logs)"
Get-ChildItem $logs -ErrorAction SilentlyContinue | Sort-Object LastWriteTime | Select-Object -Last 1 |
    ForEach-Object { Get-Content $_.FullName | Select-Object -Last 40 }

# A WebDriver session straight against msedgedriver, with its verbose log: which browser
# arguments reach WebView2, and whether the DevTools port file appears.
Write-Output '--- WebDriver session (msedgedriver --verbose)'
Get-Process lectrix -ErrorAction SilentlyContinue | Stop-Process -Force
Remove-Item Env:LECTRIX_OPEN
$driverLog = Join-Path $out 'msedgedriver.log'
$driverExe = Join-Path $root 'target/e2e-tools/msedgedriver.exe'
$driver = Start-Process $driverExe -ArgumentList '--port=9515', '--verbose', "--log-path=$driverLog" -PassThru -WindowStyle Hidden
Start-Sleep -Seconds 2
$body = @{ capabilities = @{ alwaysMatch = @{ browserName = 'webview2'; 'ms:edgeOptions' = @{
    binary = $exe; args = @(); webviewOptions = @{} } } } } | ConvertTo-Json -Depth 6
$job = Start-Job -ScriptBlock {
    param($b)
    try {
        $r = Invoke-RestMethod -Method Post -Uri http://127.0.0.1:9515/session -Body $b -ContentType 'application/json' -TimeoutSec 90
        "session created: $($r.value.sessionId)"
    } catch { "session failed: $($_.Exception.Message)" }
} -ArgumentList $body
Start-Sleep -Seconds 15
Write-Output 'WebView2 processes while the session is pending:'
Get-CimInstance Win32_Process -Filter "Name = 'msedgewebview2.exe'" | Where-Object { $_.CommandLine -notmatch '--type=' } |
    ForEach-Object { "  pid $($_.ProcessId): $($_.CommandLine)" }
$data = Join-Path $env:LOCALAPPDATA 'org.lectrix.pdf\EBWebView'
Write-Output "DevToolsActivePort files under $data :"
Get-ChildItem $data -Recurse -Filter DevToolsActivePort -ErrorAction SilentlyContinue | ForEach-Object { "  $($_.FullName)" }
Receive-Job $job -Wait
Stop-Process -Id $driver.Id -Force -ErrorAction SilentlyContinue
Get-Process lectrix -ErrorAction SilentlyContinue | Stop-Process -Force
Write-Output '--- msedgedriver log (selected lines)'
Select-String -Path $driverLog -Pattern 'Launching|WEBVIEW2_|DevToolsActivePort|ERROR|SEVERE|WARNING|user data|Failed' |
    ForEach-Object { $_.Line.Substring(0, [Math]::Min(600, $_.Line.Length)) } | Select-Object -First 40
