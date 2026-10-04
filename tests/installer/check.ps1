# Installs and uninstalls Lectrix's NSIS and MSI installers silently, with and without the
# PDF file registration, and checks the registry and files each time (ADR 0007).
#
# It really installs Lectrix, so it is meant for CI runners (MSI needs an elevated shell).
# Run it on your own machine only if you are happy for Lectrix to be installed and removed
# again. Needs the installers from `npx tauri build`.
#
#   powershell -ExecutionPolicy Bypass -File tests/installer/check.ps1

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$bundle = Join-Path $root 'target\release\bundle'
$nsis = Get-ChildItem (Join-Path $bundle 'nsis\*-setup.exe') | Select-Object -First 1
$msi = Get-ChildItem (Join-Path $bundle 'msi\*.msi') | Select-Object -First 1
if (-not $nsis -or -not $msi) { throw "installers not found in $bundle; run npx tauri build" }
$logs = Join-Path $root 'target\test-output\installer'
New-Item -ItemType Directory -Force $logs | Out-Null

$product = 'Lectrix'
# Tauri's default manufacturer: the middle part of the identifier org.lectrix.pdf.
$manufacturer = 'lectrix'
$progId = "$product.Document"
$failures = New-Object System.Collections.Generic.List[string]

function Check([bool]$ok, [string]$what) {
    if ($ok) { Write-Host "  ok    $what" } else { Write-Host "  FAIL  $what"; $failures.Add($what) }
}

function Get-Value([string]$key, [string]$name) {
    try { (Get-ItemProperty -LiteralPath $key -Name $name -ErrorAction Stop).$name } catch { $null }
}

function Has-Value([string]$key, [string]$name) {
    try { $null = Get-ItemProperty -LiteralPath $key -Name $name -ErrorAction Stop; $true } catch { $false }
}

# The registration both installers write, under HKCU (NSIS, per user) or HKLM (MSI).
function Check-Registration([string]$hive, [bool]$expected, [string]$exe) {
    $classes = "${hive}:\Software\Classes"
    $command = Get-Value "$classes\$progId\shell\open\command" '(default)'
    if ($expected) {
        Check ($command -eq "`"$exe`" `"%1`"") "$hive open command is `"$exe`" `"%1`" (found: $command)"
        Check (Has-Value "$classes\.pdf\OpenWithProgids" $progId) "$hive .pdf lists $progId under OpenWithProgids"
        Check ((Get-Value "${hive}:\Software\RegisteredApplications" $product) -eq "Software\$product\Capabilities") "$hive RegisteredApplications names Lectrix's capabilities"
        Check ((Get-Value "${hive}:\Software\$product\Capabilities\FileAssociations" '.pdf') -eq $progId) "$hive capabilities map .pdf to $progId"
    } else {
        Check ($null -eq $command) "$hive has no $progId"
        Check (-not (Has-Value "$classes\.pdf\OpenWithProgids" $progId)) "$hive .pdf does not list $progId"
        Check (-not (Has-Value "${hive}:\Software\RegisteredApplications" $product)) "$hive RegisteredApplications has no Lectrix"
        Check (-not (Test-Path "${hive}:\Software\$product\Capabilities")) "$hive has no Lectrix capabilities"
    }
}

function Run([string]$file, [string[]]$arguments) {
    $p = Start-Process -FilePath $file -ArgumentList $arguments -Wait -PassThru
    if ($p.ExitCode -ne 0) { throw "$file $($arguments -join ' ') exited with $($p.ExitCode)" }
}

# Lectrix must never take over the user's default PDF app (ADR 0007).
$pdfDefaultBefore = @(
    (Get-Value 'HKCU:\Software\Classes\.pdf' '(default)'),
    (Get-Value 'HKLM:\Software\Classes\.pdf' '(default)')
)

# --- NSIS (per user) -------------------------------------------------------------------
$nsisDir = Join-Path $env:LOCALAPPDATA $product
$nsisExe = Join-Path $nsisDir 'lectrix.exe'
foreach ($case in @(@{ Args = @('/S'); Registered = $true }, @{ Args = @('/S', '/NOPDF'); Registered = $false })) {
    Write-Host "NSIS $($nsis.Name) $($case.Args -join ' ')"
    Run $nsis.FullName $case.Args
    Check (Test-Path $nsisExe) "lectrix.exe installed in $nsisDir"
    Check (Test-Path (Join-Path $nsisDir 'THIRD_PARTY_LICENSES.md')) 'license notices installed'
    Check-Registration 'HKCU' $case.Registered $nsisExe
    # _?= runs the uninstaller in place, so Start-Process can wait for it.
    Run (Join-Path $nsisDir 'uninstall.exe') @('/S', "_?=$nsisDir")
    Check (-not (Test-Path $nsisExe)) 'lectrix.exe removed'
    Check-Registration 'HKCU' $false $nsisExe
    # The MSI would otherwise read this and install into the per-user folder.
    Check ($null -eq (Get-Value "HKCU:\Software\$manufacturer\$product" '(default)')) 'install location forgotten'
    Remove-Item -Recurse -Force $nsisDir -ErrorAction SilentlyContinue
}

# --- MSI (per machine) -----------------------------------------------------------------
$msiExe = Join-Path $env:ProgramFiles "$product\lectrix.exe"
foreach ($case in @(@{ Props = @(); Registered = $true; Log = 'msi-default' }, @{ Props = @('LECTRIX_ASSOCIATE_PDF=0'); Registered = $false; Log = 'msi-no-pdf' })) {
    Write-Host "MSI $($msi.Name) $($case.Props -join ' ')"
    Run 'msiexec.exe' (@('/i', "`"$($msi.FullName)`"", '/qn', '/norestart', '/l*v', "`"$logs\$($case.Log)-install.log`"") + $case.Props)
    Check (Test-Path $msiExe) "lectrix.exe installed in $(Split-Path $msiExe)"
    Check-Registration 'HKLM' $case.Registered $msiExe
    Run 'msiexec.exe' @('/x', "`"$($msi.FullName)`"", '/qn', '/norestart', '/l*v', "`"$logs\$($case.Log)-uninstall.log`"")
    Check (-not (Test-Path $msiExe)) 'lectrix.exe removed'
    Check-Registration 'HKLM' $false $msiExe
}

$pdfDefaultAfter = @(
    (Get-Value 'HKCU:\Software\Classes\.pdf' '(default)'),
    (Get-Value 'HKLM:\Software\Classes\.pdf' '(default)')
)
Check (($pdfDefaultBefore -join '|') -eq ($pdfDefaultAfter -join '|')) "the default app for .pdf is untouched ($($pdfDefaultAfter -join ', '))"

if ($failures.Count -gt 0) {
    Write-Host "$($failures.Count) installer check(s) failed"
    exit 1
}
Write-Host 'All installer checks passed'
