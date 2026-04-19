from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path

from .config import (
    DEFAULT_HOST, DEFAULT_PORT,
    DEFAULT_POLICY, DEFAULT_LANGUAGE, DEFAULT_STT_MODEL,
    DEFAULT_SAMPLE_RATE, DEFAULT_PARTIAL_INTERVAL_MS, DEFAULT_LOG_DIR,
)


def load_env() -> None:
    try:
        from dotenv import load_dotenv
    except ImportError:
        return

    env_path = Path(__file__).resolve().parent.parent / ".env"
    load_dotenv(env_path)


@dataclass(frozen=True)
class Settings:
    host: str = os.getenv("MASKER_DEMO_HOST", DEFAULT_HOST)
    port: int = int(os.getenv("MASKER_DEMO_PORT", str(DEFAULT_PORT)))
    policy_mode: str = os.getenv("MASKER_DEMO_POLICY_MODE", DEFAULT_POLICY)
    language: str | None = os.getenv("MASKER_DEMO_LANGUAGE", DEFAULT_LANGUAGE) or None
    stt_model: str = os.getenv("MASKER_DEMO_STT_MODEL", DEFAULT_STT_MODEL)
    sample_rate: int = int(os.getenv("MASKER_DEMO_SAMPLE_RATE", str(DEFAULT_SAMPLE_RATE)))
    partial_interval_ms: int = int(os.getenv("MASKER_DEMO_PARTIAL_INTERVAL_MS", str(DEFAULT_PARTIAL_INTERVAL_MS)))
    log_dir: str = os.getenv("MASKER_DEMO_LOG_DIR", DEFAULT_LOG_DIR)
    default_device: str | None = os.getenv("MASKER_DEMO_DEFAULT_DEVICE") or None


load_env()
SETTINGS = Settings()
