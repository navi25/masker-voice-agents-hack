from __future__ import annotations

from dataclasses import dataclass, field


@dataclass
class Entity:
    type: str  # ssn | phone | email | name | address | insurance_id | mrn | dob | health_context | other
    value: str
    start: int
    end: int
    confidence: float = 0.9


@dataclass
class DetectionResult:
    entities: list[Entity]
    risk_level: str  # none | low | medium | high

    def has_sensitive(self) -> bool:
        return self.risk_level in ("medium", "high")


@dataclass
class PolicyDecision:
    route: str      # local-only | masked-send | safe-to-send
    policy: str     # hipaa_base | hipaa_logging | hipaa_clinical | gdpr_base
    rationale: str = ""


@dataclass
class MaskedText:
    text: str
    token_map: dict[str, str] = field(default_factory=dict)


@dataclass
class TraceEvent:
    stage: str
    message: str
    elapsed_ms: float
    payload: dict = field(default_factory=dict)


@dataclass
class FilterInputResult:
    masked_input: MaskedText
    policy: PolicyDecision
    detection: DetectionResult
    trace: list[TraceEvent] = field(default_factory=list)


@dataclass
class FilterOutputResult:
    safe_text: str
    trace: list[TraceEvent] = field(default_factory=list)


@dataclass
class TurnResult:
    user_text: str
    detection: DetectionResult
    policy: PolicyDecision
    masked_input: MaskedText
    model_output: str
    safe_output: str
    trace: list[TraceEvent]
    total_ms: float


@dataclass
class StreamResult:
    seq: int
    raw_transcript: str
    route: str
    policy: str
    entity_count: int
    entity_types: list[str]
    risk_level: str
    masked_transcript: str
    processing_ms: float
    trace: list[TraceEvent] = field(default_factory=list)
