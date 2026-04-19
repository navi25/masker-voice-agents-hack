"""
masker — Python client for the Masker on-device PII/PHI voice-agent middleware.

Works in two modes, selected automatically:

1. **CLI mode** (full fidelity): delegates to the compiled ``masker`` binary.
   Set ``MASKER_BIN=/path/to/masker`` or ensure ``masker`` is on PATH.

2. **Pure-Python mode** (zero dependencies): built-in regex detection + policy
   engine used as a fallback when the binary is not available.

Quick start::

    from masker import filter_input, filter_output, run_turn, stream

    result = filter_input("My SSN is 123-45-6789")
    print(result.policy.route)          # masked-send
    print(result.masked_input.text)     # My SSN is [SSN]

    turn = run_turn("What's the weather?")
    print(turn.safe_output)

Check which mode is active::

    import masker
    print(masker.backend())   # "cli" or "pure-python"
"""
from __future__ import annotations

import json as _json

from masker._cli import run_cli, _find_binary
from masker.contracts import (
    DetectionResult,
    Entity,
    FilterInputResult,
    FilterOutputResult,
    MaskedText,
    PolicyDecision,
    StreamResult,
    TraceEvent,
    TurnResult,
)
import masker._pure as _pure

__all__ = [
    "filter_input",
    "filter_output",
    "run_turn",
    "stream",
    "backend",
    "DetectionResult",
    "Entity",
    "FilterInputResult",
    "FilterOutputResult",
    "MaskedText",
    "PolicyDecision",
    "StreamResult",
    "TraceEvent",
    "TurnResult",
]


def backend() -> str:
    """Return ``'cli'`` if the masker binary is available, else ``'pure-python'``."""
    try:
        _find_binary()
        return "cli"
    except RuntimeError:
        return "pure-python"


def _cli_available() -> bool:
    try:
        _find_binary()
        return True
    except RuntimeError:
        return False


# ── internal parsers (used only in CLI mode) ──────────────────────────────────

def _entity(d: dict) -> Entity:
    return Entity(
        type=d["type"],
        value=d["value"],
        start=d["start"],
        end=d["end"],
        confidence=d.get("confidence", 0.9),
    )


def _detection(d: dict) -> DetectionResult:
    return DetectionResult(
        entities=[_entity(e) for e in d.get("entities", [])],
        risk_level=d["risk_level"],
    )


def _policy(d: dict) -> PolicyDecision:
    return PolicyDecision(
        route=d["route"],
        policy=d["policy"],
        rationale=d.get("rationale", ""),
    )


def _masked(d: dict) -> MaskedText:
    return MaskedText(text=d["text"], token_map=d.get("token_map", {}))


def _trace(items: list) -> list[TraceEvent]:
    return [
        TraceEvent(
            stage=e["stage"],
            message=e["message"],
            elapsed_ms=e["elapsed_ms"],
            payload=e.get("payload", {}),
        )
        for e in items
    ]


# ── public API ────────────────────────────────────────────────────────────────

def filter_input(
    text: str,
    *,
    policy: str = "hipaa-base",
    mask_mode: str = "placeholder",
) -> FilterInputResult:
    """Detect PII/PHI, apply policy, and return the masked text + decision.

    Args:
        text: Raw user input to scan.
        policy: One of ``hipaa-base`` (default), ``hipaa-logging``, ``hipaa-clinical``.
        mask_mode: ``placeholder`` (default) replaces spans with ``[TYPE]``;
                   ``token`` uses reversible opaque tokens (CLI mode only).
    """
    if not _cli_available():
        return _pure.filter_input(text, policy=policy, mask_mode=mask_mode)

    data = run_cli(
        "filter-input",
        "--text", text,
        "--policy", policy,
        "--mask-mode", mask_mode,
    )
    return FilterInputResult(
        masked_input=_masked(data["masked_input"]),
        policy=_policy(data["policy"]),
        detection=_detection(data["detection"]),
        trace=_trace(data.get("trace", [])),
    )


def filter_output(
    text: str,
    detection: DetectionResult | None = None,
) -> FilterOutputResult:
    """Re-scan model output and scrub any sensitive values that leaked through.

    Args:
        text: Raw LLM output to scrub.
        detection: Optional ``DetectionResult`` from a prior ``filter_input``
                   call. When provided, the same entities are used for scrubbing
                   instead of re-running detection.
    """
    if not _cli_available():
        return _pure.filter_output(text, detection)

    args = ["filter-output", "--text", text]
    if detection is not None:
        det_json = _json.dumps({
            "entities": [
                {
                    "type": e.type,
                    "value": e.value,
                    "start": e.start,
                    "end": e.end,
                    "confidence": e.confidence,
                }
                for e in detection.entities
            ],
            "risk_level": detection.risk_level,
        })
        args += ["--detection-json", det_json]
    data = run_cli(*args)
    return FilterOutputResult(
        safe_text=data["safe_text"],
        trace=_trace(data.get("trace", [])),
    )


def run_turn(
    text: str,
    *,
    backend: str = "auto",
    policy: str = "hipaa-base",
) -> TurnResult:
    """Run a full end-to-end voice turn (detect → policy → mask → LLM → scrub).

    Args:
        text: User utterance.
        backend: ``auto`` (default), ``stub``, ``gemini``, or ``cactus``.
                 Ignored in pure-Python mode (no LLM is available).
        policy: Policy name (same values as ``filter_input``).
    """
    if not _cli_available():
        return _pure.run_turn(text, backend=backend, policy=policy)

    data = run_cli(
        "run-turn",
        "--text", text,
        "--backend", backend,
        "--policy", policy,
    )
    return TurnResult(
        user_text=data["user_text"],
        detection=_detection(data["detection"]),
        policy=_policy(data["policy"]),
        masked_input=_masked(data["masked_input"]),
        model_output=data["model_output"],
        safe_output=data["safe_output"],
        trace=_trace(data.get("trace", [])),
        total_ms=data["total_ms"],
    )


def stream(
    text: str,
    *,
    session: str = "ses_py",
    api_key: str | None = None,
) -> StreamResult:
    """Process one text chunk through the full streaming pipeline with audit logging.

    Args:
        text: Single utterance / audio transcript to process.
        session: Session ID for audit grouping (default ``ses_py``).
        api_key: Optional client API key to select a custom policy profile.
                 Ignored in pure-Python mode.
    """
    if not _cli_available():
        return _pure.stream(text, session=session, api_key=api_key)

    args = ["stream", "--text", text, "--session", session]
    if api_key is not None:
        args += ["--api-key", api_key]
    data = run_cli(*args)
    return StreamResult(
        seq=data["seq"],
        raw_transcript=data["raw_transcript"],
        route=data["route"],
        policy=data["policy"],
        entity_count=data["entity_count"],
        entity_types=data["entity_types"],
        risk_level=data["risk_level"],
        masked_transcript=data["masked_transcript"],
        processing_ms=data["processing_ms"],
        trace=_trace(data.get("trace", [])),
    )
