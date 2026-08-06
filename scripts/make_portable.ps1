# Build a portable "download-and-run" distribution of Live Translator.
# The folder carries the app exe + CUDA runtime DLLs + VC runtime + models +
# (optional) ffmpeg, so a fresh Windows machine with an NVIDIA driver can run
# it without installing CUDA Toolkit, Rust, or any other toolchain.
#
# Usage:
#   powershell -ExecutionPolicy Bypass -File scripts\make_portable.ps1
#   powershell -ExecutionPolicy Bypass -File scripts\make_portable.ps1 -IncludeFfmpeg -IncludeAllModels
#   powershell -ExecutionPolicy Bypass -File scripts\make_portable.ps1 -Lite   # no models; app downloads on first run
#
# Output: dist\LiveTranslator-portable\  (folder) and
#         dist\LiveTranslator-portable-v<Version>.zip  (or -lite variant)

param(
    [string]$ExePath = "",
    [string]$ModelDir = "",
    [string]$CudaBin = "D:\Environment\CUDA\v13.1\bin\x64",
    [string]$VcRedistDir = "",
    [string]$FfmpegBin = "",
    [switch]$IncludeFfmpeg,
    [switch]$IncludeAllModels,
    [switch]$Lite,
    [string]$Version = "0.1.0"
)

$ErrorActionPreference = "Stop"

$here = Split-Path -Parent $MyInvocation.MyCommand.Path
$root = Split-Path -Parent $here
if (-not $ExePath) { $ExePath = Join-Path $root "LiveTranslator.exe" }
if (-not $ModelDir) {
    $ModelDir = if ($env:LIVE_TRANSLATOR_MODEL_DIR) { $env:LIVE_TRANSLATOR_MODEL_DIR }
                else { Join-Path $root "models" }
}
if (-not $VcRedistDir) {
    $candidates = Get-ChildItem "D:\Microsoft Visual Studio\18\Community\VC\Redist\MSVC" -Directory -ErrorAction SilentlyContinue |
                  Where-Object { $_.Name -notlike "v*" } |
                  Sort-Object Name -Descending
    foreach ($cand in $candidates) {
        $try = Join-Path $cand.FullName "x64\Microsoft.VC143.CRT"
        if (Test-Path -LiteralPath $try) { $VcRedistDir = $try; break }
    }
}
if (-not $FfmpegBin) {
    $cmd = Get-Command ffmpeg -ErrorAction SilentlyContinue
    if ($cmd) { $FfmpegBin = Split-Path -Parent $cmd.Source }
}

foreach ($req in @(
    @{ P = $ExePath;      What = "app exe" },
    @{ P = $ModelDir;     What = "model directory" },
    @{ P = $CudaBin;      What = "CUDA runtime bin dir" },
    @{ P = $VcRedistDir;  What = "VC runtime CRT dir" }
)) {
    if (-not (Test-Path -LiteralPath $req.P)) { throw "Missing $($req.What): $($req.P)" }
}

$stage = Join-Path $root "dist\LiveTranslator-portable"
if (Test-Path -LiteralPath $stage) { Remove-Item -LiteralPath $stage -Recurse -Force }
New-Item -ItemType Directory -Path $stage -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $stage "models\hymt") -Force | Out-Null
New-Item -ItemType Directory -Path (Join-Path $stage "ffmpeg") -Force | Out-Null

Write-Host "==> app exe"
Copy-Item -LiteralPath $ExePath -Destination (Join-Path $stage "LiveTranslator.exe")

Write-Host "==> CUDA runtime DLLs"
foreach ($dll in @("cublas64_13.dll", "cublasLt64_13.dll", "cudart64_13.dll")) {
    $src = Join-Path $CudaBin $dll
    if (-not (Test-Path -LiteralPath $src)) { throw "CUDA DLL missing: $src" }
    Copy-Item -LiteralPath $src -Destination (Join-Path $stage $dll)
}

Write-Host "==> VC runtime DLLs"
foreach ($dll in @("msvcp140.dll", "msvcp140_1.dll", "msvcp140_2.dll", "vcruntime140.dll", "vcruntime140_1.dll", "concrt140.dll")) {
    $src = Join-Path $VcRedistDir $dll
    if (Test-Path -LiteralPath $src) {
        Copy-Item -LiteralPath $src -Destination (Join-Path $stage $dll)
    }
}

if ($Lite) {
    Write-Host "==> LITE mode: models are NOT bundled; the app downloads them on first run"
} else {
    Write-Host "==> Whisper + VAD models"
    $requiredModels = @("ggml-large-v3-turbo-q5_0.bin", "ggml-small.bin", "ggml-silero-v6.2.0.bin")
    foreach ($m in $requiredModels) {
        $src = Join-Path $ModelDir $m
        if (Test-Path -LiteralPath $src) { Copy-Item -LiteralPath $src -Destination (Join-Path $stage "models\$m") }
    }
    if ($IncludeAllModels) {
        foreach ($m in @("ggml-large-v3-turbo-q8_0.bin", "ggml-large-v3-q5_0.bin", "ggml-large-v3-q8_0.bin", "ggml-large-v3.bin", "ggml-base.bin")) {
            $src = Join-Path $ModelDir $m
            if (Test-Path -LiteralPath $src) { Copy-Item -LiteralPath $src -Destination (Join-Path $stage "models\$m") }
        }
    }

    Write-Host "==> local Hy-MT translation (models/hymt, without cpu-backup)"
    Get-ChildItem -LiteralPath (Join-Path $ModelDir "hymt") -Force |
        Where-Object { $_.Name -ne "cpu-backup" } |
        ForEach-Object { Copy-Item -LiteralPath $_.FullName -Destination (Join-Path $stage "models\hymt\$($_.Name)") -Recurse }
}

