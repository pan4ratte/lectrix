# Downloads the msedgedriver that matches the installed WebView2 runtime into
# target/e2e-tools (tauri-driver needs a driver of exactly the runtime's version).
#   powershell -ExecutionPolicy Bypass -File tests/e2e/fetch-edgedriver.ps1
$ErrorActionPreference = 'Stop'

$root = Resolve-Path (Join-Path $PSScriptRoot '../..')
$tools = Join-Path $root 'target/e2e-tools'
New-Item -ItemType Directory -Force $tools | Out-Null

# The Evergreen WebView2 runtime registers its version under EdgeUpdate (machine-wide or
# per-user installs).
$client = '{F3017226-FE2A-4295-8BDF-00C3A9A7E4C5}'
$keys = @(
    "HKLM:\SOFTWARE\WOW6432Node\Microsoft\EdgeUpdate\Clients\$client",
    "HKLM:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$client",
    "HKCU:\SOFTWARE\Microsoft\EdgeUpdate\Clients\$client"
)
$version = $null
foreach ($key in $keys) {
    $pv = (Get-ItemProperty -Path $key -Name pv -ErrorAction SilentlyContinue).pv
    if ($pv -and $pv -ne '0.0.0.0') { $version = $pv; break }
}
if (-not $version) { throw 'The WebView2 runtime is not installed.' }

$driver = Join-Path $tools 'msedgedriver.exe'
if (Test-Path $driver) {
    $current = & $driver --version
    if ($current -match [regex]::Escape($version)) {
        Write-Output "msedgedriver $version is already in $tools"
        exit 0
    }
}
$zip = Join-Path $tools 'edgedriver.zip'
Invoke-WebRequest -Uri "https://msedgedriver.microsoft.com/$version/edgedriver_win64.zip" -OutFile $zip
Expand-Archive -Path $zip -DestinationPath $tools -Force
Remove-Item $zip
Write-Output "msedgedriver $version installed in $tools"
