$ErrorActionPreference = "Stop"

$projectRoot = Split-Path -Parent $PSScriptRoot
$modelDir = if ($env:LIVE_TRANSLATOR_MODEL_DIR) {
    $env:LIVE_TRANSLATOR_MODEL_DIR
} else {
    Join-Path $projectRoot "models"
}

New-Item -ItemType Directory -Force -Path $modelDir | Out-Null

function Get-VerifiedModel {
    param(
        [string]$Url,
        [string]$Destination,
        [string]$Sha256
    )

    if (-not (Test-Path -LiteralPath $Destination)) {
        Write-Host "Downloading $Destination"
        Invoke-WebRequest -Uri $Url -OutFile $Destination
    }

    $actual = (Get-FileHash -Algorithm SHA256 -LiteralPath $Destination).Hash.ToLowerInvariant()
    if ($actual -ne $Sha256) {
        throw "Checksum mismatch for $Destination. Expected $Sha256, got $actual"
    }
}

Get-VerifiedModel `
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin" `
    (Join-Path $modelDir "ggml-large-v3-turbo-q5_0.bin") `
    "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2"

Get-VerifiedModel `
    "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin" `
    (Join-Path $modelDir "ggml-silero-v6.2.0.bin") `
    "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987"

Write-Host "Models are ready in $modelDir"
