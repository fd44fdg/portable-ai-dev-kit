[CmdletBinding(PositionalBinding = $false)]
param(
    [Parameter(Mandatory = $true)]
    [string]$Tool,

    [ValidateSet('status', 'login', 'run')]
    [string]$Action = 'run',

    [string]$UsbRoot,

    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Args
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path -Path $PSScriptRoot -ChildPath 'portable-kit.psm1') -Force -DisableNameChecking

if (-not $UsbRoot) {
    $UsbRoot = Split-Path -Path $PSScriptRoot -Parent
}

$manifest = Get-PortableToolManifest -ManifestPath (Join-Path -Path $UsbRoot -ChildPath 'config\tool-manifest.json')
$root = Resolve-PortableKitRoot -Path $UsbRoot
$toolStatus = Get-PortableToolStatus -Root $root -Manifest $manifest | Where-Object { $_.Name -eq $Tool } | Select-Object -First 1

if (-not $toolStatus) {
    throw "Unknown tool: $Tool"
}

function Start-InteractiveLoginWindow {
    param(
        [string]$RootPath,
        [pscustomobject]$ToolStatus,
        [string[]]$Arguments = @()
    )

    $normalizedRoot = if ([string]::IsNullOrWhiteSpace($RootPath)) {
        $RootPath
    } else {
        $RootPath.TrimEnd('\')
    }
    $workspacePath = Join-Path -Path $RootPath -ChildPath 'workspace'
    if (-not (Test-Path -LiteralPath $workspacePath)) {
        $workspacePath = $normalizedRoot
    }

    $nodePath = Join-Path -Path $normalizedRoot -ChildPath 'apps\node'
    $toolPath = $ToolStatus.BinPath
    $escapedArguments = @($Arguments | ForEach-Object { '"{0}"' -f $_.Replace('"', '\"') }) -join ' '
    $command = 'setlocal && cd /d "{0}" && set "HOME={1}\state\home" && set "USERPROFILE={1}\state\home" && set "APPDATA={1}\state\appdata" && set "LOCALAPPDATA={1}\state\localappdata" && set "XDG_CONFIG_HOME={1}\state\xdg\config" && set "XDG_CACHE_HOME={1}\state\xdg\cache" && set "XDG_STATE_HOME={1}\state\xdg\state" && set "PATH={2};%PATH%" && call "{3}" {4} && echo. && echo codex exited with code %ERRORLEVEL%.' -f $workspacePath, $normalizedRoot, $nodePath, $toolPath, $escapedArguments
    $argumentList = @(
        '/k'
        $command
    )

    Start-Process -FilePath 'cmd.exe' -ArgumentList $argumentList -WorkingDirectory $workspacePath | Out-Null
}

switch ($Action) {
    'status' {
        $aiStatus = Get-PortableAiStatus -ToolStatus $toolStatus
        $aiStatus | ConvertTo-Json -Depth 4
        break
    }
    'login' {
        if (-not $toolStatus.BinPath) {
            Write-Host "$Tool is not installed yet." -ForegroundColor Yellow
            Write-Host $toolStatus.InstallHint -ForegroundColor Yellow
            exit 1
        }

        if ($Tool -eq 'codex') {
            $loginArguments = if ($toolStatus.LoginArgs -and $toolStatus.LoginArgs.Count -gt 0) {
                @($toolStatus.LoginArgs)
            } else {
                @('login')
            }

            Start-InteractiveLoginWindow -RootPath $root -ToolStatus $toolStatus -Arguments $loginArguments
            Write-Host ""
            Write-Host "Codex login opened in a new console window." -ForegroundColor Cyan
            exit 0
        }

        $exitCode = 0
        if (-not $toolStatus.LoginArgs -or $toolStatus.LoginArgs.Count -eq 0) {
            $exitCode = Invoke-PortableToolCommand -ToolStatus $toolStatus -Root $root
        } else {
            $exitCode = Invoke-PortableToolCommand -ToolStatus $toolStatus -Arguments @($toolStatus.LoginArgs) -Root $root
        }

        if ($exitCode -eq 0) {
            Write-Host ""
            Write-Host "Start CLI with:" -ForegroundColor Cyan
            if ($toolStatus.LaunchPath) {
                Write-Host ("  {0}" -f $toolStatus.LaunchPath) -ForegroundColor Green
            } else {
                Write-Host ("  powershell -File {0} -UsbRoot {1} -Tool {2} -Action run" -f (Join-Path -Path $PSScriptRoot -ChildPath 'ai-tool.ps1'), $root, $Tool) -ForegroundColor Green
            }
        }

        exit $exitCode
    }
    'run' {
        if (-not $toolStatus.BinPath) {
            Write-Host "$Tool is not installed yet." -ForegroundColor Yellow
            Write-Host $toolStatus.InstallHint -ForegroundColor Yellow
            exit 1
        }

        exit (Invoke-PortableToolCommand -ToolStatus $toolStatus -Arguments $Args -Root $root)
    }
}
