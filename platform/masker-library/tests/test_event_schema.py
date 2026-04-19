from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest.mock import patch

from masker_demo.audio import AudioFrame
from masker_demo.models import SessionConfig, SttResult
from masker_demo.session import DemoSessionManager
from masker_demo.vad import VadEvent


class FakeSource:
    def __init__(self, frames: list[AudioFrame]) -> None:
        self._frames = frames

    def frames(self, stop_event):
        del stop_event
        for frame in self._frames:
            yield frame


class FakeTranscriber:
    def transcribe_pcm16(self, pcm: bytes, sample_rate: int, language: str | None = None) -> SttResult:
        del sample_rate, language
        text = "Hi, I'm Ravi Kumar and my SSN is 123-45-6789."
        if len(pcm) < 100:
            text = "Hi, I'm Ravi Kumar"
        return SttResult(text=text, segments=[])


class FakeSpeechSegmenter:
    def __init__(self, *args, **kwargs) -> None:
        del args, kwargs
        self.triggered = False
        self._current_audio = b""
        self._seen = 0

    def current_audio(self) -> bytes:
        return self._current_audio

    def process(self, frame: bytes, timestamp_ms: int):
        self._seen += 1
        if self._seen == 1:
            self.triggered = True
            self._current_audio = frame
            return [VadEvent(type="speech_started", start_ms=0)]
        self._current_audio += frame
        self.triggered = False
        return [VadEvent(type="speech_ended", start_ms=0, end_ms=timestamp_ms + 30, audio=self._current_audio)]

    def flush(self, end_ms: int):
        del end_ms
        return []


class EventSchemaTests(unittest.TestCase):
    def test_session_emits_safe_redacted_contract(self) -> None:
        frames = [
            AudioFrame(pcm=b"a" * 120, timestamp_ms=0),
            AudioFrame(pcm=b"b" * 120, timestamp_ms=30),
        ]

        with tempfile.TemporaryDirectory() as temp_dir:
            manager = DemoSessionManager(
                safe_log_dir=temp_dir,
                transcriber_factory=lambda model_name: FakeTranscriber(),
                source_factory=lambda config: FakeSource(frames),
            )
            subscription = manager.subscribe()
            config = SessionConfig(
                session_id="ses_test",
                audio_mode="replay",
                audio_path="fake.wav",
                stt_model="fake",
                no_model=True,
                partial_interval_ms=0,
                min_partial_ms=0,
                simulate_realtime=False,
            )

            with patch("masker_demo.session.SpeechSegmenter", FakeSpeechSegmenter):
                manager.start(config)
                manager.wait(timeout=2.0)

            events = list(subscription.replay)
            while not subscription.queue.empty():
                events.append(subscription.queue.get_nowait())
            manager.event_bus.unsubscribe(subscription)

            event_types = [event["type"] for event in events]
            self.assertIn("session.started", event_types)
            self.assertIn("transcript.partial", event_types)
            self.assertIn("transcript.final", event_types)
            self.assertIn("entities.detected", event_types)
            self.assertIn("model.input.ready", event_types)
            self.assertIn("model.output", event_types)
            self.assertIn("log.safe_entry", event_types)
            self.assertIn("session.stopped", event_types)

            model_input = next(event for event in events if event["type"] == "model.input.ready")
            self.assertIn("PERSON_1", model_input["masked_prompt"])
            self.assertIn("SSN_1", model_input["masked_prompt"])
            self.assertNotIn("Ravi Kumar", model_input["masked_prompt"])

            safe_log_event = next(event for event in events if event["type"] == "log.safe_entry")
            self.assertNotIn("Ravi Kumar", str(safe_log_event["safe_log"]))
            self.assertNotIn("123-45-6789", str(safe_log_event["safe_log"]))

            log_path = Path(safe_log_event["log_path"])
            contents = log_path.read_text(encoding="utf-8")
            self.assertIn("PERSON_1", contents)
            self.assertIn("SSN_1", contents)
            self.assertNotIn("Ravi Kumar", contents)
            self.assertNotIn("123-45-6789", contents)


if __name__ == "__main__":
    unittest.main()
