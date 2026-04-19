from __future__ import annotations

import os
from dataclasses import dataclass
from pathlib import Path


def load_env() -> None:
    try:
        from dotenv import load_dotenv
    except ImportError:
        return

    env_path = Path(__file__).resolve().parent.parent / ".env"
    load_dotenv(env_path)


@dataclass(frozen=True)
class Settings:
    host: str = os.getenv("MASKER_DEMO_HOST", "127.0.0.1")
    port: int = int(os.getenv("MASKER_DEMO_PORT", "8008"))
    policy_mode: str = os.getenv("MASKER_DEMO_POLICY_MODE", "hipaa_safe_mode")
    language: str | None = os.getenv("MASKER_DEMO_LANGUAGE", "en") or None
    stt_model: str = os.getenv("MASKER_DEMO_STT_MODEL", "small.en")
    sample_rate: int = int(os.getenv("MASKER_DEMO_SAMPLE_RATE", "16000"))
    partial_interval_ms: int = int(os.getenv("MASKER_DEMO_PARTIAL_INTERVAL_MS", "900"))
    log_dir: str = os.getenv("MASKER_DEMO_LOG_DIR", ".masker_safe_logs")
    default_device: str | None = os.getenv("MASKER_DEMO_DEFAULT_DEVICE") or None


load_env()
SETTINGS = Settings()
