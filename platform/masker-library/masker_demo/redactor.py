from __future__ import annotations

import re
from collections import defaultdict
from dataclasses import dataclass

from .models import DetectedEntity, RedactionResult


MONTH_PATTERN = (
    r"(?:jan(?:uary)?|feb(?:ruary)?|mar(?:ch)?|apr(?:il)?|may|jun(?:e)?|jul(?:y)?|"
    r"aug(?:ust)?|sep(?:t(?:ember)?)?|oct(?:ober)?|nov(?:ember)?|dec(?:ember)?)"
)
NUMBER_WORDS = {
    "zero": "0",
    "oh": "0",
    "o": "0",
    "one": "1",
    "two": "2",
    "three": "3",
    "four": "4",
    "five": "5",
    "six": "6",
    "seven": "7",
    "eight": "8",
    "nine": "9",
}


@dataclass(frozen=True)
class PendingEntity:
    entity_type: str
    raw_value: str
    start: int
    end: int
    confidence: float = 1.0


class SessionRedactor:
    def __init__(self) -> None:
        self._token_by_key: dict[tuple[str, str], str] = {}
        self._value_by_token: dict[str, str] = {}
        self._counts: dict[str, int] = defaultdict(int)

    def reset(self) -> None:
        self._token_by_key.clear()
        self._value_by_token.clear()
        self._counts.clear()

    def token_map(self) -> dict[str, str]:
        return dict(self._value_by_token)

    def redact(self, text: str) -> RedactionResult:
        pending = self._detect(text)
        entities = [
            DetectedEntity(
                entity_type=entity.entity_type,
                raw_value=entity.raw_value,
                token=self._token_for(entity.entity_type, entity.raw_value),
                start=entity.start,
                end=entity.end,
                confidence=entity.confidence,
            )
            for entity in pending
        ]
        redacted = self._apply_tokens(text, entities)
        token_map = {entity.token: self._value_by_token[entity.token] for entity in entities}
        return RedactionResult(
            raw_text=text,
            redacted_text=redacted,
            masked_prompt=redacted,
            entities=entities,
            token_map=token_map,
        )

    def _token_for(self, entity_type: str, raw_value: str) -> str:
        key = (entity_type, self._normalize(entity_type, raw_value))
        existing = self._token_by_key.get(key)
        if existing:
            return existing

        self._counts[entity_type] += 1
        token = f"{entity_type}_{self._counts[entity_type]}"
        self._token_by_key[key] = token
        self._value_by_token[token] = raw_value
        return token

    @staticmethod
    def _normalize(entity_type: str, raw_value: str) -> str:
        collapsed = " ".join(raw_value.strip().lower().split())
        if entity_type in {"SSN", "PHONE", "CARD", "GOV_ID"}:
            digits = re.sub(r"\D+", "", raw_value)
            return digits or collapsed
        return collapsed

    @staticmethod
    def _apply_tokens(text: str, entities: list[DetectedEntity]) -> str:
        if not entities:
            return text

        pieces: list[str] = []
        cursor = 0
        for entity in sorted(entities, key=lambda item: item.start):
            if entity.start < cursor:
                continue
            pieces.append(text[cursor:entity.start])
            pieces.append(entity.token)
            cursor = entity.end
        pieces.append(text[cursor:])
        return "".join(pieces)

    def _detect(self, text: str) -> list[PendingEntity]:
        candidates: list[PendingEntity] = []
        candidates.extend(self._known_value_matches(text))
        candidates.extend(self._regex_matches(text))
        candidates.extend(self._cue_based_names(text))
        candidates.extend(self._cue_based_addresses(text))
        candidates.extend(self._spoken_ssn(text))
        candidates.extend(self._spoken_phone(text))
        candidates.extend(self._spoken_card(text))
        candidates.extend(self._spoken_gov_id(text))
        return self._dedupe(candidates)

    def _known_value_matches(self, text: str) -> list[PendingEntity]:
        entities: list[PendingEntity] = []
        for token, raw_value in self._value_by_token.items():
            entity_type = token.rsplit("_", 1)[0]
            if len(raw_value.strip()) < 3:
                continue
            pattern = re.compile(rf"\b{re.escape(raw_value)}\b", re.IGNORECASE)
            for match in pattern.finditer(text):
                entities.append(
                    PendingEntity(
                        entity_type=entity_type,
                        raw_value=match.group(0),
                        start=match.start(),
                        end=match.end(),
                        confidence=0.72,
                    )
                )
        return entities

    def _regex_matches(self, text: str) -> list[PendingEntity]:
        patterns: list[tuple[str, str, int]] = [
            ("EMAIL", r"\b[\w.+-]+@[\w-]+\.[\w.-]+\b", 0),
            ("EMAIL", r"(?i)\b[\w.+-]+\s+at\s+[\w-]+\.[\w.-]+\b", 0),
            ("SSN", r"\b\d{3}-\d{2}-\d{4}\b", 0),
            (
                "SSN",
                r"(?i)(?:ssn|social security(?: number)?)\s*(?:is|=|:)?\s*((?:\d{3}-?\d{2}-?\d{4})|(?:\d{3}-?\d{6})|\d{9})\b",
                1,
            ),
            ("DOB", rf"(?i)\b{MONTH_PATTERN}\s+\d{{1,2}}(?:st|nd|rd|th)?(?:,)?\s+\d{{4}}\b", 0),
            ("DOB", r"\b(?:0?[1-9]|1[0-2])[/-](?:0?[1-9]|[12]\d|3[01])[/-](?:19|20)\d{2}\b", 0),
            ("PHONE", r"\b(?:\+?1[-.\s]?)?(?:\(?\d{3}\)?[-.\s]?)\d{3}[-.\s]?\d{4}\b", 0),
            (
                "PHONE",
                r"(?i)(?:phone number|call me at|you can call me at|mobile number|cell number)\s*(?:is|=|:)?\s*(\+?[\d][\d\s-]{6,}\d)\b",
                1,
            ),
            ("CARD", r"\b(?:\d{4}[,\s-]?){3}\d{4}\b", 0),
            ("CARD", r"(?i)(?:credit card|debit card|card number)\s*(?:is|=|:)?\s*([\d][\d,\s-]{10,}\d)\b", 1),
            (
                "ADDRESS",
                r"(?i)(?:address is|ship to|mail it to|live at|stay at|staying at|send the package to)\s+([0-9][^,.!?;\n]{6,80})",
                1,
            ),
            (
                "GOV_ID",
                r"(?i)(?:aadhaar|passport|driver'?s license|license number|government id|gov id)\s*(?:is|=|:|number)?\s*([A-Z0-9-]{6,18})",
                1,
            ),
        ]

        entities: list[PendingEntity] = []
        for entity_type, pattern, capture in patterns:
            for match in re.finditer(pattern, text):
                group = match.group(capture)
                start, end = match.span(capture)
                entities.append(
                    PendingEntity(
                        entity_type=entity_type,
                        raw_value=group.strip(),
                        start=start,
                        end=end,
                    )
                )
        return entities

    def _cue_based_names(self, text: str) -> list[PendingEntity]:
        cue_pattern = re.compile(
            r"(?i)\b(?:i am|i'm|my name is|this is|speaking is)\s+([A-Z][a-z]+(?:\s+[A-Z][a-z]+){0,2}|[A-Za-z]+(?:\s+[A-Za-z]+){0,2})"
        )
        entities: list[PendingEntity] = []
        for match in cue_pattern.finditer(text):
            candidate = match.group(1).strip()
            words = candidate.split()
            while words and words[-1].lower() in {"and", "but", "so", "with", "my"}:
                words.pop()
            candidate = " ".join(words)
            if not candidate:
                continue
            if len(candidate.split()) > 3:
                continue
            if candidate.lower() in {"here", "me", "doctor", "patient"}:
                continue
            raw_start, raw_end = match.span(1)
            start = raw_start
            end = raw_start + len(candidate)
            entities.append(PendingEntity("PERSON", candidate, start, end, confidence=0.8))
        return entities

    def _cue_based_addresses(self, text: str) -> list[PendingEntity]:
        address_pattern = re.compile(
            r"(?i)\b(?:i live at|i stay at|my address is|address is|mail it to|send the package to)\s+([0-9][^,.!?;\n]{6,80})"
        )
        entities: list[PendingEntity] = []
        for match in address_pattern.finditer(text):
            candidate = match.group(1).strip()
            start, end = match.span(1)
            entities.append(PendingEntity("ADDRESS", candidate, start, end, confidence=0.85))
        return entities

    def _spoken_ssn(self, text: str) -> list[PendingEntity]:
        return self._spoken_digits_after_cue(
            text=text,
            entity_type="SSN",
            cue_pattern=r"(?i)\b(?:ssn|social security(?: number)?|s\s*s\s*n|sn)\b",
            min_digits=5,
            max_digits=9,
        )

    def _spoken_phone(self, text: str) -> list[PendingEntity]:
        return self._spoken_digits_after_cue(
            text=text,
            entity_type="PHONE",
            cue_pattern=r"(?i)\b(?:phone number|call me at|number is|mobile number|cell number)\b",
            min_digits=7,
            max_digits=10,
        )

    def _spoken_card(self, text: str) -> list[PendingEntity]:
        return self._spoken_digits_after_cue(
            text=text,
            entity_type="CARD",
            cue_pattern=r"(?i)\b(?:credit card(?: number)?|debit card(?: number)?|card number)\b",
            min_digits=12,
            max_digits=16,
        )

    def _spoken_gov_id(self, text: str) -> list[PendingEntity]:
        return self._spoken_digits_after_cue(
            text=text,
            entity_type="GOV_ID",
            cue_pattern=r"(?i)\b(?:aadhaar|passport number|license number|government id|gov id)\b",
            min_digits=6,
            max_digits=12,
        )

    def _spoken_digits_after_cue(
        self,
        *,
        text: str,
        entity_type: str,
        cue_pattern: str,
        min_digits: int,
        max_digits: int,
    ) -> list[PendingEntity]:
        cues = re.finditer(cue_pattern, text)
        tokens = list(re.finditer(r"[A-Za-z0-9']+", text))
        entities: list[PendingEntity] = []

        for cue in cues:
            digit_tokens: list[re.Match[str]] = []
            for token in tokens:
                if token.start() < cue.end():
                    continue
                normalized = token.group(0).lower()
                if normalized in NUMBER_WORDS or normalized.isdigit():
                    digit_tokens.append(token)
                    if len(digit_tokens) >= max_digits:
                        break
                    continue
                if digit_tokens:
                    break
            if len(digit_tokens) < min_digits:
                continue
            start = digit_tokens[0].start()
            end = digit_tokens[-1].end()
            raw_value = text[start:end]
            entities.append(PendingEntity(entity_type, raw_value, start, end, confidence=0.75))
        return entities

    @staticmethod
    def _dedupe(entities: list[PendingEntity]) -> list[PendingEntity]:
        ordered = sorted(
            entities,
            key=lambda item: (item.start, -(item.end - item.start), -item.confidence, item.entity_type),
        )
        kept: list[PendingEntity] = []
        for entity in ordered:
            overlap = next(
                (
                    index
                    for index, existing in enumerate(kept)
                    if not (entity.end <= existing.start or entity.start >= existing.end)
                ),
                None,
            )
            if overlap is None:
                kept.append(entity)
                continue

            existing = kept[overlap]
            existing_len = existing.end - existing.start
            new_len = entity.end - entity.start
            if new_len > existing_len or entity.confidence > existing.confidence:
                kept[overlap] = entity

        return sorted(kept, key=lambda item: item.start)
