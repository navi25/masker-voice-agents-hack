"""
Training configuration — single source of truth for all training defaults.

Edit values here to change model, dataset, hyperparameters, or output paths.
All values can also be overridden via CLI flags in train_gemma4_nemotron_pii.py.
"""

# ── Model & dataset ────────────────────────────────────────────────────────────

# Base model to fine-tune (Unsloth-optimised Gemma 4 E4B instruction-tuned)
MODEL_NAME = "unsloth/gemma-4-E4B-it"

# PII detection dataset from Nvidia
DATASET_NAME = "nvidia/Nemotron-PII"

# ── Output ─────────────────────────────────────────────────────────────────────

DEFAULT_OUTPUT_DIR = "outputs/gemma4-pii"

# GGUF filename stem (will produce <stem>-unsloth.Q4_K_M.gguf)
GGUF_STEM = "gemma4-pii"

# ── Training limits ────────────────────────────────────────────────────────────

# Wall-clock hours before TimeLimitCallback stops training
DEFAULT_MAX_HOURS = 2.0

# Upper bound on epochs (training stops earlier via time limit)
NUM_TRAIN_EPOCHS = 10

# ── Sequence & LoRA ────────────────────────────────────────────────────────────

MAX_SEQ_LENGTH = 2048
LORA_RANK = 16
LORA_ALPHA_MULTIPLIER = 2   # lora_alpha = LORA_RANK * LORA_ALPHA_MULTIPLIER

LORA_TARGET_MODULES = [
    "q_proj", "k_proj", "v_proj", "o_proj",
    "gate_proj", "up_proj", "down_proj",
]

# ── Optimiser defaults ─────────────────────────────────────────────────────────

DEFAULT_BATCH_SIZE = 2
DEFAULT_GRAD_ACCUM = 4
DEFAULT_LR = 2e-4
WARMUP_STEPS = 20
LR_SCHEDULER = "cosine"

# ── Logging / checkpointing ────────────────────────────────────────────────────

LOGGING_STEPS = 10
SAVE_STEPS = 100
SAVE_TOTAL_LIMIT = 3
DATASET_NUM_PROC = 4

# ── System prompt ──────────────────────────────────────────────────────────────

SYSTEM_PROMPT = (
    "You are a privacy intelligence expert. "
    "Identify and tag every PII/PHI entity in the input text. "
    'Wrap each entity with <PII type="entity_type">value</PII> tags. '
    "Common types: name, email, phone, ssn, address, dob, mrn, "
    "insurance_id, account_number, credit_card, passport, ip_address, "
    "url, organization, username, npi, license_number."
)
