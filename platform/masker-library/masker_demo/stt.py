from __future__ import annotations

import re
from typing import Protocol

from .models import SttResult, SttSegment


class Transcriber(Protocol):
    def transcribe_pcm16(self, pcm: bytes, sample_rate: int, language: str | None = None) -> SttResult:
        ...


class FasterWhisperTranscriber:
    def __init__(self, model_name_or_path: str = "small.en", device: str = "cpu", compute_type: str = "int8") -> None:
        self.model_name_or_path = model_name_or_path
        self.device = device
        self.compute_type = compute_type
        self._model = None

    def _ensure_model(self):
        if self._model is not None:
            return self._model
        try:
            from faster_whisper import WhisperModel
        except ImportError as exc:
            raise RuntimeError(
                "faster-whisper is required for transcription. Install requirements.txt first."
            ) from exc

        self._model = WhisperModel(
            self.model_name_or_path,
            device=self.device,
            compute_type=self.compute_type,
        )
        return self._model

    def transcribe_pcm16(self, pcm: bytes, sample_rate: int, language: str | None = None) -> SttResult:
        if not pcm:
            return SttResult(text="", segments=[], language=language)

        import numpy as np

        model = self._ensure_model()
        audio = np.frombuffer(pcm, dtype=np.int16).astype("float32") / 32768.0
        segments, info = model.transcribe(
            audio,
            language=language,
            vad_filter=False,
            beam_size=1,
            best_of=1,
            temperature=0.0,
            condition_on_previous_text=False,
        )

        collected_segments: list[SttSegment] = []
        texts: list[str] = []
        for segment in segments:
            text = normalize_text(segment.text)
            if not text:
                continue
            texts.append(text)
            collected_segments.append(
                SttSegment(
                    text=text,
                    start_ms=int(segment.start * 1000),
                    end_ms=int(segment.end * 1000),
                )
            )

        return SttResult(
            text=normalize_text(" ".join(texts)),
            segments=collected_segments,
            language=getattr(info, "language", language),
        )


def normalize_text(text: str) -> str:
    text = re.sub(r"\s+", " ", text).strip()
    return text
