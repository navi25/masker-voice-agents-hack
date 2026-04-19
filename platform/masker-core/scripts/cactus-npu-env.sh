#!/usr/bin/env bash

set -euo pipefail

pick_weights_dir() {
  local dir=""
  for dir in "$@"; do
    [[ -n "$dir" && -d "$dir" ]] || continue
    if [[ -d "$dir/parakeet-tdt-0.6b-v3/model.mlpackage" ]] || \
       [[ -d "$dir/gemma-4-e2b-it/audio_encoder.mlpackage" ]] || \
       [[ -d "$dir/gemma-4-e2b-it/vision_encoder.mlpackage" ]]; then
      printf '%s\n' "$dir"
      return 0
    fi
  done

  for dir in "$@"; do
    [[ -n "$dir" && -d "$dir" ]] || continue
    printf '%s\n' "$dir"
    return 0
  done

  return 1
}

brew_weights_dir=""
if command -v brew >/dev/null 2>&1; then
  if brew_prefix="$(brew --prefix cactus 2>/dev/null)"; then
    brew_weights_dir="${brew_prefix}/libexec/weights"
  fi
fi

local_checkout="${CACTUS_LOCAL_CHECKOUT:-$HOME/Developer/ai/cactus}"
weights_dir="${CACTUS_WEIGHTS_DIR:-}"

if [[ -z "$weights_dir" ]]; then
  weights_dir="$(pick_weights_dir \
    "$local_checkout/weights" \
    "$brew_weights_dir")" || {
    echo "Could not locate a Cactus weights directory." >&2
    echo "Set CACTUS_WEIGHTS_DIR or CACTUS_LOCAL_CHECKOUT first." >&2
    return 1 2>/dev/null || exit 1
  }
fi

export CACTUS_WEIGHTS_DIR="$weights_dir"

if [[ -d "$weights_dir/parakeet-tdt-0.6b-v3" ]]; then
  export CACTUS_STT_MODEL_PATH="$weights_dir/parakeet-tdt-0.6b-v3"
fi

if [[ -d "$weights_dir/gemma-4-e2b-it" ]]; then
  export CACTUS_DETECTION_MODEL_PATH="$weights_dir/gemma-4-e2b-it"
fi

# Recommended defaults for Apple acceleration. Prefill prefers ANE; Gemma4
# audio/vision encoders keep Cactus runtime defaults unless explicitly forced.
export CACTUS_ANE_COMPUTE_UNITS="${CACTUS_ANE_COMPUTE_UNITS:-cpu_and_ne}"
export CACTUS_ANE_PREFILL_COMPUTE_UNITS="${CACTUS_ANE_PREFILL_COMPUTE_UNITS:-cpu_and_ne}"

stt_npu="missing"
[[ -d "${CACTUS_STT_MODEL_PATH:-}/model.mlpackage" ]] && stt_npu="ready"

gemma_audio_npu="missing"
gemma_vision_npu="missing"
gemma_prefill_npu="missing"
[[ -d "${CACTUS_DETECTION_MODEL_PATH:-}/audio_encoder.mlpackage" ]] && gemma_audio_npu="ready"
[[ -d "${CACTUS_DETECTION_MODEL_PATH:-}/vision_encoder.mlpackage" ]] && gemma_vision_npu="ready"
[[ -d "${CACTUS_DETECTION_MODEL_PATH:-}/model.mlpackage" ]] && gemma_prefill_npu="ready"

echo "Cactus NPU environment configured"
echo "  CACTUS_WEIGHTS_DIR          = $CACTUS_WEIGHTS_DIR"
echo "  CACTUS_STT_MODEL_PATH       = ${CACTUS_STT_MODEL_PATH:-unset}"
echo "  CACTUS_DETECTION_MODEL_PATH = ${CACTUS_DETECTION_MODEL_PATH:-unset}"
echo "  CACTUS_ANE_COMPUTE_UNITS    = $CACTUS_ANE_COMPUTE_UNITS"
echo "  CACTUS_ANE_PREFILL_COMPUTE_UNITS = $CACTUS_ANE_PREFILL_COMPUTE_UNITS"
echo
echo "Acceleration package status"
echo "  STT model.mlpackage         = $stt_npu"
echo "  Gemma audio_encoder         = $gemma_audio_npu"
echo "  Gemma vision_encoder        = $gemma_vision_npu"
echo "  Gemma model prefill         = $gemma_prefill_npu"

if [[ "$gemma_prefill_npu" != "ready" ]]; then
  echo
  echo "Note: Gemma text prefill will still run on CPU until model.mlpackage is available."
  echo "      Audio and vision encoders can still use Apple acceleration when their"
  echo "      mlpackage files are present."
fi
