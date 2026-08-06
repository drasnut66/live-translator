# Build a single-file self-extracting installer for the lite distribution.
# Uses Windows built-in IExpress (no third-party tools). The resulting exe
# extracts the app + CUDA/VC runtime to %LOCALAPPDATA%\LiveTranslator and
# launches it; speech/translation models are downloaded in-app on first run.
#
# Prereq: run make_portable.ps1 -Lite first (creates dist\LiveTranslator-portable).
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\make_single_exe.ps1

param(
    [string]$Version = "0.1.0"
)

$ErrorActionPreference = "Stop"

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Split-Path -Parent $here
$stage = Join-Path $root "dist\LiveTranslator-portable"
if (-not (Test-Path -LiteralPath (Join-Path $stage "LiveTranslator.exe"))) {
    throw "lite stage missing; run make_portable.ps1 -Lite first"
}

# Keep the SFX payload ASCII-only (IExpress/makecab is unreliable with CJK names).
$payload = @(
    "LiveTranslator.exe",
    "cublas64_13.dll",
    "cublasLt64_13.dll",
    "cudart64_13.dll",
    "msvcp140.dll",
    "msvcp140_1.dll",
    "msvcp140_2.dll",
    "vcruntime140.dll",
    "vcruntime140_1.dll",
    "concrt140.dll"
)

$sfx = Join-Path $root "dist\_sfx_stage"
if (Test-Path -LiteralPath $sfx) { Remove-Item -LiteralPath $sfx -Recurse -Force }
New-Item -ItemType Directory -Path $sfx -Force | Out-Null
foreach ($f in $payload) {
    Copy-Item -LiteralPath (Join-Path $stage $f) -Destination (Join-Path $sfx $f)
}

$install = @'
@echo off
setlocal
set "DEST=%LOCALAPPDATA%\LiveTranslator"
if not exist "%DEST%" mkdir "%DEST%"
xcopy /e /y /i "%~dp0." "%DEST%" >nul
start "" "%DEST%\LiveTranslator.exe"
endlocal
'@
[System.IO.File]::WriteAllText((Join-Path $sfx "install.cmd"), $install, (New-Object System.Text.ASCIIEncoding))

$readme = @'
Live Translator - lite single-file build
========================================

This package installs Live Translator to %LOCALAPPDATA%\LiveTranslator and
starts it. Speech and translation models are NOT included: on first launch
open Settings (gear) -> Model Files -> "Download ASR models" (~1 GB), and
optionally "Download offline translation" (~1.1 GB, only needed for the
local Hy-MT provider).

Requirements: Windows 10/11 x64, NVIDIA GPU with a recent driver (CUDA 13
runtime needs a recent driver). No CUDA Toolkit install is required.
'@
[System.IO.File]::WriteAllText((Join-Path $sfx "README.txt"), $readme, (New-Object System.Text.ASCIIEncoding))

# IExpress SED (silent, max compression).
$sedFile = Join-Path $sfx "LiveTranslator-lite-setup.sed"
$files = Get-ChildItem -LiteralPath $sfx -File | Select-Object -ExpandProperty Name
$sed = @(
    "[Version]"
    "Class=IEXPRESS"
    "SEDVersion=3"
    "[Options]"
    "PackagePurpose=InstallApp"
    "ShowInstallProgramWindow=0"
    "HideExtractAnimation=1"
    "UseLongFileName=1"
    "InsideCompressed=0"
    "Cabinets=1"
    "CompressedType=Maximum"
    "MaxDiskSize1=0"
    "MaxDiskSize2=0"
    "FriendlyName=Live Translator Lite"
    "AppLaunched=install.cmd"
    "PostInstallCmd=<None>"
    "AdminQuietInstCmd="
    "UserQuietInstCmd="
    "SourceFiles=SourceFiles"
    "[Strings]"
    "Title=Live Translator Lite"
    "Subject=Live Translator Lite setup"
    "CatalogFile="
    "CustomUninstall="
    "[SourceFiles]"
    ("SourceFiles0=" + $sfx + "\")
    "[SourceFiles0]"
)
for ($i = 0; $i -lt $files.Count; $i++) {
    $sed += ("%FILE{0}%={1}" -f $i, $files[$i])
}
[System.IO.File]::WriteAllText($sedFile, ($sed -join "`r`n"), (New-Object System.Text.ASCIIEncoding))

Write-Host "==> running iexpress"
$iexpress = Join-Path $env:WINDIR "System32\iexpress.exe"
$prev = Get-ChildItem -LiteralPath $sfx -Filter "*.exe" -File | Select-Object -ExpandProperty FullName
& $iexpress /N /Q $sedFile
if ($LASTEXITCODE -ne 0) { throw "iexpress failed with exit $LASTEXITCODE" }
Start-Sleep -Seconds 1
$out = Get-ChildItem -LiteralPath $sfx -Filter "*.exe" -File | Where-Object { $prev -notcontains $_.FullName } | Sort-Object LastWriteTime -Descending | Select-Object -First 1
if (-not $out) { throw "iexpress produced no output exe" }
$final = Join-Path $root ("dist\LiveTranslator-lite-setup-v" + $Version + ".exe")
Copy-Item -LiteralPath $out.FullName -Destination $final -Force
Write-Host "DONE: $final"
Write-Host "  size: $([math]::Round((Get-Item $final).Length / 1MB, 1)) MB"