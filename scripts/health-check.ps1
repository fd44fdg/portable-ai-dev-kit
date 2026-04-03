[CmdletBinding()]
param(
    [string]$UsbRoot,
    [switch]$Json,
    [switch]$Detailed
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Continue'

if (-not $UsbRoot) {
    $UsbRoot = Split-Path -Path $PSScriptRoot -Parent
}

$results = @{
    timestamp = (Get-Date).ToString('s')
    root = $UsbRoot
    checks = @{}
    issues = @()
    summary = 'healthy'
}

function Add-Check {
    param([string]$Name, [string]$Status, [string]$Message, [object]$Details)
    $results.checks[$Name] = @{
        status = $Status
        message = $Message
        details = $Details
    }
    if ($Status -eq 'error') {
        $results.issues += $Name
        $results.summary = 'unhealthy'
    } elseif ($Status -eq 'warning' -and $results.summary -ne 'unhealthy') {
        $results.summary = 'warning'
    }
}

Write-Host "Running health checks..." -ForegroundColor Cyan

$configPath = Join-Path -Path $UsbRoot -ChildPath 'config'
Add-Check -Name 'config-dir' -Status 'ok' -Message 'Config directory exists'
if (Test-Path -LiteralPath $configPath) {
    $configFiles = @('tool-manifest.json', 'package-sources.json', 'local.ps1')
    $found = 0
    foreach ($f in $configFiles) {
        $p = Join-Path -Path $configPath -ChildPath $f
        if (Test-Path -LiteralPath $p) { $found++ }
    }
    Add-Check -Name 'config-files' -Status $(if ($found -eq 3) { 'ok' } elseif ($found -gt 0) { 'warning' } else { 'error' }) -Message "Found $found/3 config files" -Details @{found = $found; total = 3}
}

$statePath = Join-Path -Path $UsbRoot -ChildPath 'state'
Add-Check -Name 'state-dir' -Status $(if (Test-Path -LiteralPath $statePath) { 'ok' } else { 'warning' }) -Message 'State directory'
if (-not (Test-Path -LiteralPath $statePath)) {
    try {
        $null = New-Item -ItemType Directory -Path $statePath -Force
        Add-Check -Name 'state-dir-created' -Status 'ok' -Message 'State directory created'
    } catch {
        Add-Check -Name 'state-dir-create' -Status 'error' -Message "Cannot create state directory: $($_.Exception.Message)"
    }
}

$workspacePath = Join-Path -Path $UsbRoot -ChildPath 'workspace'
Add-Check -Name 'workspace-dir' -Status $(if (Test-Path -LiteralPath $workspacePath) { 'ok' } else { 'warning' }) -Message 'Workspace directory'
if (-not (Test-Path -LiteralPath $workspacePath)) {
    try {
        $null = New-Item -ItemType Directory -Path $workspacePath -Force
    } catch {}
}

$drives = @()
try {
    $drive = [System.IO.Path]::GetPathRoot($UsbRoot)
    if ($drive) {
        $driveInfo = Get-PSDrive -Name $drive.TrimEnd(':\') -ErrorAction SilentlyContinue
        if ($driveInfo) {
            $freeGB = [math]::Round($driveInfo.Free / 1GB, 2)
            Add-Check -Name 'disk-space' -Status $(if ($freeGB -gt 1) { 'ok' } else { 'error' }) -Message "Free space: ${freeGB}GB" -Details @{freeGB = $freeGB}
            $drives += @{drive = $drive; freeGB = $freeGB}
        }
    }
} catch {
    Add-Check -Name 'disk-space' -Status 'warning' -Message "Cannot check disk space: $($_.Exception.Message)"
}

$manifestPath = Join-Path -Path $UsbRoot -ChildPath 'config\tool-manifest.json'
if (Test-Path -LiteralPath $manifestPath) {
    try {
        $manifest = Get-Content -LiteralPath $manifestPath -Raw | ConvertFrom-Json
        $toolCount = @($manifest.tools).Count
        Add-Check -Name 'manifest-valid' -Status 'ok' -Message "Manifest has $toolCount tools" -Details @{toolCount = $toolCount}
    } catch {
        Add-Check -Name 'manifest-valid' -Status 'error' -Message "Invalid manifest: $($_.Exception.Message)"
    }
}

$nodePath = Join-Path -Path $UsbRoot -ChildPath 'apps\node\node.exe'
Add-Check -Name 'node-installed' -Status $(if (Test-Path -LiteralPath $nodePath) { 'ok' } else { 'warning' }) -Message 'Node.js'

$nodeVersion = $null
if (Test-Path -LiteralPath $nodePath) {
    try {
        $nodeVersion = & $nodePath --version 2>&1
        Add-Check -Name 'node-version' -Status 'ok' -Message "Node $nodeVersion" -Details @{version = $nodeVersion}
    } catch {
        Add-Check -Name 'node-version' -Status 'warning' -Message "Cannot get Node version"
    }
}

$npmPath = Join-Path -Path $UsbRoot -ChildPath 'apps\node\npm.cmd'
if (-not (Test-Path -LiteralPath $npmPath)) {
    $nested = Get-ChildItem -LiteralPath (Join-Path -Path $UsbRoot -ChildPath 'apps\node') -Directory -Filter 'node-v*' -ErrorAction SilentlyContinue | Select-Object -First 1
    if ($nested) {
        $npmPath = Join-Path -Path $nested.FullName -ChildPath 'npm.cmd'
    }
}
Add-Check -Name 'npm-installed' -Status $(if (Test-Path -LiteralPath $npmPath) { 'ok' } else { 'warning' }) -Message 'npm'

$testUrls = @(
    @{name = 'npm-registry'; url = 'https://registry.npmjs.org'; label = 'NPM Registry'},
    @{name = 'npmmirror'; url = 'https://npmmirror.com'; label = 'NPM Mirror (China)'}
)
foreach ($testUrl in $testUrls) {
    try {
        $response = Invoke-WebRequest -Uri $testUrl.url -Method Head -TimeoutSec 5 -UseBasicParsing -ErrorAction Stop
        Add-Check -Name "network-$($testUrl.name)" -Status 'ok' -Message "$($testUrl.label) reachable"
    } catch {
        Add-Check -Name "network-$($testUrl.name)" -Status 'warning' -Message "$($testUrl.label): $($_.Exception.Message)"
    }
}

$aiTools = @('iflow', 'codex', 'gemini', 'openclaude')
foreach ($ai in $aiTools) {
    $toolPath = Join-Path -Path $UsbRoot -ChildPath "tools\$ai"
    $exists = Test-Path -LiteralPath $toolPath
    $wrapper = Join-Path -Path $toolPath -ChildPath "$ai.cmd"
    $wrapperExists = Test-Path -LiteralPath $wrapper
    Add-Check -Name "ai-$ai" -Status $(if ($wrapperExists) { 'ok' } elseif ($exists) { 'warning' } else { 'error' }) -Message "$ai tool" -Details @{path = $toolPath; wrapper = $wrapperExists}
}

$logDir = Join-Path -Path $UsbRoot -ChildPath 'logs'
$logSize = 0
$logFiles = 0
if (Test-Path -LiteralPath $logDir) {
    $logs = Get-ChildItem -LiteralPath $logDir -File -ErrorAction SilentlyContinue
    $logFiles = @($logs).Count
    $logStats = @($logs) | Measure-Object -Property Length -Sum
    $logSize = if ($logStats.Sum) { [math]::Round($logStats.Sum / 1MB, 2) } else { 0 }
}
Add-Check -Name 'log-files' -Status 'ok' -Message "$logFiles log files, ${logSize}MB" -Details @{count = $logFiles; sizeMB = $logSize}

$cacheDir = Join-Path -Path $UsbRoot -ChildPath 'cache'
$cacheSize = 0
if (Test-Path -LiteralPath $cacheDir) {
    $cacheFiles = Get-ChildItem -LiteralPath $cacheDir -Recurse -File -ErrorAction SilentlyContinue
    $cacheStats = @($cacheFiles) | Measure-Object -Property Length -Sum
    $cacheSize = if ($cacheStats.Sum) { [math]::Round($cacheStats.Sum / 1GB, 2) } else { 0 }
}
Add-Check -Name 'cache-size' -Status $(if ($cacheSize -lt 10) { 'ok' } else { 'warning' }) -Message "Cache size: ${cacheSize}GB" -Details @{sizeGB = $cacheSize}

if ($results.summary -eq 'healthy') {
    Write-Host "`nHealth Status: OK" -ForegroundColor Green
} elseif ($results.summary -eq 'warning') {
    Write-Host "`nHealth Status: WARNING" -ForegroundColor Yellow
    if ($results.issues.Count -gt 0) {
        Write-Host "Issues: $($results.issues -join ', ')" -ForegroundColor Yellow
    }
} else {
    Write-Host "`nHealth Status: UNHEALTHY" -ForegroundColor Red
    Write-Host "Issues: $($results.issues -join ', ')" -ForegroundColor Red
}

if ($Json) {
    $results | ConvertTo-Json -Depth 4
} else {
    Write-Host "`nDetailed Results:" -ForegroundColor Cyan
    foreach ($check in $results.checks.GetEnumerator() | Sort-Object Name) {
        $color = switch ($check.Value.status) {
            'ok' { 'Green' }
            'warning' { 'Yellow' }
            'error' { 'Red' }
            default { 'White' }
        }
        Write-Host ("  [{0}] {1}: {2}" -f $check.Value.status.ToUpper(), $check.Key, $check.Value.message) -ForegroundColor $color
    }
}

exit (@{healthy = 0; warning = 1; unhealthy = 2}[$results.summary])