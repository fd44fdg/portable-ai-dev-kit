[CmdletBinding()]
param(
    [string]$UsbRoot,
    [ValidateSet('lite', 'dev', 'studio', 'full', 'quick', 'status')]
    [string]$Profile = 'lite',
    [ValidateSet('auto', 'china', 'global')]
    [string]$NetworkMode = 'auto',
    [switch]$IncludeCodex,
    [switch]$IncludeGemini,
    [switch]$IncludeOpenClaude,
    [switch]$Force,
    [switch]$DryRun,
    [switch]$NonInteractive
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path -Path $PSScriptRoot -ChildPath 'portable-kit.psm1') -Force -DisableNameChecking

if (-not $UsbRoot) {
    $UsbRoot = Split-Path -Path $PSScriptRoot -Parent
}

$root = Resolve-PortableKitRoot -Path $UsbRoot
$manifest = Get-PortableToolManifest -ManifestPath (Join-Path -Path $root -ChildPath 'config\tool-manifest.json')
$catalog = Get-PortableJsonFile -Path (Join-Path -Path $root -ChildPath 'config\package-sources.json')
$script:InstalledDuringRun = @{}

function Write-Section {
    param([string]$Message)
    Write-Host ""
    Write-Host "== $Message ==" -ForegroundColor Cyan
}

function Invoke-PostInstallAction {
    param(
        [string]$ActionName,
        [string]$Root
    )

    switch ($ActionName) {
        'create-vscode-data' {
            $dataPath = Join-Path -Path $Root -ChildPath 'apps\vscode\data'
            Ensure-PortableKitDirectory -Path $dataPath
        }
        default {
            Write-Host "Skipping unknown post-install action: $ActionName" -ForegroundColor Yellow
        }
    }
}

function Get-ToolEntry {
    param([string]$ToolName)
    return $manifest.tools | Where-Object { $_.name -eq $ToolName } | Select-Object -First 1
}

function Get-PackageEntry {
    param([string]$ToolName)
    $property = $catalog.packages.PSObject.Properties[$ToolName]
    if ($null -eq $property) {
        return $null
    }

    return $property.Value
}

