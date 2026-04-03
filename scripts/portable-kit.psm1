Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

function Resolve-PortableKitRoot {
    param([string]$Path)

    if ([string]::IsNullOrWhiteSpace($Path)) {
        throw 'Portable kit root path is empty.'
    }

    $cleanPath = $Path.Trim()
    $cleanPath = $cleanPath.Trim('"')

    if ($cleanPath.Length -eq 2 -and $cleanPath[1] -eq ':') {
        $cleanPath = $cleanPath + '\'
    }

    return $ExecutionContext.SessionState.Path.GetUnresolvedProviderPathFromPSPath($cleanPath)
}

function Ensure-PortableKitDirectory {
    param([string]$Path)
    if (-not (Test-Path -LiteralPath $Path)) {
        try {
            $null = New-Item -ItemType Directory -Path $Path -Force
        } catch {
            throw "Unable to create directory: $Path. $($_.Exception.Message)"
        }
    }
}

function Get-PortableKitDefaultConfig {
    return @{
        WorkspacePath = 'workspace'
        PreferredEditor = 'vscode'
        PreferredTerminal = 'terminal'
        AutoOpenWorkspace = $false
        ShowAiHealthOnStart = $true
        AdditionalPaths = @()
    }
}

function Import-PortableKitConfig {
    param([string]$ConfigPath)

    $config = Get-PortableKitDefaultConfig
    if (Test-Path -LiteralPath $ConfigPath) {
        . $ConfigPath
        if ($script:PortableKitConfig -is [hashtable]) {
            foreach ($key in $script:PortableKitConfig.Keys) {
                $config[$key] = $script:PortableKitConfig[$key]
            }
        }
    }

    return $config
}

function Get-PortableToolManifest {
    param([string]$ManifestPath)

    if (-not (Test-Path -LiteralPath $ManifestPath)) {
        throw "Tool manifest not found: $ManifestPath"
    }

    return Get-Content -LiteralPath $ManifestPath -Raw | ConvertFrom-Json
}

$script:AddedPathEntries = @{}

