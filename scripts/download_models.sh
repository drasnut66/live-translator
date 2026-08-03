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

printf 'Models are ready in %s\n' "$MODEL_DIR"