if ($IncludeFfmpeg -and $FfmpegBin) {
    Write-Host "==> ffmpeg (for file/URL media playback)"
    Copy-Item -Path (Join-Path $FfmpegBin "*") -Destination (Join-Path $stage "ffmpeg") -Recurse
}

Write-Host "==> launcher + readme"
$launcher = @'
@echo off
setlocal
cd /d "%~dp0"
rem Make the bundled CUDA runtime and ffmpeg resolvable for this process tree.
set "PATH=%~dp0;%~dp0ffmpeg\bin;%PATH%"
start "" "%~dp0LiveTranslator.exe"
endlocal
'@
[System.IO.File]::WriteAllText((Join-Path $stage "启动直播翻译.cmd"), $launcher, (New-Object System.Text.ASCIIEncoding))

$readmeFull = @'
Live Translator 便携版
=====================

一、系统要求
  - Windows 10/11 64 位
  - NVIDIA 独立显卡 + 较新的 NVIDIA 驱动（nvidia-smi 能跑通即可；CUDA 13 运行时
    需要较新的驱动，驱动太旧会报 CUDA 初始化失败，请更新驱动）
  - 内存建议 8 GB 以上；显存建议 4 GB 以上
  - 本包已内置 CUDA 运行时 DLL 和 VC++ 运行库，无需安装 CUDA Toolkit

二、使用方法
  1. 解压整个文件夹（不要只拷 exe）。
  2. 双击 "启动直播翻译.cmd"（推荐），或直接双击 LiveTranslator.exe。
  3. 首次使用：
     - 设置 -> ASR 模型：默认 ggml-small.bin；建议选 ggml-large-v3-turbo-q5_0.bin
       （切换模型立即生效，无需重启）。
     - 设置 -> 翻译：可选 本地 Hy-MT（离线）/ OpenAI Compatible / DeepL（联网，
       需填入 API Key）。
     - 选择音源：浏览器音频（需配合 Chrome 扩展）/ 系统音频 / 麦克风。
  4. 点击 ▶ 开始识别。

三、目录结构
  LiveTranslator.exe          主程序
  cublas64_13.dll 等           内置 CUDA 运行时（勿删除）
  msvcp140.dll 等              内置 VC++ 运行库（勿删除）
  models\                      语音/翻译模型（勿删除）
  models\hymt\                 本地翻译 llama-server（Hy-MT）
  ffmpeg\                      可选，仅用于播放本地文件/网络流

四、常见问题
  - 双击无反应/闪退：先试 "启动直播翻译.cmd"；仍不行检查 NVIDIA 驱动是否过旧。
  - 提示模型找不到：确认 models 文件夹与本 exe 在同一目录。
  - 本地翻译不可用：确认 models\hymt 文件夹完整（GGUF + llama-server.exe + DLL）。
  - 播放文件/串流报 ffmpeg 错误：使用带 ffmpeg 的完整包，或自行安装 ffmpeg 并加入 PATH。
  - 杀毒软件拦截：首次运行可能被 SmartScreen 提示，选择"仍要运行"。
'@

$readmeLite = @'
Live Translator 轻量版（首次运行联网下载模型）
==========================================

一、系统要求
  - Windows 10/11 64 位
  - NVIDIA 独立显卡 + 较新的 NVIDIA 驱动（CUDA 13 运行时需要较新的驱动）
  - 首次运行需要联网（下载模型约 1 GB；如需离线翻译再加约 1.1 GB）
  - 本包已内置 CUDA 运行时 DLL 和 VC++ 运行库，无需安装 CUDA Toolkit

二、使用方法
  1. 解压整个文件夹。
  2. 双击 "启动直播翻译.cmd"（推荐），或直接双击 LiveTranslator.exe。
  3. 打开设置（齿轮），在"模型文件"区点【下载 ASR 模型】（约 1 GB：Whisper small +
     large-v3-turbo + VAD）。如需离线翻译，再点【下载离线翻译】（约 1.1 GB）。
     下载会显示进度，下载完成后自动校验 SHA-256，失败可重试。
  4. 之后按普通版本使用：选模型、翻译服务、音源，点 ▶ 开始识别。

三、目录结构
  LiveTranslator.exe          主程序
  cublas64_13.dll 等           内置 CUDA 运行时（勿删除）
  msvcp140.dll 等              内置 VC++ 运行库（勿删除）
  models\                      首次运行后自动生成（勿删除）
  ffmpeg\                      可选，仅用于播放本地文件/网络流

四、常见问题
  - 下载失败：检查网络；若在国外/公司网络可设置环境变量
    LIVE_TRANSLATOR_MODEL_MIRROR=https://huggingface.co 后重启再试。
  - 杀毒软件拦截：选择"仍要运行"，或加入白名单。
'@

$readme = if ($Lite) { $readmeLite } else { $readmeFull }
[System.IO.File]::WriteAllText((Join-Path $stage "使用说明.txt"), $readme, (New-Object System.Text.UTF8Encoding($true)))

Write-Host "==> zipping"
$suffix = if ($Lite) { "-lite" } else { "" }
$zip = Join-Path $root "dist\LiveTranslator-portable$suffix-v$Version.zip"
if (Test-Path -LiteralPath $zip) { Remove-Item -LiteralPath $zip -Force }
Compress-Archive -Path (Join-Path $stage "*") -DestinationPath $zip -CompressionLevel Optimal

Write-Host "DONE:"
Write-Host "  folder: $stage"
Write-Host "  zip   : $zip"