function Get-InstallDestination {
    param([string]$ToolName)

    $toolEntry = Get-ToolEntry -ToolName $ToolName
    if (-not $toolEntry) {
        throw "Unknown tool in manifest: $ToolName"
    }

    return Join-Path -Path $root -ChildPath (($toolEntry.basePath -replace '/', '\'))
}

function Resolve-SetupProfile {
    param([string]$RequestedProfile)

    switch ($RequestedProfile) {
        'quick' { return 'dev' }
        default { return $RequestedProfile }
    }
}

function Resolve-NetworkSettings {
    param([string]$Mode)

    $property = $catalog.networkModes.PSObject.Properties[$Mode]
    if ($null -eq $property) {
        throw "Unknown network mode: $Mode"
    }

    return $property.Value
}

function Get-PackageUrl {
    param(
        [object]$PackageEntry,
        [string]$SourceMode
    )

    $urls = $PackageEntry.PSObject.Properties['urls']
    if ($null -eq $urls) {
        return $PackageEntry.url
    }

    $urlProperty = $PackageEntry.urls.PSObject.Properties[$SourceMode]
    if ($null -eq $urlProperty) {
        $urlProperty = $PackageEntry.urls.PSObject.Properties['global']
    }

    if ($null -eq $urlProperty) {
        throw "No package URL is configured for source mode: $SourceMode"
    }

    return $urlProperty.Value
}

function Read-MenuChoice {
    param(
        [string]$Prompt,
        [hashtable]$Choices,
        [string]$DefaultChoice
    )

    while ($true) {
        $suffix = if ($DefaultChoice) { " [$DefaultChoice]" } else { '' }
        $rawValue = Read-Host ($Prompt + $suffix)
        if ([string]::IsNullOrWhiteSpace($rawValue) -and $DefaultChoice) {
            if ($Choices.ContainsKey($DefaultChoice)) {
                return $Choices[$DefaultChoice]
            }

            return $DefaultChoice
        }

        $normalized = $rawValue.Trim().ToLowerInvariant()
        if ($Choices.ContainsKey($normalized)) {
            return $Choices[$normalized]
        }

        Write-Host "Invalid selection. Try again." -ForegroundColor Yellow
    }
}

function Read-YesNoChoice {
    param(
        [string]$Prompt,
        [bool]$DefaultValue = $false
    )

    $defaultKey = if ($DefaultValue) { 'y' } else { 'n' }
    $result = Read-MenuChoice -Prompt $Prompt -Choices @{
        'y' = $true
        'yes' = $true
        'n' = $false
        'no' = $false
    } -DefaultChoice $defaultKey

    return [bool]$result
}

function Show-InstallPlan {
    param(
        [string]$ResolvedProfile,
        [string]$ResolvedNetworkMode,
        [object]$NetworkSettings,
        [string[]]$SelectedTools
    )

    Write-Section "Install Plan"
    Write-Host ("Root: {0}" -f $root)
    Write-Host ("Network mode: {0}" -f $ResolvedNetworkMode)
    Write-Host ("Profile: {0}" -f $ResolvedProfile)
    Write-Host ("npm registry: {0}" -f $NetworkSettings.npmRegistry)
    Write-Host ""

    foreach ($toolName in $SelectedTools) {
        $packageEntry = Get-PackageEntry -ToolName $toolName
        if (-not $packageEntry) {
            continue
        }

        $destination = Get-InstallDestination -ToolName $toolName
        if ($packageEntry.type -eq 'archive') {
            $url = Get-PackageUrl -PackageEntry $packageEntry -SourceMode $NetworkSettings.packageSourceMode
            Write-Host ("- {0}: download {1}" -f $toolName, $url)
            Write-Host ("  install to {0}" -f $destination)
        } elseif ($packageEntry.type -eq 'npm') {
            Write-Host ("- {0}: npm install {1}" -f $toolName, $packageEntry.packageName)
            Write-Host ("  registry {0}" -f $NetworkSettings.npmRegistry)
            Write-Host ("  install to {0}" -f $destination)
        }
    }

    if ($SelectedTools -contains 'codex') {
        Write-Host ""
        Write-Host "Note: Codex may still require network conditions that are not stable in mainland China." -ForegroundColor Yellow
    }

    if ($SelectedTools -contains 'gemini') {
        Write-Host "Note: Gemini often has stricter network access requirements in mainland China." -ForegroundColor Yellow
    }
}

function Install-ArchiveTool {
    param(
        [string]$ToolName,
        [object]$PackageEntry,
        [object]$NetworkSettings
    )

    $downloadPath = Join-Path -Path $root -ChildPath ("cache\downloads\" + $PackageEntry.archiveName)
    $destination = Get-InstallDestination -ToolName $ToolName
    $packageUrl = Get-PackageUrl -PackageEntry $PackageEntry -SourceMode $NetworkSettings.packageSourceMode

    if ($DryRun) {
        Write-Host ("[DryRun] Download {0}" -f $packageUrl) -ForegroundColor Yellow
        Write-Host ("[DryRun] Save to {0}" -f $downloadPath) -ForegroundColor Yellow
        Write-Host ("[DryRun] Install to {0}" -f $destination) -ForegroundColor Yellow
        return
    }

    if (Test-Path -LiteralPath $downloadPath) {
        Write-Host ("Using cached archive: {0}" -f $downloadPath) -ForegroundColor DarkGreen
    } else {
        Save-PortableDownload -Url $packageUrl -DestinationPath $downloadPath | Out-Null
    }

    Write-Host ("Extracting to {0}" -f $destination) -ForegroundColor Cyan
    Install-PortableArchive -ArchivePath $downloadPath -DestinationPath $destination -InstallerType $PackageEntry.installerType -Force:$Force

    foreach ($actionName in @($PackageEntry.postInstall)) {
        Invoke-PostInstallAction -ActionName $actionName -Root $root
    }
}

function Install-NpmTool {
    param(
        [string]$ToolName,
        [object]$PackageEntry,
        [object]$NetworkSettings
    )

    foreach ($dependency in @($PackageEntry.dependsOn)) {
        Install-ToolByName -ToolName $dependency
    }

    $toolRoot = Get-InstallDestination -ToolName $ToolName
    $nodeRoot = Get-InstallDestination -ToolName 'node'

    if ($DryRun) {
        Write-Host ("[DryRun] npm install {0} into {1}" -f $PackageEntry.packageName, $toolRoot) -ForegroundColor Yellow
        Write-Host ("[DryRun] Registry {0}" -f $NetworkSettings.npmRegistry) -ForegroundColor Yellow
        return
    }

    Write-Host ("Installing {0} via npm..." -f $ToolName) -ForegroundColor Cyan
    Install-PortableNpmPackage -NodeRoot $nodeRoot -PackageName $PackageEntry.packageName -ToolRoot $toolRoot -RegistryUrl $NetworkSettings.npmRegistry -Force:$Force
}

function Install-ToolByName {
    param([string]$ToolName)

    if ($script:InstalledDuringRun.ContainsKey($ToolName) -and -not $Force) {
        Write-Host ("Skipping {0}: already handled in this run" -f $ToolName) -ForegroundColor DarkGreen
        return
    }

    $toolStatus = Get-PortableToolStatus -Root $root -Manifest $manifest | Where-Object { $_.Name -eq $ToolName } | Select-Object -First 1
    if (-not $toolStatus) {
        throw "Unknown tool: $ToolName"
    }

    if ($toolStatus.BinPath -and -not $Force) {
        Write-Host ("Skipping {0}: already installed" -f $ToolName) -ForegroundColor Green
        return
    }

    $packageEntry = Get-PackageEntry -ToolName $ToolName
    if (-not $packageEntry) {
        throw "No package source is configured for $ToolName"
    }

    Write-Section ("Installing " + $ToolName)
    switch ($packageEntry.type) {
        'archive' {
            Install-ArchiveTool -ToolName $ToolName -PackageEntry $packageEntry -NetworkSettings $script:NetworkSettings
        }
        'npm' {
            Install-NpmTool -ToolName $ToolName -PackageEntry $packageEntry -NetworkSettings $script:NetworkSettings
        }
        default {
            throw ("Unsupported package type for {0}: {1}" -f $ToolName, $packageEntry.type)
        }
    }

    $script:InstalledDuringRun[$ToolName] = $true
}

if ($Profile -eq 'status') {
    & (Join-Path -Path $PSScriptRoot -ChildPath 'bootstrap.ps1') -UsbRoot $root -EntryPoint Status
    exit 0
}

$resolvedProfile = Resolve-SetupProfile -RequestedProfile $Profile
if (-not $catalog.profiles.PSObject.Properties[$resolvedProfile]) {
    throw "Unknown profile: $resolvedProfile"
}

$shouldPromptProfile = (-not $NonInteractive) -and (-not $PSBoundParameters.ContainsKey('Profile'))
$shouldPromptNetwork = (-not $NonInteractive) -and (($NetworkMode -eq 'auto') -and (-not $PSBoundParameters.ContainsKey('NetworkMode')))
$shouldPromptCodex = (-not $NonInteractive) -and (-not $PSBoundParameters.ContainsKey('IncludeCodex'))
$shouldPromptGemini = (-not $NonInteractive) -and (-not $PSBoundParameters.ContainsKey('IncludeGemini'))
$shouldPromptOpenClaude = (-not $NonInteractive) -and (-not $PSBoundParameters.ContainsKey('IncludeOpenClaude'))

if ($shouldPromptNetwork) {
    Write-Section "Network Mode"
    Write-Host "1. China (recommended in mainland China)"
    Write-Host "2. Global"
    $NetworkMode = Read-MenuChoice -Prompt 'Choose network mode' -Choices @{
        '1' = 'china'
        'china' = 'china'
        '2' = 'global'
        'global' = 'global'
    } -DefaultChoice '1'
}

    if ($shouldPromptProfile) {
        Write-Section "Install Profile"
        Write-Host "1. lite: Node + iFlow"
        Write-Host "2. dev: Git + Node + iFlow"
        Write-Host "3. studio: Git + Node + VS Code + iFlow"
        Write-Host "4. full: studio + Python + terminal"
        $resolvedProfile = Read-MenuChoice -Prompt 'Choose install profile' -Choices @{
            '1' = 'lite'
            'lite' = 'lite'
            '2' = 'dev'
            'dev' = 'dev'
            '3' = 'studio'
            'studio' = 'studio'
            '4' = 'full'
            'full' = 'full'
        } -DefaultChoice '1'
    }

if ($NetworkMode -eq 'auto') {
    $NetworkMode = 'global'
}

$script:NetworkSettings = Resolve-NetworkSettings -Mode $NetworkMode
$selectedProfile = @($catalog.profiles.PSObject.Properties[$resolvedProfile].Value)

if ($shouldPromptCodex) {
    $IncludeCodex = Read-YesNoChoice -Prompt 'Install Codex CLI too?' -DefaultValue $false
}

if ($shouldPromptGemini) {
    $IncludeGemini = Read-YesNoChoice -Prompt 'Install Gemini CLI too?' -DefaultValue $false
}

if ($shouldPromptOpenClaude) {
    $IncludeOpenClaude = Read-YesNoChoice -Prompt 'Install OpenClaude CLI too?' -DefaultValue $false
}

$selectedTools = @($selectedProfile)
if ($IncludeCodex -and $selectedTools -notcontains 'codex') {
    $selectedTools += 'codex'
}

if ($IncludeGemini -and $selectedTools -notcontains 'gemini') {
    $selectedTools += 'gemini'
}

if ($IncludeOpenClaude -and $selectedTools -notcontains 'openclaude') {
    $selectedTools += 'openclaude'
}

Write-Section "Portable AI Dev Kit Setup"
if ($DryRun) {
    Write-Host "Dry run mode enabled. No files will be downloaded or installed." -ForegroundColor Yellow
}

Show-InstallPlan -ResolvedProfile $resolvedProfile -ResolvedNetworkMode $NetworkMode -NetworkSettings $script:NetworkSettings -SelectedTools $selectedTools

if (-not $NonInteractive -and -not $DryRun) {
    $confirmed = Read-YesNoChoice -Prompt 'Proceed with setup now?' -DefaultValue $true
    if (-not $confirmed) {
        Write-Host "Setup cancelled." -ForegroundColor Yellow
        exit 0
    }
}

foreach ($toolName in $selectedTools) {
    Install-ToolByName -ToolName $toolName
}

Write-Section "Next Step"
Write-Host "Setup complete. Running status check..." -ForegroundColor Green
& (Join-Path -Path $PSScriptRoot -ChildPath 'bootstrap.ps1') -UsbRoot $root -EntryPoint Status
exit 0
