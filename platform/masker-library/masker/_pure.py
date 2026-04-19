"""Pure-Python detection + policy + masking — no binary required.

Mirrors the Rust logic in masker-core/crates/masker/src/{detection,policy,masking}.rs
so the SDK works out of the box when the compiled binary is not available.
"""
from __future__ import annotations

import re
from typing import Optional

from masker.contracts import (
    DetectionResult,
    Entity,
    FilterInputResult,
    FilterOutputResult,
    MaskedText,
    PolicyDecision,
    StreamResult,
    TurnResult,
)

# ── detection patterns ────────────────────────────────────────────────────────

_PATTERNS: list[tuple[str, re.Pattern, int]] = [
    ("ssn",          re.compile(r"\b\d{3}-\d{2}-\d{4}\b"), 0),
    ("phone",        re.compile(r"\b(?:\+?1[-.\s]?)?\(?\d{3}\)?[-.\s]?\d{3}[-.\s]?\d{4}\b"), 0),
    ("email",        re.compile(r"\b[\w.+-]+@[\w-]+\.[\w.-]+\b"), 0),
    ("insurance_id", re.compile(
        r"(?i)\b(?:insurance|member|policy)\s*(?:id|#|number)?(?:\s+(?:is|=|number))?\s*[:#=]?\s*"
        r"([A-Z]{2,}-?[A-Z0-9]{4,}|[A-Z0-9]{6,})\b"
    ), 1),
    ("mrn",          re.compile(r"(?i)\bMRN\s*(?:#|number|is)?\s*[:#=]?\s*([A-Z0-9-]{4,})\b"), 1),
    ("dob",          re.compile(
        r"\b(?:0?[1-9]|1[0-2])[/-](?:0?[1-9]|[12]\d|3[01])[/-](?:19|20)\d{2}\b"
    ), 0),
    ("address",      re.compile(
        r"\b\d{1,5}\s+[A-Z][a-z]+(?:\s+(?:Street|St|Avenue|Ave|Road|Rd|Boulevard|Blvd|Drive|Dr|Lane|Ln|Way))?\b"
    ), 0),
]

_HEALTH_KEYWORDS = re.compile(
    r"(?i)\b(chest pain|diabetes|cancer|asthma|prescription|medication|symptoms?|diagnosis|"
    r"insurance|patient|medical|hipaa|surgery|allergy|allergies|blood pressure|heart attack|"
    r"depression|anxiety)\b"
)

_SSN_CUES = re.compile(r"(?i)\b(ssn|s\s*s\s*n|sns|social security(?: number)?)\b")
_CARD_CUES = re.compile(
    r"(?i)\b(credit card(?: number)?|debit card(?: number)?|card number|grid card(?: number)?)\b"
)
_CVV_CUES = re.compile(r"(?i)\b(c\s*v\s*v|cvv|security code)\b")
_WORD_TOKENS = re.compile(r"[A-Za-z0-9']+")

_SPOKEN_DIGITS = {
    "zero": 1, "oh": 1, "o": 1, "one": 1, "two": 1, "three": 1,
    "four": 1, "five": 1, "six": 1, "seven": 1, "eight": 1, "nine": 1,
}
_SEPARATORS = {"dash", "hyphen", "minus"}
_FILLERS = {"is", "was", "number", "num", "equals", "equal", "colon", "my", "the"}

_HIGH_RISK_TYPES = {"ssn", "mrn", "insurance_id"}


def _risk_from_entities(entities: list[Entity], has_health: bool) -> str:
    identifying = [e for e in entities if e.type != "health_context"]
    if not identifying and not has_health:
        return "none"
    types = {e.type for e in identifying}
    if types & _HIGH_RISK_TYPES:
        return "high"
    if has_health and identifying:
        return "high"
    if identifying:
        return "medium"
    return "low"


def _tokenize(text: str) -> list[tuple[str, int, int]]:
    return [(m.group(), m.start(), m.end()) for m in _WORD_TOKENS.finditer(text)]


