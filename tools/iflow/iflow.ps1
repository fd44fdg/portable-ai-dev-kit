[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Args
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$usbRoot = Split-Path -Path (Split-Path -Path (Split-Path -Path $PSCommandPath -Parent) -Parent) -Parent
$dispatcher = Join-Path -Path $usbRoot -ChildPath 'scripts\ai-tool.ps1'
& $dispatcher -Tool 'iflow' -Action 'run' @Args
exit $LASTEXITCODE
