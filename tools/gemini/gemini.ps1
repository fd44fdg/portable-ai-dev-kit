[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Args
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'
$runner = Join-Path -Path $PSScriptRoot -ChildPath 'gemini-run.cmd'
& $runner @Args
exit $LASTEXITCODE
