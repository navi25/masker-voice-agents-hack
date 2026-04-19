from __future__ import annotations

import contextlib
import time
import wave
from dataclasses import dataclass
from pathlib import Path
from threading import Event
from typing import Iterator


@dataclass
class AudioFrame:
    pcm: bytes
    timestamp_ms: int


class MicrophoneAudioSource:
    def __init__(self, sample_rate: int = 16000, frame_duration_ms: int = 30, device: str | int | None = None) -> None:
        self.sample_rate = sample_rate
        self.frame_duration_ms = frame_duration_ms
        self.frames_per_chunk = int(sample_rate * frame_duration_ms / 1000)
        self.device = device

    def frames(self, stop_event: Event) -> Iterator[AudioFrame]:
        try:
            import sounddevice as sd
        except ImportError as exc:
            raise RuntimeError(
                "sounddevice is required for live microphone capture. Install requirements.txt first."
            ) from exc

        stream = sd.RawInputStream(
            samplerate=self.sample_rate,
            blocksize=self.frames_per_chunk,
            channels=1,
            dtype="int16",
            device=self.device,
        )

        start = time.monotonic()
        with stream:
            while not stop_event.is_set():
                data, overflowed = stream.read(self.frames_per_chunk)
                if overflowed:
                    continue
                timestamp_ms = int((time.monotonic() - start) * 1000)
                yield AudioFrame(pcm=bytes(data), timestamp_ms=timestamp_ms)


class WavAudioSource:
    def __init__(
        self,
        path: str | Path,
        sample_rate: int = 16000,
        frame_duration_ms: int = 30,
        simulate_realtime: bool = True,
    ) -> None:
        self.path = Path(path)
        self.sample_rate = sample_rate
        self.frame_duration_ms = frame_duration_ms
        self.frames_per_chunk = int(sample_rate * frame_duration_ms / 1000)
        self.simulate_realtime = simulate_realtime

    def frames(self, stop_event: Event) -> Iterator[AudioFrame]:
        if not self.path.exists():
            raise FileNotFoundError(self.path)

        with contextlib.closing(wave.open(str(self.path), "rb")) as wav_file:
            if wav_file.getframerate() != self.sample_rate:
                raise RuntimeError(
                    f"Replay audio must be {self.sample_rate}Hz WAV. Got {wav_file.getframerate()}Hz from {self.path}."
                )
            if wav_file.getnchannels() != 1 or wav_file.getsampwidth() != 2:
                raise RuntimeError("Replay audio must be mono 16-bit PCM WAV.")

            timestamp_ms = 0
            while not stop_event.is_set():
                chunk = wav_file.readframes(self.frames_per_chunk)
                if not chunk:
                    break
                expected_bytes = self.frames_per_chunk * 2
                if len(chunk) < expected_bytes:
                    chunk = chunk + (b"\x00" * (expected_bytes - len(chunk)))
                yield AudioFrame(pcm=chunk, timestamp_ms=timestamp_ms)
                if self.simulate_realtime:
                    time.sleep(self.frame_duration_ms / 1000)
                timestamp_ms += self.frame_duration_ms
