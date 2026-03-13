[CmdletBinding()]
param(
    [string]$UsbRoot
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path -Path $PSScriptRoot -ChildPath 'portable-kit.psm1') -Force -DisableNameChecking

if (-not $UsbRoot) {
    $UsbRoot = Split-Path -Path $PSScriptRoot -Parent
}

$root = Resolve-PortableKitRoot -Path $UsbRoot
$manifest = Get-PortableToolManifest -ManifestPath (Join-Path -Path $root -ChildPath 'config\tool-manifest.json')
$toolStatus = @(Get-PortableToolStatus -Root $root -Manifest $manifest | Where-Object { $_.Kind -eq 'ai-cli' })

if ($toolStatus.Count -eq 0) {
    Write-Host "No AI tools are configured." -ForegroundColor Yellow
    exit 1
}

Write-Host ""
Write-Host "AI Login Menu" -ForegroundColor Cyan
Write-Host "Select one tool to log in." -ForegroundColor Cyan
Write-Host ""

for ($i = 0; $i -lt $toolStatus.Count; $i++) {
    $tool = $toolStatus[$i]
    $aiState = Get-PortableAiStatus -ToolStatus $tool
    $state = $aiState.Summary
    Write-Host ("{0}. {1} [{2}]" -f ($i + 1), $tool.Name, $state)
}

Write-Host ("{0}. Exit" -f ($toolStatus.Count + 1))
Write-Host ""

[int]$choice = 0
$choiceRaw = Read-Host "Enter a number"
if (-not [int]::TryParse($choiceRaw, [ref]$choice)) {
    Write-Host "Invalid selection." -ForegroundColor Yellow
    exit 1
}

if ($choice -eq ($toolStatus.Count + 1)) {
    exit 0
}

if ($choice -lt 1 -or $choice -gt $toolStatus.Count) {
    Write-Host "Invalid selection." -ForegroundColor Yellow
    exit 1
}

$selected = $toolStatus[$choice - 1]
& (Join-Path -Path $PSScriptRoot -ChildPath 'ai-tool.ps1') -UsbRoot $root -Tool $selected.Name -Action login
exit $LASTEXITCODE
