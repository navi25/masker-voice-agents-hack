#!/usr/bin/env python3
"""
Fine-tune Gemma 4 E4B on nvidia/Nemotron-PII for on-device PII detection.

Runs for up to 2 hours, then saves:
  outputs/gemma4-pii/lora_adapter/   -- HuggingFace LoRA weights
  outputs/gemma4-pii/*.gguf          -- Q4_K_M GGUF for cactus-compute/cactus

Usage:
  python train_gemma4_nemotron_pii.py
  python train_gemma4_nemotron_pii.py --max-hours 1 --output-dir /tmp/pii-model
"""

import argparse
import time

import torch
from datasets import load_dataset
from trl import SFTConfig, SFTTrainer
from transformers import TrainerCallback
from unsloth import FastModel

from config import (
    MODEL_NAME, DATASET_NAME, DEFAULT_OUTPUT_DIR, GGUF_STEM,
    DEFAULT_MAX_HOURS, NUM_TRAIN_EPOCHS,
    MAX_SEQ_LENGTH, LORA_RANK, LORA_ALPHA_MULTIPLIER, LORA_TARGET_MODULES,
    DEFAULT_BATCH_SIZE, DEFAULT_GRAD_ACCUM, DEFAULT_LR,
    WARMUP_STEPS, LR_SCHEDULER,
    LOGGING_STEPS, SAVE_STEPS, SAVE_TOTAL_LIMIT, DATASET_NUM_PROC,
    SYSTEM_PROMPT,
)


# ─── Time-based early stopping ────────────────────────────────────────────────

class TimeLimitCallback(TrainerCallback):
    """Stop training after `max_seconds` wall-clock seconds."""

    def __init__(self, max_seconds: float):
        self.max_seconds = max_seconds
        self._start: float | None = None

    def on_train_begin(self, args, state, control, **kwargs):
        self._start = time.time()

    def on_step_end(self, args, state, control, **kwargs):
        elapsed = time.time() - self._start
        if elapsed >= self.max_seconds:
            hrs = elapsed / 3600
            print(f"\n[TimeLimitCallback] {hrs:.2f}h elapsed — stopping.")
            control.should_training_stop = True
        return control


# ─── Dataset formatting ───────────────────────────────────────────────────────

def build_dataset(tokenizer, split: str = "train"):
    """
    Load Nemotron-PII and format each row as a chat-templated string.
    Input:  row["text"]       (raw document with PII)
    Target: row["text_tagged"] (same doc with <PII type="...">...</PII> tags)
    """
    raw = load_dataset(DATASET_NAME, split=split)

    def _format(row):
        messages = [
            {
                "role": "user",
                "content": f"{SYSTEM_PROMPT}\n\n{row['text']}",
            },
            {
                "role": "assistant",
                "content": row["text_tagged"],
            },
        ]
        return {
            "text": tokenizer.apply_chat_template(
                messages,
                tokenize=False,
                add_generation_prompt=False,
            )
        }

    return raw.map(
        _format,
        remove_columns=raw.column_names,
        num_proc=4,
        desc="Formatting dataset",
    )


# ─── Main ─────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--max-hours", type=float, default=DEFAULT_MAX_HOURS)
    parser.add_argument("--output-dir", default=DEFAULT_OUTPUT_DIR)
    parser.add_argument("--lora-rank", type=int, default=LORA_RANK)
    parser.add_argument("--batch-size", type=int, default=DEFAULT_BATCH_SIZE)
    parser.add_argument("--grad-accum", type=int, default=DEFAULT_GRAD_ACCUM)
    parser.add_argument("--lr", type=float, default=DEFAULT_LR)
    parser.add_argument("--no-gguf", action="store_true",
                        help="Skip GGUF export (faster, still saves LoRA)")
    args = parser.parse_args()

    max_seconds = args.max_hours * 3600
    out = args.output_dir

    print(f"Loading {MODEL_NAME} (4-bit)…")
    model, tokenizer = FastModel.from_pretrained(
        model_name=MODEL_NAME,
        max_seq_length=MAX_SEQ_LENGTH,
        load_in_4bit=True,
        full_finetuning=False,
    )

    model = FastModel.get_peft_model(
        model,
        r=args.lora_rank,
        lora_alpha=args.lora_rank * LORA_ALPHA_MULTIPLIER,
        target_modules=LORA_TARGET_MODULES,
        lora_dropout=0,
        bias="none",
        use_gradient_checkpointing="unsloth",
        random_state=42,
    )

    print(f"Loading {DATASET_NAME}…")
    train_dataset = build_dataset(tokenizer, split="train")
    print(f"  {len(train_dataset):,} training examples")

    use_bf16 = torch.cuda.is_bf16_supported()

    trainer = SFTTrainer(
        model=model,
        processing_class=tokenizer,
        train_dataset=train_dataset,
        args=SFTConfig(
            output_dir=out,
            per_device_train_batch_size=args.batch_size,
            gradient_accumulation_steps=args.grad_accum,
            max_seq_length=MAX_SEQ_LENGTH,
            num_train_epochs=NUM_TRAIN_EPOCHS,
            learning_rate=args.lr,
            warmup_steps=WARMUP_STEPS,
            lr_scheduler_type=LR_SCHEDULER,
            bf16=use_bf16,
            fp16=not use_bf16,
            logging_steps=LOGGING_STEPS,
            save_steps=SAVE_STEPS,
            save_total_limit=SAVE_TOTAL_LIMIT,
            dataset_text_field="text",
            dataset_num_proc=DATASET_NUM_PROC,
            report_to="none",
        ),
        callbacks=[TimeLimitCallback(max_seconds)],
    )

    print(f"\nStarting training (max {args.max_hours:.1f}h on {DATASET_NAME})…")
    trainer.train()

    # ── Save LoRA adapter ──────────────────────────────────────────────────────
    lora_path = f"{out}/lora_adapter"
    model.save_pretrained(lora_path)
    tokenizer.save_pretrained(lora_path)
    print(f"\nLoRA adapter → {lora_path}")

    # ── Export GGUF for Cactus ─────────────────────────────────────────────────
    if not args.no_gguf:
        gguf_prefix = f"{out}/{GGUF_STEM}"
        print(f"Exporting Q4_K_M GGUF → {gguf_prefix}-unsloth.Q4_K_M.gguf …")
        model.save_pretrained_gguf(
            gguf_prefix,
            tokenizer,
            quantization_method="q4_k_m",
        )
        gguf_file = f"{gguf_prefix}-unsloth.Q4_K_M.gguf"
        print("\n✓ Done. Load with cactus:")
        print(f"  export CACTUS_DETECTION_MODEL_PATH={gguf_file}")
        print("  cargo run --release --features cactus -p masker-cli -- --backend cactus")
    else:
        print("\n✓ Done (GGUF skipped). Run load_for_cactus.sh to convert later.")


if __name__ == "__main__":
    main()
