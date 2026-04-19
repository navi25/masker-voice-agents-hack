from __future__ import annotations

import threading
import uuid
from pathlib import Path
from typing import Callable

from .audio import MicrophoneAudioSource, WavAudioSource
from .events import EventBus, build_event, now_ms
from .gemma import StubGemmaClient
from .logging_safe import SafeLogger
from .models import RedactionResult, SafeLogEntry, SessionConfig, to_dict
from .redactor import SessionRedactor
from .stt import FasterWhisperTranscriber, Transcriber, normalize_text
from .vad import SpeechSegmenter, VadEvent


class DemoSessionManager:
    def __init__(
        self,
        *,
        safe_log_dir: str | Path,
        transcriber_factory: Callable[[str], Transcriber] | None = None,
        model_client: StubGemmaClient | None = None,
        source_factory: Callable[[SessionConfig], object] | None = None,
    ) -> None:
        self.event_bus = EventBus()
        self.safe_logger = SafeLogger(safe_log_dir)
        self.redactor = SessionRedactor()
        self.model_client = model_client or StubGemmaClient()
        self.transcriber_factory = transcriber_factory or self._default_transcriber_factory
        self.source_factory = source_factory or self._default_source_factory

        self._transcriber_cache: dict[str, Transcriber] = {}
        self._lock = threading.Lock()
        self._stop_event = threading.Event()
        self._worker: threading.Thread | None = None
        self._active_config: SessionConfig | None = None

    @property
    def is_running(self) -> bool:
        with self._lock:
            return self._worker is not None and self._worker.is_alive()

    @property
    def active_session_id(self) -> str | None:
        with self._lock:
            return self._active_config.session_id if self._active_config else None

    def subscribe(self):
        return self.event_bus.subscribe()

    def start(self, config: SessionConfig) -> dict[str, str]:
        with self._lock:
            if self._worker is not None and self._worker.is_alive():
                raise RuntimeError(f"session {self._active_config.session_id if self._active_config else ''} is already running")
            self.redactor.reset()
            self._stop_event.clear()
            self._active_config = config
            self._worker = threading.Thread(
                target=self._run_session,
                args=(config,),
                daemon=True,
                name=f"masker-demo-{config.session_id}",
            )
            self._worker.start()

        return {"session_id": config.session_id, "status": "started"}

    def stop(self) -> dict[str, str]:
        session_id = self.active_session_id
        self._stop_event.set()
        return {"session_id": session_id or "", "status": "stopping"}

    def wait(self, timeout: float | None = None) -> None:
        worker = self._worker
        if worker is not None:
            worker.join(timeout=timeout)

    def reset(self, session_id: str | None = None) -> dict[str, str]:
        active = self.active_session_id or session_id or f"ses_{uuid.uuid4().hex[:8]}"
        self._stop_event.set()
        self.wait(timeout=2.0)
        self.redactor.reset()
        self.event_bus.emit(build_event("session.reset", active))
        with self._lock:
            self._worker = None
            self._active_config = None
        return {"session_id": active, "status": "reset"}

    def _default_transcriber_factory(self, model_name: str) -> Transcriber:
        existing = self._transcriber_cache.get(model_name)
        if existing is not None:
            return existing
        transcriber = FasterWhisperTranscriber(model_name_or_path=model_name)
        self._transcriber_cache[model_name] = transcriber
        return transcriber

    @staticmethod
    def _default_source_factory(config: SessionConfig):
        if config.audio_mode == "replay":
            return WavAudioSource(
                path=config.audio_path or "",
                sample_rate=config.sample_rate,
                simulate_realtime=config.simulate_realtime,
            )
        return MicrophoneAudioSource(
            sample_rate=config.sample_rate,
            device=config.device,
        )

    def _run_session(self, config: SessionConfig) -> None:
        last_timestamp_ms = 0
        utterance_index = 0
        pending_utterance_id = "utt_1"
        partial_emit_ms = -1
        last_partial_text = ""

        self.event_bus.emit(
            build_event(
                "session.started",
                config.session_id,
                config=to_dict(config),
            )
        )

        try:
            source = self.source_factory(config)
            transcriber = self.transcriber_factory(config.stt_model)
            segmenter = SpeechSegmenter(sample_rate=config.sample_rate)

            for frame in source.frames(self._stop_event):
                if self._stop_event.is_set():
                    break

                last_timestamp_ms = frame.timestamp_ms
                vad_events = segmenter.process(frame.pcm, frame.timestamp_ms)
                utterance_duration_ms = int(len(segmenter.current_audio()) / 2 / config.sample_rate * 1000)

                if segmenter.triggered and utterance_duration_ms >= config.min_partial_ms:
                    if partial_emit_ms < 0 or frame.timestamp_ms - partial_emit_ms >= config.partial_interval_ms:
                        partial_text = normalize_text(
                            transcriber.transcribe_pcm16(
                                segmenter.current_audio(),
                                sample_rate=config.sample_rate,
                                language=config.language,
                            ).text
                        )
                        if partial_text and partial_text != last_partial_text:
                            last_partial_text = partial_text
                            partial_emit_ms = frame.timestamp_ms
                            redaction = self.redactor.redact(partial_text)
                            self.event_bus.emit(
                                build_event(
                                    "transcript.partial",
                                    config.session_id,
                                    utterance_id=pending_utterance_id,
                                    raw_text=redaction.raw_text,
                                    redacted_text=redaction.redacted_text,
                                    entities=[to_dict(entity) for entity in redaction.entities],
                                    is_final=False,
                                )
                            )
                            if redaction.entities:
                                self.event_bus.emit(
                                    build_event(
                                        "entities.detected",
                                        config.session_id,
                                        utterance_id=pending_utterance_id,
                                        entities=[to_dict(entity) for entity in redaction.entities],
                                        is_final=False,
                                    )
                                )

                for vad_event in vad_events:
                    if vad_event.type == "speech_started":
                        partial_emit_ms = -1
                        last_partial_text = ""
                        self.event_bus.emit(
                            build_event(
                                "audio.speech_started",
                                config.session_id,
                                utterance_id=pending_utterance_id,
                                start_ms=vad_event.start_ms,
                            )
                        )
                    elif vad_event.type == "speech_ended":
                        utterance_index += 1
                        utterance_id = f"utt_{utterance_index}"
                        pending_utterance_id = f"utt_{utterance_index + 1}"
                        self.event_bus.emit(
                            build_event(
                                "audio.speech_ended",
                                config.session_id,
                                utterance_id=utterance_id,
                                start_ms=vad_event.start_ms,
                                end_ms=vad_event.end_ms,
                                duration_ms=(vad_event.end_ms or 0) - (vad_event.start_ms or 0),
                            )
                        )
                        self._finalize_utterance(
                            config=config,
                            transcriber=transcriber,
                            utterance_id=utterance_id,
                            utterance_audio=vad_event.audio or b"",
                        )

            for vad_event in segmenter.flush(last_timestamp_ms):
                utterance_index += 1
                utterance_id = f"utt_{utterance_index}"
                self.event_bus.emit(
                    build_event(
                        "audio.speech_ended",
                        config.session_id,
                        utterance_id=utterance_id,
                        start_ms=vad_event.start_ms,
                        end_ms=vad_event.end_ms,
                        duration_ms=(vad_event.end_ms or 0) - (vad_event.start_ms or 0),
                    )
                )
                self._finalize_utterance(
                    config=config,
                    transcriber=transcriber,
                    utterance_id=utterance_id,
                    utterance_audio=vad_event.audio or b"",
                )

            reason = "stopped" if self._stop_event.is_set() else "completed"
            self.event_bus.emit(build_event("session.stopped", config.session_id, reason=reason))
        except Exception as exc:
            self.event_bus.emit(
                build_event(
                    "error",
                    config.session_id,
                    message=str(exc),
                )
            )
            self.event_bus.emit(build_event("session.stopped", config.session_id, reason="error"))
        finally:
            with self._lock:
                self._worker = None
                self._active_config = None
            self._stop_event.clear()

    def _finalize_utterance(
        self,
        *,
        config: SessionConfig,
        transcriber: Transcriber,
        utterance_id: str,
        utterance_audio: bytes,
    ) -> None:
        stt_result = transcriber.transcribe_pcm16(
            utterance_audio,
            sample_rate=config.sample_rate,
            language=config.language,
        )
        raw_text = normalize_text(stt_result.text)
        if not raw_text:
            return

        redaction = self.redactor.redact(raw_text)
        self.event_bus.emit(
            build_event(
                "transcript.final",
                config.session_id,
                utterance_id=utterance_id,
                raw_text=redaction.raw_text,
                redacted_text=redaction.redacted_text,
                entities=[to_dict(entity) for entity in redaction.entities],
                segments=[to_dict(segment) for segment in stt_result.segments],
                is_final=True,
            )
        )

        if redaction.entities:
            self.event_bus.emit(
                build_event(
                    "entities.detected",
                    config.session_id,
                    utterance_id=utterance_id,
                    entities=[to_dict(entity) for entity in redaction.entities],
                    entity_types=sorted({entity.entity_type for entity in redaction.entities}),
                    is_final=True,
                )
            )

        self.event_bus.emit(
            build_event(
                "model.input.ready",
                config.session_id,
                utterance_id=utterance_id,
                masked_prompt=redaction.masked_prompt,
                policy_mode=config.policy_mode,
                eligible_for_downstream=True,
            )
        )

        if config.no_model:
            model_output = {
                "model": "disabled",
                "text": "",
                "skipped": True,
                "reason": "no_model_mode",
            }
        else:
            model_output = self.model_client.generate(redaction.masked_prompt)
            model_output["skipped"] = False

        self.event_bus.emit(
            build_event(
                "model.output",
                config.session_id,
                utterance_id=utterance_id,
                **model_output,
            )
        )

        safe_entry = SafeLogEntry(
            session_id=config.session_id,
            utterance_id=utterance_id,
            policy_mode=config.policy_mode,
            redacted_text=redaction.redacted_text,
            masked_prompt=redaction.masked_prompt,
            entity_types=sorted({entity.entity_type for entity in redaction.entities}),
            entity_count=len(redaction.entities),
            timestamp_ms=now_ms(),
        )
        log_path = self.safe_logger.write(safe_entry)
        self.event_bus.emit(
            build_event(
                "log.safe_entry",
                config.session_id,
                utterance_id=utterance_id,
                safe_log=to_dict(safe_entry),
                log_path=str(log_path),
            )
        )


def default_session_id() -> str:
    return f"ses_{uuid.uuid4().hex[:8]}"
