#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
DEMOS_DIR="${ROOT_DIR}/demos"

if ! command -v say >/dev/null 2>&1; then
  echo "macOS 'say' is required to generate demo audio."
  exit 1
fi

if ! command -v ffmpeg >/dev/null 2>&1; then
  echo "ffmpeg is required to convert demo audio to 16kHz mono WAV."
  exit 1
fi

for fixture in healthcare finance phone_address; do
  text_file="${DEMOS_DIR}/${fixture}.txt"
  aiff_file="${DEMOS_DIR}/${fixture}.aiff"
  wav_file="${DEMOS_DIR}/${fixture}.wav"

  say -f "${text_file}" -o "${aiff_file}"
  ffmpeg -y -i "${aiff_file}" -ar 16000 -ac 1 "${wav_file}" >/dev/null 2>&1
  rm -f "${aiff_file}"
  echo "generated ${wav_file}"
done
