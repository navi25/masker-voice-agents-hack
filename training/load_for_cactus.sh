#!/usr/bin/env bash
# load_for_cactus.sh
#
# Copies the fine-tuned Gemma4-PII GGUF into the cactus weights directory
# and prints the env-var commands needed to activate it in masker-core.
#
# Usage:
#   ./training/load_for_cactus.sh                          # default output dir
#   ./training/load_for_cactus.sh /custom/path/to/outputs  # custom output dir
#   ./training/load_for_cactus.sh "" /custom/cactus/weights # custom weights dir

set -euo pipefail

OUTPUTS_DIR="${1:-outputs/gemma4-pii}"
WEIGHTS_DIR="${2:-$HOME/.cactus/weights}"
MODEL_SLUG="gemma4-pii"

GGUF_SRC="$OUTPUTS_DIR/gemma4-pii-unsloth.Q4_K_M.gguf"
GGUF_DST="$WEIGHTS_DIR/$MODEL_SLUG.gguf"
LORA_SRC="$OUTPUTS_DIR/lora_adapter"

# ── 1. Verify training output exists ──────────────────────────────────────────
if [ ! -f "$GGUF_SRC" ]; then
    echo "ERROR: GGUF not found at $GGUF_SRC"
    echo ""
    echo "Run training first:"
    echo "  pip install -r training/requirements.txt"
    echo "  python training/train_gemma4_nemotron_pii.py"
    echo ""
    echo "Or open the Colab notebook:"
    echo "  training/Gemma4_E4B_Nemotron_PII.ipynb"
    exit 1
fi

# ── 2. Copy GGUF into cactus weights dir ──────────────────────────────────────
mkdir -p "$WEIGHTS_DIR"
cp "$GGUF_SRC" "$GGUF_DST"
echo "✓ Copied GGUF: $GGUF_DST ($(du -sh "$GGUF_DST" | cut -f1))"

# ── 3. Print activation instructions ──────────────────────────────────────────
echo ""
echo "─────────────────────────────────────────────────────────"
echo "Activate the fine-tuned PII model in masker-core:"
echo ""
echo "  export CACTUS_DETECTION_MODEL_PATH=$GGUF_DST"
if [ -d "$WEIGHTS_DIR/whisper-small" ]; then
    echo "  export CACTUS_STT_MODEL_PATH=$WEIGHTS_DIR/whisper-small"
else
    echo "  # (download STT model if needed)"
    echo "  cactus download openai/whisper-small"
    echo "  export CACTUS_STT_MODEL_PATH=$WEIGHTS_DIR/whisper-small"
fi
echo ""
echo "Then run:"
echo "  cd platform/masker-core"
echo "  cargo run --release --features cactus -p masker-cli -- --backend cactus"
echo ""
echo "Single-turn CLI:"
echo "  masker run-turn --text \"My SSN is 123-45-6789\" --backend cactus --policy hipaa-clinical"
echo "─────────────────────────────────────────────────────────"

# ── 4. Optionally copy LoRA adapter for reference ─────────────────────────────
if [ -d "$LORA_SRC" ]; then
    LORA_DST="$WEIGHTS_DIR/$MODEL_SLUG-lora"
    cp -r "$LORA_SRC" "$LORA_DST"
    echo ""
    echo "LoRA adapter also copied → $LORA_DST"
    echo "(Load with PEFT/transformers for further fine-tuning or merging)"
fi