function Add-PortablePathEntry {
    param([string]$PathEntry)

    if ([string]::IsNullOrWhiteSpace($PathEntry)) {
        return
    }

    $normalizedPath = $PathEntry.TrimEnd('\')
    if ($script:AddedPathEntries.ContainsKey($normalizedPath)) {
        return
    }

    if (-not (Test-Path -LiteralPath $normalizedPath)) {
        return
    }

    $current = @($env:PATH -split ';' | ForEach-Object { $_.TrimEnd('\') })
    if ($current -notcontains $normalizedPath) {
        $env:PATH = "$normalizedPath;$env:PATH"
        $script:AddedPathEntries[$normalizedPath] = $true
    }
}

function Clear-PathCache {
    $script:AddedPathEntries.Clear()
}

function Resolve-ManifestPath {
    param(
        [string]$Root,
        [string]$RelativePath
    )

    $expanded = $RelativePath -replace '/', '\'
    return Join-Path -Path $Root -ChildPath $expanded
}

$script:NestedDirectoryCache = @{}

function Clear-PortableCache {
    $script:NestedDirectoryCache.Clear()
}

function Clear-PathCache {
    $script:AddedPathEntries.Clear()
}

function Find-PortableToolPath {
    param(
        [string]$BasePath,
        [object]$RelativePaths
    )

    if (-not $script:NestedDirectoryCache.ContainsKey($BasePath)) {
        $script:NestedDirectoryCache[$BasePath] = @(Get-ChildItem -LiteralPath $BasePath -Directory -Force -ErrorAction SilentlyContinue)
    }
    $nestedDirectories = $script:NestedDirectoryCache[$BasePath]

    foreach ($relativePath in @($RelativePaths)) {
        if ([string]::IsNullOrWhiteSpace($relativePath)) {
            continue
        }

        $candidate = Join-Path -Path $BasePath -ChildPath ($relativePath -replace '/', '\')
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }

        foreach ($nestedDirectory in $nestedDirectories) {
            $nestedCandidate = Join-Path -Path $nestedDirectory.FullName -ChildPath ($relativePath -replace '/', '\')
            if (Test-Path -LiteralPath $nestedCandidate) {
                return $nestedCandidate
            }
        }
    }

    return $null
}

function Get-ManifestPropertyValue {
    param(
        [object]$InputObject,
        [string]$PropertyName,
        $DefaultValue = $null
    )

    $property = $InputObject.PSObject.Properties[$PropertyName]
    if ($null -eq $property) {
        return $DefaultValue
    }

    return $property.Value
}

function Get-PortableToolStatus {
    param(
        [string]$Root,
        [object]$Manifest
    )

    $results = @()
    foreach ($tool in $Manifest.tools) {
        $basePath = Resolve-ManifestPath -Root $Root -RelativePath $tool.basePath
        $exists = Test-Path -LiteralPath $basePath
        $launchPath = $null
        $binPath = $null
        $wrapperPaths = @(Get-ManifestPropertyValue -InputObject $tool -PropertyName 'wrapperPaths' -DefaultValue @())
        $binPaths = @(Get-ManifestPropertyValue -InputObject $tool -PropertyName 'binPaths' -DefaultValue @())
        $loginCheckArgs = @(Get-ManifestPropertyValue -InputObject $tool -PropertyName 'loginCheckArgs' -DefaultValue @())
        $loginArgs = @(Get-ManifestPropertyValue -InputObject $tool -PropertyName 'loginArgs' -DefaultValue @())
        $loginCheckIndicatesAuth = [bool](Get-ManifestPropertyValue -InputObject $tool -PropertyName 'loginCheckIndicatesAuth' -DefaultValue $false)
        $portableValidationPaths = @(Get-ManifestPropertyValue -InputObject $tool -PropertyName 'portableValidationPaths' -DefaultValue @())
        $source = Get-ManifestPropertyValue -InputObject $tool -PropertyName 'source'
        $archiveName = Get-ManifestPropertyValue -InputObject $tool -PropertyName 'archiveName'
        $installHint = Get-ManifestPropertyValue -InputObject $tool -PropertyName 'installHint'
        $portableValidationPath = $null

        if ($exists) {
            $launchPath = Find-PortableToolPath -BasePath $basePath -RelativePaths $wrapperPaths
            $binPath = Find-PortableToolPath -BasePath $basePath -RelativePaths $binPaths
            $portableValidationPath = Find-PortableToolPath -BasePath $basePath -RelativePaths $portableValidationPaths
            if (-not $launchPath) {
                $launchPath = $binPath
            }
        }

        $portableAvailable = if ($portableValidationPaths.Count -gt 0) {
            [bool]$portableValidationPath
        } else {
            [bool]$binPath
        }

        $results += [pscustomobject]@{
            Root = $Root
            Name = $tool.name
            Kind = $tool.kind
            BasePath = $basePath
            Exists = $exists
            LaunchPath = $launchPath
            BinPath = $binPath
            PortableValidationPath = $portableValidationPath
            PortableAvailable = $portableAvailable
            Required = [bool]$tool.required
            InstallHint = $installHint
            LoginCheckArgs = $loginCheckArgs
            LoginArgs = $loginArgs
            LoginCheckIndicatesAuth = $loginCheckIndicatesAuth
            Source = $source
            ArchiveName = $archiveName
        }
    }

    return $results
}

function Get-PortableAiStatus {
    param([pscustomobject]$ToolStatus)

    $state = [pscustomobject]@{
        Name = $ToolStatus.Name
        Installed = [bool]$ToolStatus.PortableAvailable
        LoggedIn = $false
        Summary = 'not-installed'
    }

    if (-not $ToolStatus.LaunchPath) {
        return $state
    }

    if (-not $ToolStatus.BinPath) {
        $state.Summary = 'wrapper-only'
        return $state
    }

    if (-not $ToolStatus.PortableAvailable) {
        $state.Summary = 'host-dependent'
        return $state
    }

    if (-not $ToolStatus.LoginCheckArgs -or $ToolStatus.LoginCheckArgs.Count -eq 0) {
        $state.Summary = 'installed'
        return $state
    }

    return $state
}

function Get-PortableToolWorkingDirectory {
    param(
        [pscustomobject]$ToolStatus,
        [string]$Root
    )

    if (-not [string]::IsNullOrWhiteSpace($Root)) {
        $configPath = Join-Path -Path $Root -ChildPath 'config\local.ps1'
        $config = Import-PortableKitConfig -ConfigPath $configPath
        $workspacePath = Join-Path -Path $Root -ChildPath $config.WorkspacePath
        if (Test-Path -LiteralPath $workspacePath) {
            return $workspacePath
        }
    }

    if ($ToolStatus.BasePath -and (Test-Path -LiteralPath $ToolStatus.BasePath)) {
        return $ToolStatus.BasePath
    }

    if ($ToolStatus.BinPath) {
        return (Split-Path -Path $ToolStatus.BinPath -Parent)
    }

    return $PWD.Path
}

function Set-PortableToolEnvironment {
    param([string]$Root)

    if ([string]::IsNullOrWhiteSpace($Root)) {
        return @{}
    }

    $portableEnv = @{
        HOME = Join-Path -Path $Root -ChildPath 'state\home'
        USERPROFILE = Join-Path -Path $Root -ChildPath 'state\home'
        APPDATA = Join-Path -Path $Root -ChildPath 'state\appdata'
        LOCALAPPDATA = Join-Path -Path $Root -ChildPath 'state\localappdata'
        XDG_CONFIG_HOME = Join-Path -Path $Root -ChildPath 'state\xdg\config'
        XDG_CACHE_HOME = Join-Path -Path $Root -ChildPath 'state\xdg\cache'
        XDG_STATE_HOME = Join-Path -Path $Root -ChildPath 'state\xdg\state'
    }
    $hostEnv = @{
        PORTABLEKIT_HOST_HOME = [Environment]::GetEnvironmentVariable('HOME')
        PORTABLEKIT_HOST_USERPROFILE = [Environment]::GetEnvironmentVariable('USERPROFILE')
        PORTABLEKIT_HOST_APPDATA = [Environment]::GetEnvironmentVariable('APPDATA')
        PORTABLEKIT_HOST_LOCALAPPDATA = [Environment]::GetEnvironmentVariable('LOCALAPPDATA')
    }

    $missingPaths = @($portableEnv.Values | Where-Object { -not (Test-Path -LiteralPath $_) })
    foreach ($path in $missingPaths) {
        $null = New-Item -ItemType Directory -Path $path -Force
    }

    $originalEnv = @{}
    foreach ($entry in $portableEnv.GetEnumerator()) {
        $originalEnv[$entry.Key] = [Environment]::GetEnvironmentVariable($entry.Key)
        [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value)
    }
    foreach ($entry in $hostEnv.GetEnumerator()) {
        $originalEnv[$entry.Key] = [Environment]::GetEnvironmentVariable($entry.Key)
        [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value)
    }

    return $originalEnv
}

function Restore-PortableToolEnvironment {
    param([hashtable]$OriginalEnv)

    if ($null -eq $OriginalEnv) {
        return
    }

    foreach ($entry in $OriginalEnv.GetEnumerator()) {
        [Environment]::SetEnvironmentVariable($entry.Key, $entry.Value)
    }
}

function Export-PortableBootstrapState {
    param(
        [string]$StatePath,
        [object]$ToolStatus,
        [object]$AiStatus,
        [hashtable]$Config
    )

    $payload = [pscustomobject]@{
        generatedAt = (Get-Date).ToString('s')
        hostComputer = $env:COMPUTERNAME
        workspacePath = $Config.WorkspacePath
        tools = $ToolStatus
        ai = $AiStatus
    }

    $payload | ConvertTo-Json -Depth 6 | Set-Content -LiteralPath $StatePath -Encoding UTF8

    $logDir = Split-Path -Path $StatePath -Parent
    $maxLogFiles = 10
    $maxLogSizeMB = 50

    if (Test-Path -LiteralPath $logDir) {
        $allFiles = Get-ChildItem -LiteralPath $logDir -File -ErrorAction SilentlyContinue
        $logFiles = @($allFiles | Where-Object { $_.Name -like '*.log' } | Sort-Object LastWriteTime -Descending)
        
        $totalSizeMB = 0
        if ($logFiles.Count -gt 0) {
            $totalSize = 0
            foreach ($f in $logFiles) { $totalSize += $f.Length }
            $totalSizeMB = [math]::Round($totalSize / 1MB, 2)
        }

        if ($logFiles.Count -gt $maxLogFiles) {
            $toDelete = $logFiles | Select-Object -Skip $maxLogFiles
            foreach ($f in $toDelete) {
                Remove-Item -LiteralPath $f.FullName -Force -ErrorAction SilentlyContinue
            }
        }

        if ($totalSizeMB -gt $maxLogSizeMB) {
            $toDeleteSize = $logFiles | Select-Object -Skip 5
            foreach ($f in $toDeleteSize) {
                Remove-Item -LiteralPath $f.FullName -Force -ErrorAction SilentlyContinue
            }
        }
    }
}

function Get-PortableJsonFile {
    param([string]$Path)

    if (-not (Test-Path -LiteralPath $Path)) {
        throw "JSON file not found: $Path"
    }

    return Get-Content -LiteralPath $Path -Raw | ConvertFrom-Json
}

function Save-PortableDownload {
    param(
        [Parameter(Mandatory = $true)]
        [string]$Url,

        [Parameter(Mandatory = $true)]
        [string]$DestinationPath
    )

    $destinationDir = Split-Path -Path $DestinationPath -Parent
    Ensure-PortableKitDirectory -Path $destinationDir
    Write-Host "Downloading $Url" -ForegroundColor Cyan
    Invoke-WebRequest -Uri $Url -OutFile $DestinationPath -UseBasicParsing
    return $DestinationPath
}

function Install-PortableArchive {
    param(
        [Parameter(Mandatory = $true)]
        [string]$ArchivePath,

        [Parameter(Mandatory = $true)]
        [string]$DestinationPath,

        [string]$InstallerType = 'zip',
        [switch]$Force
    )

    Ensure-PortableKitDirectory -Path $DestinationPath

    if ((Test-Path -LiteralPath $DestinationPath) -and $Force) {
        Get-ChildItem -LiteralPath $DestinationPath -Force | Remove-Item -Recurse -Force
        Ensure-PortableKitDirectory -Path $DestinationPath
    }

    if ((Test-Path -LiteralPath $DestinationPath) -and -not $Force) {
        $existingItems = @(Get-ChildItem -LiteralPath $DestinationPath -Force -ErrorAction SilentlyContinue | Where-Object { $_.Name -ne '.keep' })
        if ($existingItems.Count -gt 0) {
            throw "Destination already contains files: $DestinationPath. Re-run with -Force to replace them."
        }
    }

    switch ($InstallerType) {
        'zip' {
            Expand-Archive -LiteralPath $ArchivePath -DestinationPath $DestinationPath -Force
        }
        'self-extract-7z' {
            & $ArchivePath "-o$DestinationPath" '-y' | Out-Null
            if ($LASTEXITCODE -ne 0) {
                throw "Self-extract installer failed: $ArchivePath"
            }
        }
        default {
            throw "Unsupported installer type: $InstallerType"
        }
    }

    $topLevelItems = @(Get-ChildItem -LiteralPath $DestinationPath -Force | Where-Object { $_.Name -ne '.keep' })
    if ($topLevelItems.Count -eq 1 -and $topLevelItems[0].PSIsContainer) {
        $nestedRoot = $topLevelItems[0].FullName
        $nestedItems = @(Get-ChildItem -LiteralPath $nestedRoot -Force)
        foreach ($item in $nestedItems) {
            Move-Item -LiteralPath $item.FullName -Destination $DestinationPath -Force
        }

        Remove-Item -LiteralPath $nestedRoot -Force -Recurse
    }
}

function Get-PortableNpmCommand {
    param([string]$NodeRoot)

    $candidates = @(
        (Join-Path -Path $NodeRoot -ChildPath 'npm.cmd'),
        (Join-Path -Path $NodeRoot -ChildPath 'node_modules\npm\bin\npm-cli.js')
    )

    $nestedRoot = Get-ChildItem -LiteralPath $NodeRoot -Directory -Force -ErrorAction SilentlyContinue |
        Where-Object { $_.Name -like 'node-v*-win-*' } |
        Select-Object -First 1

    if ($nestedRoot) {
        $candidates += @(
            (Join-Path -Path $nestedRoot.FullName -ChildPath 'npm.cmd'),
            (Join-Path -Path $nestedRoot.FullName -ChildPath 'node_modules\npm\bin\npm-cli.js')
        )
    }

    foreach ($candidate in $candidates) {
        if (Test-Path -LiteralPath $candidate) {
            return $candidate
        }
    }

    return $null
}

function Install-PortableNpmPackage {
    param(
        [Parameter(Mandatory = $true)]
        [string]$NodeRoot,

        [Parameter(Mandatory = $true)]
        [string]$PackageName,

        [Parameter(Mandatory = $true)]
        [string]$ToolRoot,

        [string]$RegistryUrl,

        [switch]$Force
    )

    $nodeExe = Join-Path -Path $NodeRoot -ChildPath 'node.exe'
    if (-not (Test-Path -LiteralPath $nodeExe)) {
        $nestedRoot = Get-ChildItem -LiteralPath $NodeRoot -Directory -Force -ErrorAction SilentlyContinue |
            Where-Object { $_.Name -like 'node-v*-win-*' } |
            Select-Object -First 1
        if ($nestedRoot) {
            $nodeExe = Join-Path -Path $nestedRoot.FullName -ChildPath 'node.exe'
        }
    }

    if (-not (Test-Path -LiteralPath $nodeExe)) {
        throw "Node runtime not found: $nodeExe"
    }

    $nodeBinDirectory = Split-Path -Path $nodeExe -Parent
    $npmCommand = Get-PortableNpmCommand -NodeRoot $NodeRoot
    if (-not $npmCommand) {
        throw "npm was not found in: $NodeRoot"
    }

    Ensure-PortableKitDirectory -Path $ToolRoot
    $packageJsonPath = Join-Path -Path $ToolRoot -ChildPath 'package.json'
    if (-not (Test-Path -LiteralPath $packageJsonPath)) {
        '{"name":"portable-ai-tool","private":true}' | Set-Content -LiteralPath $packageJsonPath -Encoding UTF8
    }

    if ($Force) {
        foreach ($cleanupPath in @('node_modules', 'package-lock.json')) {
            $fullPath = Join-Path -Path $ToolRoot -ChildPath $cleanupPath
            if (Test-Path -LiteralPath $fullPath) {
                Remove-Item -LiteralPath $fullPath -Recurse -Force
            }
        }
    }

    $npmArgs = @('install', '--prefix', $ToolRoot, $PackageName, '--no-fund', '--no-audit')
    if (-not [string]::IsNullOrWhiteSpace($RegistryUrl)) {
        $npmArgs += @('--registry', $RegistryUrl)
    }

    $originalPath = $env:PATH
    $env:PATH = "$nodeBinDirectory;$originalPath"
    try {
        if ($npmCommand -like '*.js') {
            & $nodeExe $npmCommand @npmArgs
        } else {
            & $npmCommand @npmArgs
        }
    } finally {
        $env:PATH = $originalPath
    }

    if ($LASTEXITCODE -ne 0) {
        throw "npm install failed for $PackageName"
    }
}

function Invoke-PortableToolCommand {
    param(
        [Parameter(Mandatory = $true)]
        [pscustomobject]$ToolStatus,

        [string[]]$Arguments = @(),

        [string]$Root
    )

    if (-not $ToolStatus.BinPath) {
        throw "Tool binary is unavailable: $($ToolStatus.Name)"
    }

    $originalPath = $env:PATH
    $workingDirectory = Get-PortableToolWorkingDirectory -ToolStatus $ToolStatus -Root $Root
    $locationPushed = $false
    $originalEnv = $null
    try {
        $originalEnv = Set-PortableToolEnvironment -Root $Root
        $usesNodeShim = $ToolStatus.BinPath -like '*.cmd' -or $ToolStatus.BinPath -like '*.ps1' -or $ToolStatus.BinPath -like '*.js'
        if ($usesNodeShim -and -not [string]::IsNullOrWhiteSpace($Root)) {
            $nodeStatus = Get-PortableToolStatus -Root $Root -Manifest ([pscustomobject]@{
                tools = @(
                    [pscustomobject]@{
                        name = 'node'
                        kind = 'runtime'
                        basePath = 'apps/node'
                        required = $true
                        binPaths = @('node.exe')
                    }
                )
            }) | Select-Object -First 1

            if ($nodeStatus -and $nodeStatus.BinPath) {
                Add-PortablePathEntry -PathEntry (Split-Path -Path $nodeStatus.BinPath -Parent)
            }
        }

        Push-Location -LiteralPath $workingDirectory
        $locationPushed = $true
        & $ToolStatus.BinPath @Arguments
        return $LASTEXITCODE
    } finally {
        if ($locationPushed) {
            Pop-Location
        }
        Restore-PortableToolEnvironment -OriginalEnv $originalEnv
        $env:PATH = $originalPath
    }
}

Export-ModuleMember -Function Resolve-PortableKitRoot, Ensure-PortableKitDirectory, `
    Get-PortableKitDefaultConfig, Import-PortableKitConfig, Get-PortableToolManifest, `
    Add-PortablePathEntry, Resolve-ManifestPath, Get-PortableToolStatus, Get-PortableAiStatus, `
    Get-PortableToolWorkingDirectory, Set-PortableToolEnvironment, Restore-PortableToolEnvironment, `
    Export-PortableBootstrapState, Get-PortableJsonFile, Save-PortableDownload, `
    Install-PortableArchive, Get-PortableNpmCommand, Install-PortableNpmPackage, `
    Invoke-PortableToolCommand, Clear-PortableCache, Clear-PathCache
