from __future__ import annotations

from collections import deque
from dataclasses import dataclass


@dataclass
class VadEvent:
    type: str
    start_ms: int | None = None
    end_ms: int | None = None
    audio: bytes | None = None


class EnergyVadAdapter:
    def __init__(self, threshold: int = 500) -> None:
        self.threshold = threshold

    def is_speech(self, frame: bytes, sample_rate: int) -> bool:
        del sample_rate
        if not frame:
            return False
        amplitudes = memoryview(frame).cast("h")
        average = sum(abs(sample) for sample in amplitudes) / max(len(amplitudes), 1)
        return average >= self.threshold


class VadAdapter:
    def __init__(self, aggressiveness: int = 2) -> None:
        self._impl = None
        try:
            import webrtcvad

            self._impl = webrtcvad.Vad(aggressiveness)
        except ImportError:
            self._impl = EnergyVadAdapter()

    def is_speech(self, frame: bytes, sample_rate: int) -> bool:
        if hasattr(self._impl, "is_speech"):
            return bool(self._impl.is_speech(frame, sample_rate))
        return bool(self._impl.is_speech(frame, sample_rate))


class SpeechSegmenter:
    def __init__(
        self,
        sample_rate: int = 16000,
        frame_duration_ms: int = 30,
        padding_duration_ms: int = 300,
        aggressiveness: int = 2,
        start_ratio: float = 0.6,
        end_ratio: float = 0.8,
    ) -> None:
        self.sample_rate = sample_rate
        self.frame_duration_ms = frame_duration_ms
        self.num_padding_frames = max(1, padding_duration_ms // frame_duration_ms)
        self.start_ratio = start_ratio
        self.end_ratio = end_ratio
        self.detector = VadAdapter(aggressiveness=aggressiveness)

        self._triggered = False
        self._ring: deque[tuple[bytes, bool, int]] = deque(maxlen=self.num_padding_frames)
        self._current_frames: list[bytes] = []
        self._speech_start_ms: int | None = None

    @property
    def triggered(self) -> bool:
        return self._triggered

    def current_audio(self) -> bytes:
        return b"".join(self._current_frames)

    def process(self, frame: bytes, timestamp_ms: int) -> list[VadEvent]:
        is_speech = self.detector.is_speech(frame, self.sample_rate)
        events: list[VadEvent] = []

        if not self._triggered:
            self._ring.append((frame, is_speech, timestamp_ms))
            voiced = sum(1 for _, voiced_flag, _ in self._ring if voiced_flag)
            if len(self._ring) == self.num_padding_frames and voiced >= self.start_ratio * len(self._ring):
                self._triggered = True
                self._speech_start_ms = self._ring[0][2]
                self._current_frames = [buffer for buffer, _, _ in self._ring]
                self._ring.clear()
                events.append(VadEvent(type="speech_started", start_ms=self._speech_start_ms))
            return events

        self._current_frames.append(frame)
        self._ring.append((frame, is_speech, timestamp_ms))
        unvoiced = sum(1 for _, voiced_flag, _ in self._ring if not voiced_flag)
        if len(self._ring) == self.num_padding_frames and unvoiced >= self.end_ratio * len(self._ring):
            end_ms = timestamp_ms + self.frame_duration_ms
            events.append(
                VadEvent(
                    type="speech_ended",
                    start_ms=self._speech_start_ms,
                    end_ms=end_ms,
                    audio=b"".join(self._current_frames),
                )
            )
            self._triggered = False
            self._ring.clear()
            self._current_frames = []
            self._speech_start_ms = None
        return events

    def flush(self, end_ms: int) -> list[VadEvent]:
        if not self._current_frames:
            return []
        event = VadEvent(
            type="speech_ended",
            start_ms=self._speech_start_ms,
            end_ms=end_ms,
            audio=b"".join(self._current_frames),
        )
        self._triggered = False
        self._ring.clear()
        self._current_frames = []
        self._speech_start_ms = None
        return [event]
