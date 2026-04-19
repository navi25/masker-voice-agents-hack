from __future__ import annotations

from dataclasses import asdict, dataclass, field, is_dataclass
from typing import Any


def to_dict(value: Any) -> Any:
    if is_dataclass(value):
        return {k: to_dict(v) for k, v in asdict(value).items()}
    if isinstance(value, list):
        return [to_dict(item) for item in value]
    if isinstance(value, dict):
        return {key: to_dict(item) for key, item in value.items()}
    return value


@dataclass
class DetectedEntity:
    entity_type: str
    raw_value: str
    token: str
    start: int
    end: int
    confidence: float = 1.0


@dataclass
class RedactionResult:
    raw_text: str
    redacted_text: str
    masked_prompt: str
    entities: list[DetectedEntity] = field(default_factory=list)
    token_map: dict[str, str] = field(default_factory=dict)


@dataclass
class SafeLogEntry:
    session_id: str
    utterance_id: str
    policy_mode: str
    redacted_text: str
    masked_prompt: str
    entity_types: list[str]
    entity_count: int
    timestamp_ms: int


@dataclass
class SessionConfig:
    session_id: str
    audio_mode: str = "mic"
    audio_path: str | None = None
    stt_model: str = "small.en"
    language: str | None = "en"
    no_model: bool = False
    policy_mode: str = "hipaa_safe_mode"
    partial_interval_ms: int = 900
    sample_rate: int = 16000
    device: str | int | None = None
    simulate_realtime: bool = True
    min_partial_ms: int = 700


@dataclass
class SttSegment:
    text: str
    start_ms: int
    end_ms: int


@dataclass
class SttResult:
    text: str
    segments: list[SttSegment] = field(default_factory=list)
    language: str | None = None