def _spoken_digit_len(raw: str, norm: str) -> Optional[int]:
    if norm in _SPOKEN_DIGITS:
        return 1
    if raw.isdigit():
        return len(raw)
    return None


def _parse_number_after_cue(
    text: str,
    tokens: list[tuple[str, int, int]],
    start_idx: int,
    kind: str,
    min_digits: int,
    max_digits: int,
    confidence: float,
) -> Optional[Entity]:
    first_pos = last_pos = None
    total_digits = 0
    examined = 0
    saw_digit = False

    for raw, tok_start, tok_end in tokens[start_idx:start_idx + 16]:
        examined += 1
        norm = raw.lower()

        if not saw_digit and norm in _FILLERS:
            continue

        dlen = _spoken_digit_len(raw, norm)
        if dlen is not None:
            if first_pos is None:
                first_pos = tok_start
            last_pos = tok_end
            total_digits += dlen
            saw_digit = True
            if total_digits > max_digits:
                return None
            continue

        if norm in _SEPARATORS:
            if not saw_digit:
                break
            continue

        if saw_digit:
            break
        if examined >= 4:
            break

    if first_pos is None or last_pos is None:
        return None
    if not (min_digits <= total_digits <= max_digits):
        return None

    return Entity(
        type=kind,
        value=text[first_pos:last_pos],
        start=first_pos,
        end=last_pos,
        confidence=confidence,
    )


def _find_spoken_ssn(text: str, tokens: list[tuple[str, int, int]]) -> list[Entity]:
    results = []
    for cue in _SSN_CUES.finditer(text):
        idx = next((i for i, (_, ts, _) in enumerate(tokens) if ts >= cue.end()), None)
        if idx is None:
            continue
        entity = _parse_number_after_cue(text, tokens, idx, "ssn", 8, 9, 0.8)
        if entity:
            results.append(entity)
    return results


def _find_contextual_financial(text: str, tokens: list[tuple[str, int, int]]) -> list[Entity]:
    results = []
    for cue in _CARD_CUES.finditer(text):
        idx = next((i for i, (_, ts, _) in enumerate(tokens) if ts >= cue.end()), None)
        if idx is None:
            continue
        entity = _parse_number_after_cue(text, tokens, idx, "other", 8, 19, 0.75)
        if entity:
            results.append(entity)
    for cue in _CVV_CUES.finditer(text):
        idx = next((i for i, (_, ts, _) in enumerate(tokens) if ts >= cue.end()), None)
        if idx is None:
            continue
        entity = _parse_number_after_cue(text, tokens, idx, "other", 3, 4, 0.75)
        if entity:
            results.append(entity)
    return results


def _dedupe(entities: list[Entity]) -> list[Entity]:
    seen: set[tuple] = set()
    out = []
    for e in sorted(entities, key=lambda e: (e.start, e.end, e.type, e.value)):
        key = (e.type, e.start, e.end, e.value)
        if key not in seen:
            seen.add(key)
            out.append(e)
    return out


def detect(text: str) -> DetectionResult:
    entities: list[Entity] = []

    for kind, pat, cap_idx in _PATTERNS:
        for m in pat.finditer(text):
            g = m.group(cap_idx) if cap_idx else m.group(0)
            if g is None:
                continue
            start = m.start(cap_idx) if cap_idx else m.start()
            end = m.end(cap_idx) if cap_idx else m.end()
            entities.append(Entity(type=kind, value=g, start=start, end=end, confidence=0.9))

    health_m = _HEALTH_KEYWORDS.search(text)
    has_health = health_m is not None
    if health_m:
        entities.append(Entity(
            type="health_context",
            value=health_m.group(0),
            start=health_m.start(),
            end=health_m.end(),
            confidence=0.7,
        ))

    tokens = _tokenize(text)
    entities.extend(_find_spoken_ssn(text, tokens))
    entities.extend(_find_contextual_financial(text, tokens))
    entities = _dedupe(entities)

    risk = _risk_from_entities(entities, has_health)
    return DetectionResult(entities=entities, risk_level=risk)


# ── masking ───────────────────────────────────────────────────────────────────

