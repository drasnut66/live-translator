#!/bin/sh

set -eu

PROJECT_ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
REPO_ROOT=$(CDPATH= cd -- "$PROJECT_ROOT/.." && pwd)

INPUT=${1:-"$REPO_ROOT/1.flv"}
OUTPUT_PREFIX=${2:-"$PROJECT_ROOT/work/test-transcription"}
MODEL=${WHISPER_MODEL:-"$PROJECT_ROOT/models/ggml-large-v3-turbo-q5_0.bin"}
VAD_MODEL=${WHISPER_VAD_MODEL:-"$PROJECT_ROOT/models/ggml-silero-v6.2.0.bin"}
WHISPER_BIN=${WHISPER_BIN:-"$PROJECT_ROOT/build/whisper.cpp/bin/whisper-cli"}

if [ ! -f "$INPUT" ]; then
    printf 'Input file does not exist: %s\n' "$INPUT" >&2
    exit 1
fi
if [ ! -x "$WHISPER_BIN" ]; then
    printf 'whisper-cli is not built: %s\n' "$WHISPER_BIN" >&2
    exit 1
fi
if [ ! -f "$MODEL" ] || [ ! -f "$VAD_MODEL" ]; then
    printf 'Model files are missing. See work/handoff.md.\n' >&2
    exit 1
fi

mkdir -p "$(dirname -- "$OUTPUT_PREFIX")"
TEMP_WAV=$(mktemp "${TMPDIR:-/tmp}/live-translator-audio.XXXXXX")
cleanup() {
    rm -f "$TEMP_WAV"
}
trap cleanup EXIT INT TERM

ffmpeg -hide_banner -loglevel error -y \
    -i "$INPUT" -map 0:a:0 -vn -ac 1 -ar 16000 -c:a pcm_s16le -f wav "$TEMP_WAV"

"$WHISPER_BIN" \
    -m "$MODEL" \
    -vm "$VAD_MODEL" \
    --vad \
    -l ja \
    -t "${WHISPER_THREADS:-8}" \
    -bs 3 -bo 3 \
    -otxt -osrt -ojf \
    -of "$OUTPUT_PREFIX" \
    -f "$TEMP_WAV"

printf 'Outputs: %s.txt, %s.srt, %s.json\n' "$OUTPUT_PREFIX" "$OUTPUT_PREFIX" "$OUTPUT_PREFIX"
