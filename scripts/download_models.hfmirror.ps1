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
    "https://hf-mirror.com/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin" `
    (Join-Path $modelDir "ggml-large-v3-turbo-q5_0.bin") `
    "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2"
# Optional higher-fidelity Whisper models (larger downloads; not required).
# - ggml-large-v3-turbo-q8_0.bin: turbo at q8_0, ~834 MB. Slightly more
#   faithful than q5_0 at the same streaming speed (measured: avg decode
#   297 ms vs 291 ms at an 8 s window on RTX 3060 Ti; quality is a wash).
# - ggml-large-v3-q5_0.bin: full-size large-v3 at q5_0, ~1 GB. More accurate
#   offline, but ~2.5x slower per decode with 3-4 s spikes; NOT recommended
#   for live streaming.
# - ggml-large-v3.bin: full-size f16, ~3 GB. Convert to q8_0 yourself with
#   whisper.cpp's whisper-quantize (see work/optimization-plan.md), because
#   the official repo does not ship full-size q8_0.
Get-VerifiedModel `
    "$base/ggml-large-v3-turbo-q8_0.bin" `
    (Join-Path $modelDir "ggml-large-v3-turbo-q8_0.bin") `
    "317eb69c11673c9de1e1f0d459b253999804ec71ac4c23c17ecf5fbe24e259a1"
Get-VerifiedModel `
    "$base/ggml-large-v3-q5_0.bin" `
    (Join-Path $modelDir "ggml-large-v3-q5_0.bin") `
    "d75795ecff3f83b5faa89d1900604ad8c780abd5739fae406de19f23ecd98ad1"
Get-VerifiedModel `
    "$base/ggml-large-v3.bin" `
    (Join-Path $modelDir "ggml-large-v3.bin") `
    "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2"

Get-VerifiedModel `
    "https://hf-mirror.com/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin" `
    (Join-Path $modelDir "ggml-silero-v6.2.0.bin") `
    "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987"

# Speaker diarization models (speakrs), ONNX CPU path.
$speakrBase = "https://hf-mirror.com/avencera/speakrs-models/resolve/main"
Get-VerifiedModel "$speakrBase/segmentation-3.0.onnx" (Join-Path $modelDir "speakrs/segmentation-3.0.onnx") "038b971741ed623af9773ecafdefa4b7bc523520099c2a68f8568b24189e8ad9"
Get-VerifiedModel "$speakrBase/wespeaker-voxceleb-resnet34.onnx" (Join-Path $modelDir "speakrs/wespeaker-voxceleb-resnet34.onnx") "203a4c67112167580ab1fcb62f4568c633499fb283805890aebe1c48564fcc0f"
Get-VerifiedModel "$speakrBase/plda_lda.npy" (Join-Path $modelDir "speakrs/plda_lda.npy") "e20c9b012bebd1aabda5a38a127e63a43cf35debdc502715fc143e2fb6bc3c4b"
Get-VerifiedModel "$speakrBase/plda_tr.npy" (Join-Path $modelDir "speakrs/plda_tr.npy") "e700b68cb319de3fafb5fa093eb9222c23c447084741f8d3a533640d425510ee"
Get-VerifiedModel "$speakrBase/plda_mu.npy" (Join-Path $modelDir "speakrs/plda_mu.npy") "d286d48acf99bbc1ed1502fed0a3e361ae5626ce1870c8be9f7397c5e47886c6"
Get-VerifiedModel "$speakrBase/plda_phi.npy" (Join-Path $modelDir "speakrs/plda_phi.npy") "1f403ada4969a5897b24912e5c25b81ed446b3a513a90135bef3c5c885992677"
Get-VerifiedModel "$speakrBase/plda_psi.npy" (Join-Path $modelDir "speakrs/plda_psi.npy") "d7128c9ed2f28a9781971805131129f077c04f948e2df12e52dcdb99f2b4e5f5"
Get-VerifiedModel "$speakrBase/plda_mean1.npy" (Join-Path $modelDir "speakrs/plda_mean1.npy") "e424c0c352182aa8e0f555dec1f3b30e29a20b9ed6b25d339f112af92e51e36f"
Get-VerifiedModel "$speakrBase/plda_mean2.npy" (Join-Path $modelDir "speakrs/plda_mean2.npy") "6f6fb708a2037197b5b84ffeaa8f140cb878088fbecd6ab042ad26a7691bd2cf"

# Offline translation model (Hy-MT1.5 Q4_K_M). NOTE: the local provider also
# needs a llama-server.exe next to the model; download it separately from a
# llama.cpp release for Windows.
Get-VerifiedModel `
    "https://hf-mirror.com/tencent/HY-MT1.5-1.8B-GGUF/resolve/main/HY-MT1.5-1.8B-Q4_K_M.gguf" `
    (Join-Path $modelDir "hymt/HY-MT1.5-1.8B-Q4_K_M.gguf") `
    "4383ac0c3c8e476de98ff979c2a3f069f8c4fb385e7860cf2d28da896cc477c7"

Write-Host "Models are ready in $modelDir"

