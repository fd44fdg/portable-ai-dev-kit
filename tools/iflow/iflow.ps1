[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Args
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$runner = Join-Path -Path $PSScriptRoot -ChildPath 'iflow-msys2.cmd'
& $runner @Args
exit $LASTEXITCODE
