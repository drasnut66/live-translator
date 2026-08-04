#!/bin/sh

set -eu

PROJECT_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
MODEL_DIR=${LIVE_TRANSLATOR_MODEL_DIR:-"$PROJECT_ROOT/models"}

mkdir -p "$MODEL_DIR"

download() {
    url=$1
    destination=$2
    checksum=$3

    if [ -f "$destination" ]; then
        printf 'Checking %s\n' "$destination"
    else
        printf 'Downloading %s\n' "$destination"
        curl -fL --retry 3 --retry-delay 2 -o "$destination" "$url"
    fi

    actual=$(shasum -a 256 "$destination" | awk '{print $1}')
    if [ "$actual" != "$checksum" ]; then
        printf 'Checksum mismatch for %s\nExpected: %s\nActual:   %s\n' "$destination" "$checksum" "$actual" >&2
        exit 1
    fi
}

# Required: whisper-small is the default streaming model. It reaches 1.0x realtime
# on Apple Silicon while keeping accurate Japanese output.
download \
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-small.bin" \
    "$MODEL_DIR/ggml-small.bin" \
    "1be3a9b2063867b937e64e2ec7483364a79917e157fa98c5d94b5c1fffea987b"

# Optional: large-v3-turbo for higher-accuracy offline runs. Streaming at 0.49x
# realtime on Apple Silicon, so it is not the default.
download \
    "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo-q5_0.bin" \
    "$MODEL_DIR/ggml-large-v3-turbo-q5_0.bin" \
    "394221709cd5ad1f40c46e6031ca61bce88931e6e088c188294c6d5a55ffa7e2"

download \
    "https://huggingface.co/ggml-org/whisper-vad/resolve/main/ggml-silero-v6.2.0.bin" \
    "$MODEL_DIR/ggml-silero-v6.2.0.bin" \
    "2aa269b785eeb53a82983a20501ddf7c1d9c48e33ab63a41391ac6c9f7fb6987"

# Speaker diarization models (speakrs). Together about 34 MB; used by the
# optional speaker-turn labeling, never on the ASR critical path.
SPEAKR_BASE="https://huggingface.co/avencera/speakrs-models/resolve/main"
download "$SPEAKR_BASE/segmentation-3.0.onnx" "$MODEL_DIR/speakrs/segmentation-3.0.onnx" \
    "038b971741ed623af9773ecafdefa4b7bc523520099c2a68f8568b24189e8ad9"
download "$SPEAKR_BASE/wespeaker-voxceleb-resnet34.onnx" "$MODEL_DIR/speakrs/wespeaker-voxceleb-resnet34.onnx" \
    "203a4c67112167580ab1fcb62f4568c633499fb283805890aebe1c48564fcc0f"
download "$SPEAKR_BASE/plda_lda.npy" "$MODEL_DIR/speakrs/plda_lda.npy" \
    "e20c9b012bebd1aabda5a38a127e63a43cf35debdc502715fc143e2fb6bc3c4b"
download "$SPEAKR_BASE/plda_tr.npy" "$MODEL_DIR/speakrs/plda_tr.npy" \
    "e700b68cb319de3fafb5fa093eb9222c23c447084741f8d3a533640d425510ee"
download "$SPEAKR_BASE/plda_mu.npy" "$MODEL_DIR/speakrs/plda_mu.npy" \
    "d286d48acf99bbc1ed1502fed0a3e361ae5626ce1870c8be9f7397c5e47886c6"
download "$SPEAKR_BASE/plda_phi.npy" "$MODEL_DIR/speakrs/plda_phi.npy" \
    "1f403ada4969a5897b24912e5c25b81ed446b3a513a90135bef3c5c885992677"
download "$SPEAKR_BASE/plda_psi.npy" "$MODEL_DIR/speakrs/plda_psi.npy" \
    "d7128c9ed2f28a9781971805131129f077c04f948e2df12e52dcdb99f2b4e5f5"
download "$SPEAKR_BASE/plda_mean1.npy" "$MODEL_DIR/speakrs/plda_mean1.npy" \
    "e424c0c352182aa8e0f555dec1f3b30e29a20b9ed6b25d339f112af92e51e36f"
download "$SPEAKR_BASE/plda_mean2.npy" "$MODEL_DIR/speakrs/plda_mean2.npy" \
    "6f6fb708a2037197b5b84ffeaa8f140cb878088fbecd6ab042ad26a7691bd2cf"

# Offline translation model (Tencent Hy-MT1.5, Q4_K_M). 1.1 GB; used by the
# "local" translation provider for fully offline Japanese-to-Chinese.
download \
    "https://huggingface.co/tencent/HY-MT1.5-1.8B-GGUF/resolve/main/HY-MT1.5-1.8B-Q4_K_M.gguf" \
    "$MODEL_DIR/hymt/HY-MT1.5-1.8B-Q4_K_M.gguf" \
    "4383ac0c3c8e476de98ff979c2a3f069f8c4fb385e7860cf2d28da896cc477c7"

printf 'Models are ready in %s\n' "$MODEL_DIR"
