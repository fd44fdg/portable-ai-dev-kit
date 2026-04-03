[CmdletBinding()]
param(
    [Parameter(ValueFromRemainingArguments = $true)]
    [string[]]$Args
)

Set-StrictMode -Version Latest
$ErrorActionPreference = 'Stop'

$ScriptDir = Split-Path -Parent $MyInvocation.MyCommand.Path
$RootDir = (Get-Item $ScriptDir).Parent.Parent.FullName
$Msys2Path = Join-Path $RootDir 'apps\msys64'

if (-not (Test-Path (Join-Path $Msys2Path 'usr\bin\bash.exe'))) {
    Write-Error "MSYS2 not found at $Msys2Path. Please install MSYS2 and place it in the apps/msys64 folder."
    exit 1
}

$Msys2Bin = Join-Path $Msys2Path 'usr\bin'
$Msys2Home = Join-Path $RootDir 'state\home'

if (-not (Test-Path $Msys2Home)) {
    New-Item -ItemType Directory -Path $Msys2Home -Force | Out-Null
}

$Env:PATH = "$Msys2Bin;$Env:PATH"
$Env:HOME = $Msys2Home
$Env:MSYSTEM = 'MINGW64'

$DriveLetter = (Get-Location).Drive.Name
$DrivePath = "/$DriveLetter/" + (Get-Location).Path.Substring(1).Replace('\', '/')
if ($DrivePath -match '^/[A-Z]:/$') {
    $DrivePath = $DrivePath.TrimEnd('/')
}

$ScriptContent = @"
cd "$DrivePath"
$($Args -join ' ')
"@

$TempScript = [System.IO.Path]::GetTempFileName() + '.sh'
Set-Content -Path $TempScript -Value $ScriptContent -Encoding UTF8

try {
    & (Join-Path $Msys2Bin 'bash.exe') -l $TempScript
} finally {
    if (Test-Path $TempScript) {
        Remove-Item $TempScript -Force
    }
}