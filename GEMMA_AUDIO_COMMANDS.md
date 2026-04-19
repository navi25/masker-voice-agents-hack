# Using gemma-4-e2b-it/audio_encoder for Live Masker Transcribe

## Prerequisites

```bash
# Install models (once)
cactus download openai/whisper-small
cactus download google/gemma-4-E2B-it
```

## Setup: Apple NPU environment (recommended)

```bash
cd platform/masker-core
source scripts/cactus-npu-env.sh
```

This sets `CACTUS_STT_MODEL_PATH`, `CACTUS_DETECTION_MODEL_PATH`, and
`CACTUS_ANE_COMPUTE_UNITS=cpu_and_ne` to enable Apple Neural Engine acceleration
for `audio_encoder.mlpackage`.

## Verify audio_encoder is available

```bash
cargo run --release -p masker-cli -- coreml check-gemma-e2b \
  --gemma-dir "$CACTUS_WEIGHTS_DIR/gemma-4-e2b-it"

# Or manually:
find "$CACTUS_WEIGHTS_DIR/gemma-4-e2b-it" -maxdepth 1 \
  \( -name '*.mlpackage' -o -name '*.mlmodelc' \)
```

Expected output: `[OK] audio_encoder`, `[OK] vision_encoder`, `[OK] model`

---

## masker transcribe (recommended entrypoint)

Privacy-first STT: auto-loads Gemma detection, live redaction updates while speaking.

```bash
# Interactive mic (press Enter to stop)
cargo run --release --features cactus -p masker-cli -- transcribe --interactive

# Fixed 8-second capture from mic
cargo run --release --features cactus -p masker-cli -- transcribe --seconds 8

# From an existing WAV file
cargo run --release --features cactus -p masker-cli -- \
  transcribe --audio-file /path/to/sample.wav
```

---

## masker live — with Gemma audio_encoder as the STT engine

Use `--stt-engine gemma4` to route transcription through `gemma-4-e2b-it/audio_encoder`
instead of Whisper/Parakeet. Gemma reads the WAV directly via the audio encoder.

```bash
# Gemma4 STT + Gemma4 detection (full audio_encoder pipeline)
export CACTUS_GEMMA_STT_MODEL_PATH="$CACTUS_WEIGHTS_DIR/gemma-4-e2b-it"
cargo run --release --features cactus -p masker-cli -- \
  live --seconds 5 --stt-engine gemma4

# With explicit model path instead of env var
cargo run --release --features cactus -p masker-cli -- \
  live --seconds 5 \
  --stt-engine gemma4 \
  --gemma-stt-model-path /path/to/gemma-4-e2b-it

# From an audio file (--audio-file triggers Gemma audio path)
cargo run --release --features cactus -p masker-cli -- \
  live --audio-file /path/to/sample.wav --stt-engine gemma4

# JSON output
cargo run --release --features cactus -p masker-cli -- \
  live --audio-file /path/to/sample.wav --stt-engine gemma4 --output json
```

> Note: `--stream-output` is NOT supported with `--stt-engine gemma4`
> (Gemma needs a complete on-disk WAV per chunk). Use `--audio-file` or
> switch to `--stt-engine cactus` for streaming.

---

## masker live — Whisper/Parakeet STT + Gemma4 detection via audio_encoder

This uses a dedicated STT model for transcription, then Gemma's audio_encoder
for PII/PHI detection (it sees both the audio waveform and the transcript text).

```bash
# Whisper STT + Gemma4 detection (auto-detect model paths from env)
source platform/masker-core/scripts/cactus-npu-env.sh
cargo run --release --features cactus -p masker-cli -- \
  live --seconds 5 --stt whisper --detect gemma4

# Parakeet STT + Gemma4 detection, live streaming output
cargo run --release --features cactus -p masker-cli -- \
  live --interactive --stream-output --stt parakeet --detect gemma4

# With explicit paths
cargo run --release --features cactus -p masker-cli -- \
  live --seconds 5 \
  --stt-model-path /path/to/whisper-small \
  --detection-model-path /path/to/gemma-4-e2b-it
```

---

## Environment variables

| Variable | Purpose |
|---|---|
| `CACTUS_STT_MODEL_PATH` | Path to Whisper or Parakeet model dir |
| `CACTUS_GEMMA_STT_MODEL_PATH` | Path to gemma-4-e2b-it for `--stt-engine gemma4` |
| `CACTUS_DETECTION_MODEL_PATH` | Path to gemma-4-e2b-it for PII detection |
| `CACTUS_WEIGHTS_DIR` | Base dir containing all model subdirs |
| `CACTUS_ANE_COMPUTE_UNITS` | Set to `cpu_and_ne` to enable Apple Neural Engine |

---

## Troubleshooting

```bash
# List available audio inputs
ffmpeg -f avfoundation -list_devices true -i ""

# Use a specific mic input (default is :0)
cargo run --release --features cactus -p masker-cli -- \
  live --seconds 5 --input ":1"

# Compile audio_encoder.mlpackage to .mlmodelc for faster load
cargo run --release -p masker-cli -- coreml compile \
  --model "$CACTUS_WEIGHTS_DIR/gemma-4-e2b-it/audio_encoder.mlpackage" \
  --out-dir "$CACTUS_WEIGHTS_DIR/gemma-4-e2b-it"

# Re-download and reconvert Gemma if mlpackage files are missing
cactus download google/gemma-4-E2B-it --reconvert
```
