# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Builds a TreeDock release into dist/: the installer, the portable ZIP and SHA256SUMS.txt.
# Used by .github/workflows/release.yml; also runs locally (see RELEASING.md).
#
#   pwsh tools/build_release.ps1 -Version 0.1.0 [-Arch x64] [-SkipInstaller]
param(
    [Parameter(Mandatory)] [string] $Version,
    [ValidateSet('x64')] [string] $Arch = 'x64',
    [switch] $SkipInstaller
)
$ErrorActionPreference = 'Stop'
$root = Split-Path -Parent $PSScriptRoot
$dist = Join-Path $root 'dist'
$name = "TreeDock-$Version-$Arch"
$app = Join-Path $dist $name

function Invoke-Native([string] $what, [scriptblock] $cmd) {
    Write-Host "== $what"
    & $cmd
    if ($LASTEXITCODE -ne 0) { throw "$what failed (exit $LASTEXITCODE)" }
}

$current = & python -I (Join-Path $root 'tools/version.py') current
if ($LASTEXITCODE -ne 0) { throw 'version check failed' }
if ($current -ne $Version) { throw "source version is $current, not $Version (python tools/version.py set $Version)" }

if (Test-Path $dist) { Remove-Item -Recurse -Force $dist }
New-Item -ItemType Directory -Force $dist | Out-Null

Push-Location (Join-Path $root 'engine')
try {
    Invoke-Native 'Build engine and CLI' { cargo build --release --locked -p treedock-ffi -p treedock-cli }
} finally { Pop-Location }

Invoke-Native 'Publish app (Native AOT)' {
    dotnet publish (Join-Path $root 'app/TreeDock.App/TreeDock.App.csproj') -c Release `
        -p:Platform=$Arch -p:Version=$Version -p:SkipEngineBuild=true -o $app
}
Copy-Item (Join-Path $root 'engine/target/release/treedock-cli.exe') $app
foreach ($f in 'TreeDock.exe', 'treedock_engine.dll', 'treedock-cli.exe', 'LICENSE.md', 'NOTICE', 'THIRD_PARTY_NOTICES.md') {
    if (-not (Test-Path (Join-Path $app $f))) { throw "publish output is missing $f" }
}

Write-Host '== Portable ZIP'
Compress-Archive -Path $app -DestinationPath (Join-Path $dist "$name-portable.zip") -CompressionLevel Optimal

if (-not $SkipInstaller) {
    $iscc = @("${env:ProgramFiles(x86)}\Inno Setup 6\ISCC.exe", "$env:ProgramFiles\Inno Setup 6\ISCC.exe",
              "$env:LOCALAPPDATA\Programs\Inno Setup 6\ISCC.exe") | Where-Object { Test-Path $_ } | Select-Object -First 1
    if (-not $iscc) { throw 'Inno Setup 6 (ISCC.exe) not found; install it or pass -SkipInstaller' }
    Invoke-Native 'Installer' {
        & $iscc /Q "/DAppVersion=$Version" "/DSourceDir=$app" "/DOutputDir=$dist" "/DArch=$Arch" (Join-Path $root 'installer/TreeDock.iss')
    }
}

Remove-Item -Recurse -Force $app
Write-Host '== Checksums'
$sums = Get-ChildItem $dist -File | Sort-Object Name | ForEach-Object {
    '{0}  {1}' -f (Get-FileHash $_.FullName -Algorithm SHA256).Hash.ToLowerInvariant(), $_.Name
}
[IO.File]::WriteAllLines((Join-Path $dist 'SHA256SUMS.txt'), [string[]] $sums)
Get-ChildItem $dist | Format-Table Name, Length
