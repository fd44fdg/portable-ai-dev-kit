[CmdletBinding()]
param(
    [string]$UsbRoot,
    [string]$Tool,
    [string]$ArchivePath,
    [switch]$Force,
    [switch]$ShowManifestOnly
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path -Path $PSScriptRoot -ChildPath 'portable-kit.psm1') -Force -DisableNameChecking

if (-not $UsbRoot) {
    $UsbRoot = Split-Path -Path $PSScriptRoot -Parent
}

$root = Resolve-PortableKitRoot -Path $UsbRoot
$manifestPath = Join-Path -Path $root -ChildPath 'config\tool-manifest.json'
$manifest = Get-PortableToolManifest -ManifestPath $manifestPath

if ($ShowManifestOnly) {
    $manifest.tools | Format-Table name, kind, required, basePath, archiveName, source, installHint -AutoSize
    return
}

if (-not $Tool -or -not $ArchivePath) {
    Write-Host "Usage:" -ForegroundColor Yellow
    Write-Host "  .\\scripts\\install-tools.ps1 -Tool vscode -ArchivePath F:\\cache\\downloads\\vscode.zip"
    Write-Host "  .\\scripts\\install-tools.ps1 -ShowManifestOnly"
    exit 1
}

$toolEntry = $manifest.tools | Where-Object { $_.name -eq $Tool } | Select-Object -First 1
if (-not $toolEntry) {
    throw "Unknown tool: $Tool"
}

if (-not (Test-Path -LiteralPath $ArchivePath)) {
    throw "Archive not found: $ArchivePath"
}

$destination = Join-Path -Path $root -ChildPath (($toolEntry.basePath -replace '/', '\'))
$parent = Split-Path -Path $destination -Parent
if (-not (Test-Path -LiteralPath $parent)) {
    Ensure-PortableKitDirectory -Path $parent
}

if ((Test-Path -LiteralPath $destination) -and $Force) {
    Get-ChildItem -LiteralPath $destination -Force | Remove-Item -Recurse -Force
}

if (-not (Test-Path -LiteralPath $destination)) {
    Ensure-PortableKitDirectory -Path $destination
}

$installerType = 'zip'
$manifestInstallerType = $toolEntry.PSObject.Properties['installerType']
if ($manifestInstallerType) {
    $installerType = $manifestInstallerType.Value
}

Install-PortableArchive -ArchivePath $ArchivePath -DestinationPath $destination -InstallerType $installerType -Force:$Force

$stateDir = Join-Path -Path $root -ChildPath 'state'
if (-not (Test-Path -LiteralPath $stateDir)) {
    Ensure-PortableKitDirectory -Path $stateDir
}

$installStatePath = Join-Path -Path $stateDir -ChildPath 'installed-tools.json'
$installState = @()
if (Test-Path -LiteralPath $installStatePath) {
    $installState = Get-Content -LiteralPath $installStatePath -Raw | ConvertFrom-Json
}

$existing = @($installState | Where-Object { $_.name -ne $Tool })
$existing += [pscustomobject]@{
    name = $Tool
    archiveName = $toolEntry.archiveName
    source = $toolEntry.source
    archivePath = [System.IO.Path]::GetFullPath($ArchivePath)
    installedAt = (Get-Date).ToString('s')
    destination = $destination
}

$existing | ConvertTo-Json -Depth 4 | Set-Content -LiteralPath $installStatePath -Encoding UTF8

Write-Host "Installed $Tool to $destination" -ForegroundColor Green
