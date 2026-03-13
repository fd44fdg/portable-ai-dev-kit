[CmdletBinding()]
param(
    [Parameter(Mandatory = $true)]
    [string]$Name,

    [string]$PackageName = '',

    [string]$LoginHint = 'interactive'
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
Import-Module (Join-Path -Path $PSScriptRoot -ChildPath 'portable-kit.psm1') -Force -DisableNameChecking

$usbRoot = Split-Path -Path $PSScriptRoot -Parent
$toolKey = $Name.ToLowerInvariant()
$toolRoot = Join-Path -Path $usbRoot -ChildPath ("tools\" + $toolKey)
if (-not (Test-Path -LiteralPath $toolRoot)) {
    Ensure-PortableKitDirectory -Path $toolRoot
}

if (-not (Test-Path -LiteralPath $toolRoot)) {
    throw "Unable to create tool directory: $toolRoot"
}

$wrapperCmdPath = Join-Path -Path $toolRoot -ChildPath ($toolKey + '.cmd')
$wrapperPsPath = Join-Path -Path $toolRoot -ChildPath ($toolKey + '.ps1')
$runnerCmdPath = Join-Path -Path $toolRoot -ChildPath ($toolKey + '-run.cmd')

$cmdContent = @"
@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
call "%SCRIPT_DIR%$toolKey-run.cmd" %*
set "EXIT_CODE=%ERRORLEVEL%"
endlocal & exit /b %EXIT_CODE%
"@

$runnerContent = @"
@echo off
setlocal
set "SCRIPT_DIR=%~dp0"
set "TARGET_CMD=%SCRIPT_DIR%node_modules\.bin\$toolKey.cmd"

if exist "%TARGET_CMD%" (
  call "%TARGET_CMD%" %*
  set "EXIT_CODE=%ERRORLEVEL%"
  endlocal & exit /b %EXIT_CODE%
)

echo $toolKey is not installed yet.
endlocal & exit /b 1
"@

$psContent = @"
[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = `$true)]
    [string[]]`$Args
)

Set-StrictMode -Version Latest
`$ErrorActionPreference = 'Stop'

`$runner = Join-Path -Path `$PSScriptRoot -ChildPath '$toolKey-run.cmd'
& `$runner @Args
exit `$LASTEXITCODE
"@

$cmdContent | Set-Content -LiteralPath $wrapperCmdPath -Encoding ASCII
$runnerContent | Set-Content -LiteralPath $runnerCmdPath -Encoding ASCII
$psContent | Set-Content -LiteralPath $wrapperPsPath -Encoding UTF8

Write-Host ""
Write-Host "Wrapper scaffold created:" -ForegroundColor Green
Write-Host $wrapperCmdPath
Write-Host $runnerCmdPath
Write-Host $wrapperPsPath
Write-Host ""
Write-Host "Add this manifest entry to config\\tool-manifest.json:" -ForegroundColor Cyan
Write-Host (@"
{
  "name": "$toolKey",
  "kind": "ai-cli",
  "required": false,
  "basePath": "tools/$toolKey",
  "archiveName": "",
  "source": "npm",
  "wrapperPaths": [
    "$toolKey.cmd",
    "$toolKey.ps1"
  ],
  "binPaths": [
    "$toolKey-run.cmd",
    "node_modules/.bin/$toolKey.cmd"
  ],
  "loginCheckArgs": [],
  "loginCheckIndicatesAuth": false,
  "loginArgs": [],
  "installHint": "Install $Name CLI into tools/$toolKey."
}
"@)

if ($PackageName) {
    Write-Host ""
    Write-Host "Add this package source entry to config\\package-sources.json:" -ForegroundColor Cyan
    Write-Host (@"
"$toolKey": {
  "type": "npm",
  "packageName": "$PackageName",
  "dependsOn": [
    "node"
  ],
  "postInstall": []
}
"@)
}

Write-Host ""
Write-Host ("Login hint: {0}" -f $LoginHint) -ForegroundColor DarkCyan