def mask(text: str, detection: DetectionResult, mode: str = "placeholder") -> MaskedText:
    spans = sorted(
        [e for e in detection.entities if e.type != "health_context"],
        key=lambda e: e.start,
        reverse=True,
    )
    token_map: dict[str, str] = {}
    result = text

    for e in spans:
        placeholder = f"[{e.type.upper()}]"
        token_map[placeholder] = e.value
        result = result[:e.start] + placeholder + result[e.end:]

    return MaskedText(text=result, token_map=token_map)


def scrub_output(model_text: str, detection: DetectionResult) -> str:
    result = model_text
    for e in sorted(detection.entities, key=lambda e: e.start, reverse=True):
        if e.value and e.value in result:
            result = result.replace(e.value, f"[{e.type.upper()}]")
    return result


# ── policy ────────────────────────────────────────────────────────────────────

def decide(detection: DetectionResult, policy: str = "hipaa-base") -> PolicyDecision:
    types = {e.type for e in detection.entities}
    has_sensitive = detection.has_sensitive()

    if policy == "hipaa-logging" and has_sensitive:
        return PolicyDecision(
            route="masked-send", policy=policy,
            rationale="Strict logging policy: any sensitive data must be masked before traversal.",
        )

    if policy == "gdpr-base" and has_sensitive:
        return PolicyDecision(
            route="masked-send", policy=policy,
            rationale="GDPR base: all personal data must be masked before forwarding.",
        )

    if policy == "hipaa-clinical":
        if types & _HIGH_RISK_TYPES:
            return PolicyDecision(
                route="local-only", policy=policy,
                rationale="Direct identifiers (SSN/MRN) must stay on-device under clinical policy.",
            )
        if "health_context" in types and len(detection.entities) > 1:
            return PolicyDecision(
                route="masked-send", policy=policy,
                rationale="Clinical context with identifiers → mask identifiers, keep medical context.",
            )

    if types & _HIGH_RISK_TYPES:
        hits = sorted(types & _HIGH_RISK_TYPES)
        return PolicyDecision(
            route="local-only", policy=policy,
            rationale=f"High-risk identifiers detected: {hits}",
        )

    if has_sensitive:
        sensitive = sorted({e.type for e in detection.entities if e.type != "health_context"})
        return PolicyDecision(
            route="masked-send", policy=policy,
            rationale=f"Sensitive entities present: {sensitive}",
        )

    return PolicyDecision(
        route="safe-to-send", policy=policy,
        rationale="No sensitive entities detected.",
    )


# ── public API (mirrors __init__.py) ─────────────────────────────────────────

def filter_input(text: str, *, policy: str = "hipaa-base", mask_mode: str = "placeholder") -> FilterInputResult:
    detection = detect(text)
    decision = decide(detection, policy)
    masked = mask(text, detection, mask_mode)
    return FilterInputResult(masked_input=masked, policy=decision, detection=detection)


def filter_output(text: str, detection: Optional[DetectionResult] = None) -> FilterOutputResult:
    if detection is None:
        detection = detect(text)
    return FilterOutputResult(safe_text=scrub_output(text, detection))


def run_turn(text: str, *, backend: str = "auto", policy: str = "hipaa-base") -> TurnResult:
    detection = detect(text)
    decision = decide(detection, policy)
    masked = mask(text, detection)
    return TurnResult(
        user_text=text,
        detection=detection,
        policy=decision,
        masked_input=masked,
        model_output="[stub — binary not available]",
        safe_output=masked.text,
        trace=[],
        total_ms=0.0,
    )


def stream(text: str, *, session: str = "ses_py", api_key: Optional[str] = None) -> StreamResult:
    detection = detect(text)
    decision = decide(detection)
    masked = mask(text, detection)
    return StreamResult(
        seq=0,
        raw_transcript=text,
        route=decision.route,
        policy=decision.policy,
        entity_count=len(detection.entities),
        entity_types=[e.type for e in detection.entities],
        risk_level=detection.risk_level,
        masked_transcript=masked.text,
        processing_ms=0.0,
    )
