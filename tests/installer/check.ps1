# Installs and uninstalls Lectrix's NSIS and MSI installers silently, with and without the
# PDF file registration, per user and per machine, and checks the registry and files each
# time (ADR 0007, ADR 0015).
#
# It really installs Lectrix, so it is meant for CI runners (it needs an elevated shell).
# Run it on your own machine only if you are happy for Lectrix to be installed and removed
# again. Needs the installers from `npm run bundle`.
#
#   powershell -ExecutionPolicy Bypass -File tests/installer/check.ps1

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..\..')
$bundle = Join-Path $root 'target\release\bundle'
$nsis = Get-ChildItem (Join-Path $bundle 'nsis\*-setup.exe') | Select-Object -First 1
$msi = Get-ChildItem (Join-Path $bundle 'msi\*.msi') | Select-Object -First 1
if (-not $nsis -or -not $msi) { throw "installers not found in $bundle; run npm run bundle" }
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

# The registration both installers write, under HKCU (per user) or HKLM (per machine).
function Check-Registration([string]$hive, [bool]$expected, [string]$exe) {
    $classes = "${hive}:\Software\Classes"
    $command = Get-Value "$classes\$progId\shell\open\command" '(default)'
    $icon = Join-Path (Split-Path $exe) 'lectrix-pdf.ico'
    if ($expected) {
        Check ($command -eq "`"$exe`" `"%1`"") "$hive open command is `"$exe`" `"%1`" (found: $command)"
        $defaultIcon = Get-Value "$classes\$progId\DefaultIcon" '(default)'
        Check ($defaultIcon -eq "`"$icon`",0") "$hive DefaultIcon is `"$icon`",0 (found: $defaultIcon)"
        Check (Test-Path -LiteralPath $icon) "the PDF icon $icon exists"
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

# --- NSIS (per user or per machine, ADR 0015) ------------------------------------------
# An administrator's silent install is per machine unless /CurrentUser is given.
$userDir = Join-Path $env:LOCALAPPDATA "Programs\$product"
$machineDir = Join-Path $env:ProgramFiles $product
$uninstKey = "Software\Microsoft\Windows\CurrentVersion\Uninstall\$product"
$nsisCases = @(
    @{ Args = @('/S', '/CurrentUser'); Dir = $userDir; Hive = 'HKCU'; Registered = $true },
    @{ Args = @('/S', '/CurrentUser', '/NOPDF'); Dir = $userDir; Hive = 'HKCU'; Registered = $false },
    @{ Args = @('/S'); Dir = $machineDir; Hive = 'HKLM'; Registered = $true },
    @{ Args = @('/S', '/AllUsers', '/NOPDF'); Dir = $machineDir; Hive = 'HKLM'; Registered = $false }
)

function Check-Nsis([hashtable]$case, [string]$when) {
    $exe = Join-Path $case.Dir 'lectrix.exe'
    $other = if ($case.Dir -eq $userDir) { $machineDir } else { $userDir }
    Check (Test-Path $exe) "lectrix.exe installed in $($case.Dir)$when"
    Check (-not (Test-Path (Join-Path $other 'lectrix.exe'))) "no copy in $other"
    Check (Test-Path (Join-Path $case.Dir 'THIRD_PARTY_LICENSES.md')) 'license notices installed'
    Check-Registration $case.Hive $case.Registered $exe
}

function Uninstall-Nsis([hashtable]$case) {
    $exe = Join-Path $case.Dir 'lectrix.exe'
    # _?= runs the uninstaller in place, so Start-Process can wait for it.
    Run (Join-Path $case.Dir 'uninstall.exe') @('/S', "_?=$($case.Dir)")
    Check (-not (Test-Path $exe)) 'lectrix.exe removed'
    Check (-not (Test-Path (Join-Path $case.Dir 'lectrix-pdf.ico'))) 'PDF icon removed'
    Check-Registration $case.Hive $false $exe
    Check (-not (Test-Path "$($case.Hive):\$uninstKey")) "$($case.Hive) has no uninstall entry"
    # The MSI would otherwise read this and install into the NSIS folder.
    Check ($null -eq (Get-Value "$($case.Hive):\Software\$manufacturer\$product" '(default)')) 'install location forgotten'
    Remove-Item -Recurse -Force $case.Dir -ErrorAction SilentlyContinue
}

foreach ($case in $nsisCases) {
    Write-Host "NSIS $($nsis.Name) $($case.Args -join ' ')"
    Run $nsis.FullName $case.Args
    Check-Nsis $case ''
    Uninstall-Nsis $case
}

# An in-app update runs the installer with /UPDATE (ADR 0011): it keeps the earlier choices,
# the PDF registration and where Lectrix is installed.
foreach ($case in $nsisCases) {
    Write-Host "NSIS $($nsis.Name) $($case.Args -join ' '), then /P /UPDATE"
    Run $nsis.FullName $case.Args
    Run $nsis.FullName @('/P', '/UPDATE')
    Check-Nsis $case ' after the update'
    Uninstall-Nsis $case
}

# Per-user installs from before ADR 0015 did not record their mode; an administrator's
# update must keep them per user, not add a copy in Program Files.
$case = $nsisCases[0]
Write-Host "NSIS $($nsis.Name) $($case.Args -join ' ') without the recorded mode, then /P /UPDATE"
Run $nsis.FullName $case.Args
Remove-ItemProperty -LiteralPath "HKCU:\$uninstKey" -Name 'CurrentUser'
Run $nsis.FullName @('/P', '/UPDATE')
Check-Nsis $case ' after the update'
Check (Has-Value "HKCU:\$uninstKey" 'CurrentUser') 'the update recorded the per-user mode'
Uninstall-Nsis $case

# --- MSI (per machine) -----------------------------------------------------------------
$msiExe = Join-Path $env:ProgramFiles "$product\lectrix.exe"
foreach ($case in @(@{ Props = @(); Registered = $true; Log = 'msi-default' }, @{ Props = @('LECTRIX_ASSOCIATE_PDF=0'); Registered = $false; Log = 'msi-no-pdf' })) {
    Write-Host "MSI $($msi.Name) $($case.Props -join ' ')"
    Run 'msiexec.exe' (@('/i', "`"$($msi.FullName)`"", '/qn', '/norestart', '/l*v', "`"$logs\$($case.Log)-install.log`"") + $case.Props)
    Check (Test-Path $msiExe) "lectrix.exe installed in $(Split-Path $msiExe)"
    Check-Registration 'HKLM' $case.Registered $msiExe
    Run 'msiexec.exe' @('/x', "`"$($msi.FullName)`"", '/qn', '/norestart', '/l*v', "`"$logs\$($case.Log)-uninstall.log`"")
    Check (-not (Test-Path $msiExe)) 'lectrix.exe removed'
    Check (-not (Test-Path (Join-Path (Split-Path $msiExe) 'lectrix-pdf.ico'))) 'PDF icon removed'
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
