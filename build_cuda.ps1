# Build the CUDA-enabled release binary under D:\myapp\target (pure-ASCII path).
# Usage:  powershell -ExecutionPolicy Bypass -File build_cuda.ps1 [--release|--check|--tauri]
#
# The `cuda` feature is OFF by default (so CI compiles a CPU build); these modes
# explicitly enable it. `-Mode check` does NOT enable cuda (matches CI).
#
# Prereqs on this machine:
#   - CUDA Toolkit 13.1 at D:\Environment\CUDA\v13.1 (machine CUDA_PATH already set)
#   - MSVC 14.44 toolset (vcvars_ver=14.44) inside VS 18 Community
#   - ninja + ffmpeg on PATH (see $extra_path below)
param(
    [ValidateSet('release','check','tauri')]
    [string]$Mode = 'release'
)

$ErrorActionPreference = 'Stop'
$here = Split-Path -Parent $MyInvocation.MyCommand.Path   # live-translator/
$root = Split-Path -Parent $here                          # project root
$tauri = Join-Path $here 'desktop\src-tauri'
Write-Host "here=$here  root=$root  tauri=$tauri"

# --- MSVC toolchain (14.44, required by nvcc 13.x) ------------------------------
$vcvars = 'D:\Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvars64.bat'
cmd /c "call `"$vcvars`" -vcvars_ver=14.44 >nul 2>&1 && set" | ForEach-Object {
    if ($_ -match '^([^=]+)=(.*)$') { Set-Item -Path "Env:$($matches[1])" -Value $matches[2] }
}

# --- CUDA environment -----------------------------------------------------------
$env:CUDA_PATH   = 'D:\Environment\CUDA\v13.1'
$env:CUDA_HOME   = $env:CUDA_PATH
$env:CUDACXX     = "$env:CUDA_PATH\bin\nvcc.exe"
$env:CMAKE_GENERATOR = 'Ninja'
$extra_path = @(
    "$env:CUDA_PATH\bin\x64",
    "$env:CUDA_PATH\bin",
    'C:\Program Files\Ninja',
    'D:\Environment\ffmpeg\bin'
)
$env:PATH = ($extra_path + $env:PATH) -join ';'

# --- Build dir (pure-ASCII path, holds the 8 GB cache) ---------------------------
$env:CARGO_TARGET_DIR = 'D:\myapp\target'

Write-Host "CARGO_TARGET_DIR=$env:CARGO_TARGET_DIR"
Write-Host "CUDA_PATH=$env:CUDA_PATH  nvcc=$env:CUDACXX"
Push-Location $tauri
try {
    switch ($Mode) {
        'release' { cargo build --release --features cuda }
        'check'   { cargo check --all-targets }
        'tauri'   { npm run tauri build -- --features cuda }
    }
    if ($LASTEXITCODE -ne 0) { throw "cargo failed with exit $LASTEXITCODE" }
    Write-Host "BUILD_OK mode=$Mode exit=$LASTEXITCODE"
}
finally {
    Pop-Location
}
