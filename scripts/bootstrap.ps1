[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$UsbRoot,

    [ValidateSet('Start', 'Status')]
    [string]$EntryPoint = 'Start'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path -Path $PSScriptRoot -ChildPath 'portable-kit.psm1') -Force -DisableNameChecking

function Write-Section {
    param([string]$Message)
    Write-Host ""
    Write-Host "== $Message ==" -ForegroundColor Cyan
}

function Open-PreferredApp {
    param(
        [string]$Root,
        [hashtable]$Config,
        [object[]]$ToolStatus
    )

    $workspacePath = Join-Path -Path $Root -ChildPath $Config.WorkspacePath
    $editor = $ToolStatus | Where-Object { $_.Name -eq $Config.PreferredEditor } | Select-Object -First 1
    $terminal = $ToolStatus | Where-Object { $_.Name -eq $Config.PreferredTerminal } | Select-Object -First 1

    if ($Config.AutoOpenWorkspace -and $editor -and $editor.LaunchPath) {
        Write-Host "Launching editor: $($editor.LaunchPath)" -ForegroundColor Green
        Start-Process -FilePath $editor.LaunchPath -ArgumentList @($workspacePath) | Out-Null
        return
    }

    if ($terminal -and $terminal.LaunchPath) {
        Write-Host "Launching terminal: $($terminal.LaunchPath)" -ForegroundColor Green
        Start-Process -FilePath $terminal.LaunchPath -WorkingDirectory $workspacePath | Out-Null
        return
    }

    Write-Host "No preferred portable app found. Staying in PowerShell." -ForegroundColor Yellow
    Set-Location -LiteralPath $workspacePath
}

function Show-Status {
    param(
        [object[]]$ToolStatus,
        [object[]]$AiStatus
    )

    $aiStatusByName = @{}
    foreach ($entry in @($AiStatus)) {
        $aiStatusByName[$entry.Name] = $entry
    }

    Write-Section "Tool Status"
    foreach ($tool in $ToolStatus) {
        $state = if ($tool.Kind -eq 'ai-cli') {
            $aiEntry = $aiStatusByName[$tool.Name]
            if ($null -eq $aiEntry) {
                if ($tool.BinPath) { 'ready' } elseif ($tool.LaunchPath -or $tool.Exists) { 'partial' } else { 'missing' }
            } elseif ($aiEntry.Summary -in @('ready', 'installed', 'login-required')) {
                'ready'
            } elseif ($aiEntry.Summary -in @('wrapper-only', 'host-dependent', 'broken')) {
                'partial'
            } elseif ($tool.LaunchPath -or $tool.Exists) {
                'partial'
            } else {
                'missing'
            }
        } else {
            if ($tool.LaunchPath) { 'ready' } elseif ($tool.Exists) { 'partial' } else { 'missing' }
        }
        $color = switch ($state) {
            'ready' { 'Green' }
            'partial' { 'Yellow' }
            default { 'Red' }
        }

        Write-Host ("{0,-12} {1,-8} {2}" -f $tool.Name, $state, $tool.BasePath) -ForegroundColor $color
        if ($state -ne 'ready') {
            Write-Host ("  hint: {0}" -f $tool.InstallHint) -ForegroundColor DarkYellow
        }
    }
}

function Show-AiStatus {
    param([object[]]$AiStatus)

    Write-Section "AI Status"
    foreach ($entry in $AiStatus) {
        $color = switch ($entry.Summary) {
            'ready' { 'Green' }
            'installed' { 'Green' }
            'login-required' { 'Yellow' }
            'wrapper-only' { 'Yellow' }
            'host-dependent' { 'Yellow' }
            default { 'Red' }
        }

        Write-Host ("{0,-12} {1}" -f $entry.Name, $entry.Summary) -ForegroundColor $color
    }
}

function Start-PortableKit {
    $root = Resolve-PortableKitRoot -Path $UsbRoot

    $requiredDirectories = @(
        'apps',
        'apps\git',
        'apps\node',
        'apps\python',
        'apps\terminal',
        'apps\vscode',
        'cache',
        'cache\downloads',
        'cache\tools',
        'config',
        'docs',
        'logs',
        'scripts',
        'state',
        'tools',
        'tools\codex',
        'tools\gemini',
        'workspace'
    )

    foreach ($relativeDir in $requiredDirectories) {
        Ensure-PortableKitDirectory -Path (Join-Path -Path $root -ChildPath $relativeDir)
    }

    $configPath = Join-Path -Path $root -ChildPath 'config\local.ps1'
    $manifestPath = Join-Path -Path $root -ChildPath 'config\tool-manifest.json'
    $statePath = Join-Path -Path $root -ChildPath 'state\bootstrap-state.json'

    $config = Import-PortableKitConfig -ConfigPath $configPath
    $manifest = Get-PortableToolManifest -ManifestPath $manifestPath
    $toolStatus = Get-PortableToolStatus -Root $root -Manifest $manifest
    $aiStatus = @($toolStatus | Where-Object { $_.Kind -eq 'ai-cli' } | ForEach-Object { Get-PortableAiStatus -ToolStatus $_ })

    foreach ($entry in @($statePath, (Join-Path -Path $root -ChildPath 'logs'))) {
        $parent = Split-Path -Path $entry -Parent
        if (-not (Test-Path -LiteralPath $parent)) {
            throw "Required path is unavailable: $parent"
        }
    }

    foreach ($tool in $toolStatus) {
        if ($tool.LaunchPath) {
            Add-PortablePathEntry -PathEntry (Split-Path -Path $tool.LaunchPath -Parent)
        }
    }

    foreach ($extraPath in $config.AdditionalPaths) {
        $resolved = Resolve-ManifestPath -Root $root -RelativePath $extraPath
        Add-PortablePathEntry -PathEntry $resolved
    }

    Export-PortableBootstrapState -StatePath $statePath -ToolStatus $toolStatus -AiStatus $aiStatus -Config $config
    Show-Status -ToolStatus $toolStatus -AiStatus $aiStatus
    if ($config.ShowAiHealthOnStart) {
        Show-AiStatus -AiStatus $aiStatus
    }

    Write-Section "Workspace"
    Write-Host ("Root: {0}" -f $root)
    Write-Host ("Workspace: {0}" -f (Join-Path -Path $root -ChildPath $config.WorkspacePath))

    if ($EntryPoint -eq 'Start') {
        Open-PreferredApp -Root $root -Config $config -ToolStatus $toolStatus
    }
}

Start-PortableKit
